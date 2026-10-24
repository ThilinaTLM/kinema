//! Private Linux worker for the GPU bridge proof, not a production mpv backend.
//! libmpv's public client/render ABI is dynamically loaded (mpv/client.h,
//! mpv/render.h and mpv/render_gl.h). No mpv source is copied here.
#![deny(unsafe_op_in_unsafe_fn)]

use crate::interop::Interop;
use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::PathBuf,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_LIVE_FRAMES: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecoderMode {
    #[default]
    Software,
    Vaapi,
}
impl DecoderMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Software => "no",
            Self::Vaapi => "vaapi",
        }
    }
}

fn validate_decoder(
    mode: DecoderMode,
    actual: &str,
    decoded: &str,
    output: &str,
    underlying: &str,
) -> Result<(), String> {
    if mode == DecoderMode::Vaapi
        && (actual != "vaapi"
            || decoded != "vaapi"
            || output != "vaapi"
            || !matches!(underlying, "nv12" | "p010"))
    {
        return Err(format!(
            "vaapi-capability: required decoder and decoded/output pixel formats=vaapi with nv12/p010 storage; observed decoder={actual}, decoded={decoded}, output={output}, underlying={underlying}"
        ));
    }
    Ok(())
}

/// Stable classifier prefix: environment selection is deliberately unsupported.
pub const UNQUALIFIED_HWDEC_ERROR: &str = "unqualified-capability: environment decoder overrides are unsupported; use the explicit qualification candidate CLI; environment must be unset or no";

fn validate_hwdec_policy(value: Option<&std::ffi::OsStr>) -> Result<(), &'static str> {
    match value {
        None => Ok(()),
        Some(value) if value == "no" => Ok(()),
        Some(_) => Err(UNQUALIFIED_HWDEC_ERROR),
    }
}

#[derive(Default)]
pub struct Stats {
    pub texture_frames: AtomicU64,
    pub gpu_buffer_copies: AtomicU64,
    /// Always zero: playback does not map rendered pixels (decoder may be software).
    pub cpu_pixel_maps: AtomicU64,
    pub dropped_frames: AtomicU64,
    pub live_frames: AtomicUsize,
    pub max_live_frames: AtomicUsize,
    retired_frames: AtomicUsize,
    /// Actual mpv hwdec-current; "no" means software decoding/upload by libmpv.
    pub decoder: Mutex<String>,
    pub requested_decoder: Mutex<String>,
    pub codec: Mutex<String>,
    pub hw_pixel_format: Mutex<String>,
    pub decoder_pixel_format: Mutex<String>,
    pub output_pixel_format: Mutex<String>,
    pub source_size: Mutex<Option<[u32; 2]>>,
    pub render_size: Mutex<[u32; 2]>,
}
#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub texture_frames: u64,
    pub gpu_buffer_copies: u64,
    pub cpu_pixel_maps: u64,
    pub dropped_frames: u64,
    pub max_live_frames: usize,
    pub elapsed: Duration,
    pub decoder: String,
    pub requested_decoder: String,
    pub codec: String,
    pub hw_pixel_format: String,
    pub decoder_pixel_format: String,
    pub output_pixel_format: String,
    pub source_size: Option<[u32; 2]>,
    pub render_size: [u32; 2],
}
impl Stats {
    pub fn summary(&self, elapsed: Duration) -> Summary {
        Summary {
            texture_frames: self.texture_frames.load(Ordering::Relaxed),
            gpu_buffer_copies: self.gpu_buffer_copies.load(Ordering::Relaxed),
            cpu_pixel_maps: self.cpu_pixel_maps.load(Ordering::Relaxed),
            dropped_frames: self.dropped_frames.load(Ordering::Relaxed),
            max_live_frames: self.max_live_frames.load(Ordering::Relaxed),
            elapsed,
            decoder: self
                .decoder
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            requested_decoder: self
                .requested_decoder
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            codec: self.codec.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            hw_pixel_format: self
                .hw_pixel_format
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            decoder_pixel_format: self
                .decoder_pixel_format
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            output_pixel_format: self
                .output_pixel_format
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            source_size: *self.source_size.lock().unwrap_or_else(|e| e.into_inner()),
            render_size: *self.render_size.lock().unwrap_or_else(|e| e.into_inner()),
        }
    }
}

struct Permit(Arc<Stats>);
impl Permit {
    fn acquire(stats: &Arc<Stats>) -> Option<Self> {
        stats
            .live_frames
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                (live < MAX_LIVE_FRAMES).then_some(live + 1)
            })
            .ok()
            .map(|previous| {
                stats
                    .max_live_frames
                    .fetch_max(previous + 1, Ordering::Relaxed);
                Self(stats.clone())
            })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        // Drop may run on GPUI: defer permit reclamation until the worker
        // has completed outstanding presentation submissions, never wait here.
        self.0.retired_frames.fetch_add(1, Ordering::AcqRel);
    }
}

pub struct ProducedFrame {
    pub texture: wgpu::Texture,
    pub size: [u32; 2],
    pub sequence: u64,
    // Last field: permit released after texture handle is dropped. Copy GPU work
    // has already completed before publication. UI may keep Arc for its lifetime.
    _permit: Permit,
}

pub struct Producer {
    pub receiver: Receiver<Result<Arc<ProducedFrame>, String>>,
    pub stop: Arc<AtomicBool>,
    pub stats: Arc<Stats>,
    worker: Option<JoinHandle<Result<Summary, String>>>,
}
impl Producer {
    #[allow(
        dead_code,
        reason = "software-only entry point retained for standalone harnesses"
    )]
    pub fn spawn(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        queue_gate: Arc<Mutex<()>>,
        media: PathBuf,
        size: [u32; 2],
    ) -> Self {
        Self::spawn_with_decoder(
            device,
            queue,
            queue_gate,
            media,
            size,
            DecoderMode::Software,
        )
    }
    pub fn spawn_with_decoder(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        queue_gate: Arc<Mutex<()>>,
        media: PathBuf,
        size: [u32; 2],
        decoder: DecoderMode,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Stats {
            requested_decoder: Mutex::new(decoder.as_str().into()),
            render_size: Mutex::new(size),
            ..Stats::default()
        });
        let worker_stop = stop.clone();
        let worker_stats = stats.clone();
        let worker = thread::spawn(move || {
            let result = run(
                (device, queue, queue_gate),
                media,
                (size, decoder),
                &sender,
                &worker_stop,
                &worker_stats,
            );
            if let Err(error) = &result {
                // Retain a terminal error while a single previously published
                // frame is pending; stop/disconnect always cancels this loop.
                let mut message = Err(error.clone());
                loop {
                    match sender.try_send(message) {
                        Ok(()) | Err(TrySendError::Disconnected(_)) => break,
                        Err(TrySendError::Full(value)) => {
                            message = value;
                        }
                    }
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(2));
                }
            }
            result
        });
        Self {
            receiver,
            stop,
            stats,
            worker: Some(worker),
        }
    }
    /// Signals stop and joins. Call outside GPUI's UI thread.
    pub fn join(mut self) -> Result<Summary, String> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .ok_or("producer already joined")?
            .join()
            .map_err(|_| "GPU producer worker panicked".to_string())?
    }
}
impl Drop for Producer {
    fn drop(&mut self) {
        // Never synchronously join from Drop: the owner may be on the UI thread.
        self.stop.store(true, Ordering::Release);
    }
}

#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
impl Param {
    fn new<T>(kind: c_int, value: &mut T) -> Self {
        Self {
            kind,
            data: ptr::from_mut(value).cast(),
        }
    }
    fn end() -> Self {
        Self {
            kind: 0,
            data: ptr::null_mut(),
        }
    }
}
#[repr(C)]
struct GlInit {
    get_proc: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    context: *mut c_void,
}
#[repr(C)]
struct Fbo {
    fbo: c_int,
    width: c_int,
    height: c_int,
    internal_format: c_int,
}
#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
    playlist_insert_id: i64,
    playlist_insert_num_entries: c_int,
}

type GetProc = unsafe extern "C" fn(*const c_char) -> *const c_void;
unsafe extern "C" fn resolve(context: *mut c_void, name: *const c_char) -> *mut c_void {
    // Box-owned resolver remains stable for the entire mpv render lifetime.
    unsafe { (*(context.cast::<GetProc>()))(name).cast_mut() }
}

#[repr(C)]
struct DrmParams {
    fd: c_int,
    crtc_id: c_int,
    connector_id: c_int,
    atomic_request_ptr: *mut *mut c_void,
    render_fd: c_int,
}

struct Mpv {
    _library: Library,
    _get_proc: Box<GetProc>,
    _drm: Option<Box<DrmParams>>,
    _render_node: Option<std::fs::File>,
    handle: *mut c_void,
    render: *mut c_void,
    terminate: unsafe extern "C" fn(*mut c_void),
    render_free: unsafe extern "C" fn(*mut c_void),
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render_frame: unsafe extern "C" fn(*mut c_void, *mut Param) -> c_int,
    wait: unsafe extern "C" fn(*mut c_void, f64) -> *mut Event,
    get_string: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char,
    free: unsafe extern "C" fn(*mut c_void),
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
}
impl Mpv {
    fn new(interop: &Interop, media: PathBuf, decoder: DecoderMode) -> Result<Self, String> {
        // Local path only, never interpreted as a URL or command fragment.
        let media = media
            .canonicalize()
            .map_err(|e| format!("local media: {e}"))?;
        if !media.is_file() {
            return Err("media must be a regular local file".into());
        }
        use std::os::unix::ffi::OsStrExt;
        let path = CString::new(media.as_os_str().as_bytes()).map_err(|_| "NUL in media path")?;
        unsafe {
            let library = Library::new("libmpv.so.2").map_err(|e| format!("load libmpv: {e}"))?;
            let version: unsafe extern "C" fn() -> std::os::raw::c_ulong = *library
                .get(b"mpv_client_api_version\0")
                .map_err(|e| e.to_string())?;
            let version = version();
            if version >> 16 != 2 {
                return Err(format!(
                    "unsupported libmpv client API major {} (requires 2)",
                    version >> 16
                ));
            }
            let create: unsafe extern "C" fn() -> *mut c_void =
                *library.get(b"mpv_create\0").map_err(|e| e.to_string())?;
            let option: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int =
                *library
                    .get(b"mpv_set_option_string\0")
                    .map_err(|e| e.to_string())?;
            let initialize: unsafe extern "C" fn(*mut c_void) -> c_int = *library
                .get(b"mpv_initialize\0")
                .map_err(|e| e.to_string())?;
            let render_create: unsafe extern "C" fn(
                *mut *mut c_void,
                *mut c_void,
                *mut Param,
            ) -> c_int = *library
                .get(b"mpv_render_context_create\0")
                .map_err(|e| e.to_string())?;
            let command: unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int =
                *library.get(b"mpv_command\0").map_err(|e| e.to_string())?;
            let mut this = Self {
                terminate: *library
                    .get(b"mpv_terminate_destroy\0")
                    .map_err(|e| e.to_string())?,
                render_free: *library
                    .get(b"mpv_render_context_free\0")
                    .map_err(|e| e.to_string())?,
                render_update: *library
                    .get(b"mpv_render_context_update\0")
                    .map_err(|e| e.to_string())?,
                render_frame: *library
                    .get(b"mpv_render_context_render\0")
                    .map_err(|e| e.to_string())?,
                wait: *library
                    .get(b"mpv_wait_event\0")
                    .map_err(|e| e.to_string())?,
                get_string: *library
                    .get(b"mpv_get_property_string\0")
                    .map_err(|e| e.to_string())?,
                free: *library.get(b"mpv_free\0").map_err(|e| e.to_string())?,
                error_string: *library
                    .get(b"mpv_error_string\0")
                    .map_err(|e| e.to_string())?,
                _library: library,
                _get_proc: Box::new(interop.egl.get_proc),
                _drm: None,
                _render_node: None,
                handle: ptr::null_mut(),
                render: ptr::null_mut(),
            };
            this.handle = create();
            if this.handle.is_null() {
                return Err("mpv_create failed".into());
            }
            // Qualified baseline only: software decoding uploads source frames.
            // The rendered-frame GL→wgpu bridge remains GPU-resident; this is
            // deliberately not a claim that the entire pipeline avoids the CPU.
            for (name, value) in [
                ("config", "no"),
                ("load-scripts", "no"),
                ("audio", "no"),
                ("hwdec", decoder.as_str()),
                ("vo", "libmpv"),
                ("terminal", "no"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("osc", "no"),
                ("ytdl", "no"),
                ("idle", "yes"),
            ] {
                let name = CString::new(name).map_err(|e| e.to_string())?;
                let value = CString::new(value).map_err(|e| e.to_string())?;
                this.check(
                    option(this.handle, name.as_ptr(), value.as_ptr()),
                    "mpv option",
                )?;
            }
            if decoder == DecoderMode::Vaapi {
                let node = interop.egl.vaapi_render_node()?;
                use std::os::{
                    fd::AsRawFd,
                    unix::fs::{FileTypeExt, MetadataExt},
                };
                let expected = std::fs::metadata(&node)
                    .map_err(|_| "vaapi-capability: render node metadata unavailable")?;
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&node)
                    .map_err(|_| "vaapi-capability: cannot open matched render node")?;
                let opened = file
                    .metadata()
                    .map_err(|_| "vaapi-capability: opened render node metadata unavailable")?;
                if !opened.file_type().is_char_device() || opened.rdev() != expected.rdev() {
                    return Err("vaapi-capability: render node changed while opening".into());
                }
                this._drm = Some(Box::new(DrmParams {
                    fd: -1,
                    crtc_id: 0,
                    connector_id: 0,
                    atomic_request_ptr: ptr::null_mut(),
                    render_fd: file.as_raw_fd(),
                }));
                this._render_node = Some(file);
                let node = CString::new(node.as_os_str().as_bytes())
                    .map_err(|_| "vaapi-capability: invalid render node")?;
                for (name, value) in [
                    (c"gpu-hwdec-interop", c"vaapi"),
                    (c"hwdec-software-fallback", c"no"),
                    (c"vaapi-device", node.as_c_str()),
                ] {
                    this.check(
                        option(this.handle, name.as_ptr(), value.as_ptr()),
                        "vaapi-capability: mpv option",
                    )?;
                }
            }
            this.check(initialize(this.handle), "mpv initialize")?;
            let mut init = GlInit {
                get_proc: resolve,
                context: ptr::from_mut(&mut *this._get_proc).cast(),
            };
            let mut params = vec![
                Param {
                    kind: 1,
                    data: c"opengl".as_ptr().cast_mut().cast(),
                },
                Param::new(2, &mut init),
            ];
            if let Some(drm) = &mut this._drm {
                params.push(Param::new(16, &mut **drm));
            }
            params.push(Param::end());
            let status = render_create(&mut this.render, this.handle, params.as_mut_ptr());
            this.check(status, "mpv OpenGL render context")?;
            this.check(
                command(
                    this.handle,
                    [c"loadfile".as_ptr(), path.as_ptr(), ptr::null()].as_ptr(),
                ),
                "mpv load local media",
            )?;
            Ok(this)
        }
    }
    fn check(&self, status: c_int, operation: &str) -> Result<(), String> {
        if status >= 0 {
            Ok(())
        } else {
            Err(format!(
                "{operation}: {}",
                unsafe { CStr::from_ptr((self.error_string)(status)) }.to_string_lossy()
            ))
        }
    }
    fn property(&self, name: &CStr) -> String {
        unsafe {
            let value = (self.get_string)(self.handle, name.as_ptr());
            if value.is_null() {
                return "unavailable".into();
            }
            let text = CStr::from_ptr(value).to_string_lossy().into_owned();
            (self.free)(value.cast());
            text
        }
    }
    fn render(&self, interop: &Interop, size: [u32; 2]) -> Result<(), String> {
        // Native synthetic readback verifies FLIP_Y=0 puts source top row in
        // the first PBO row, matching wgpu/GPUI texture coordinates.
        let mut fbo = Fbo {
            fbo: interop.fbo as c_int,
            width: size[0] as c_int,
            height: size[1] as c_int,
            internal_format: 0x8058,
        };
        let mut flip = 0 as c_int;
        let mut params = [
            Param::new(3, &mut fbo),
            Param::new(4, &mut flip),
            Param::end(),
        ];
        self.check(
            unsafe { (self.render_frame)(self.render, params.as_mut_ptr()) },
            "mpv render",
        )
    }
    fn ended(&self) -> Result<bool, String> {
        unsafe {
            loop {
                let event = (self.wait)(self.handle, 0.0);
                if event.is_null() {
                    return Err("null mpv event".into());
                }
                match (*event).id {
                    0 => return Ok(false),
                    1 => return Ok(true),
                    7 => {
                        if (*event).data.is_null() {
                            return Err("missing mpv end-file data".into());
                        }
                        let end = &*(*event).data.cast::<EndFile>();
                        self.check(end.error, "mpv end-file")?;
                        return Ok(true);
                    }
                    _ => {}
                }
            }
        }
    }
}
impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe {
            if !self.render.is_null() {
                (self.render_free)(self.render);
            }
            if !self.handle.is_null() {
                (self.terminate)(self.handle);
            }
        }
    }
}

fn run(
    gpu: (Arc<wgpu::Device>, Arc<wgpu::Queue>, Arc<Mutex<()>>),
    media: PathBuf,
    render: ([u32; 2], DecoderMode),
    sender: &SyncSender<Result<Arc<ProducedFrame>, String>>,
    stop: &AtomicBool,
    stats: &Arc<Stats>,
) -> Result<Summary, String> {
    // Reject unqualified modes before EGL allocation or any libmpv initialization.
    validate_hwdec_policy(std::env::var_os("KINEMA_GPU_EXPERIMENT_HWDEC").as_deref())?;
    let start = Instant::now();
    let (size, decoder) = render;
    let (device, queue, queue_gate) = gpu;
    let mut interop = Interop::new(device, queue, queue_gate, size)?;
    let result = (|| {
        // Declared inside closure: drops while EGL is current, before shutdown.
        let mpv = Mpv::new(&interop, media, decoder)?;
        let first_decode_deadline = Instant::now() + Duration::from_secs(10);
        let mut sequence = 0;
        let mut decoded_seen = false;
        while !stop.load(Ordering::Acquire) {
            if decoder == DecoderMode::Vaapi
                && sequence == 0
                && Instant::now() >= first_decode_deadline
            {
                return Err(if decoded_seen {
                    "vaapi-capability: decoded frame observed but required hardware/output metadata unavailable after 10s".into()
                } else {
                    "vaapi-startup: no decoded-frame dimensions and decoder metadata after 10s"
                        .into()
                });
            }
            if mpv.ended()? {
                break;
            }
            // Retired frame handles may still be referenced by an in-flight GPUI
            // submission. Only the worker waits before recycling their permits.
            let retired = stats.retired_frames.swap(0, Ordering::AcqRel);
            if retired != 0 {
                interop.complete_submissions()?;
                stats.live_frames.fetch_sub(retired, Ordering::AcqRel);
            }
            let Some(permit) = Permit::acquire(stats) else {
                thread::sleep(Duration::from_millis(2));
                continue;
            };
            let update = unsafe { (mpv.render_update)(mpv.render) };
            if update & 1 == 0 {
                drop(permit);
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            mpv.render(&interop, size)?;
            // Check every rendered frame before publication; requests alone are
            // not evidence, and copy/software fallback is never accepted.
            let actual = mpv.property(c"hwdec-current");
            let decoded = mpv.property(c"video-dec-params/pixelformat");
            let output = mpv.property(c"video-out-params/pixelformat");
            let hw_format = mpv.property(c"video-dec-params/hw-pixelformat");
            let width = mpv
                .property(c"video-dec-params/w")
                .parse::<u32>()
                .ok()
                .filter(|&n| n > 0);
            let height = mpv
                .property(c"video-dec-params/h")
                .parse::<u32>()
                .ok()
                .filter(|&n| n > 0);
            let available = |value: &str| !value.is_empty() && value != "unavailable";
            *stats.decoder.lock().unwrap_or_else(|e| e.into_inner()) = actual.clone();
            *stats
                .decoder_pixel_format
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = decoded.clone();
            *stats
                .output_pixel_format
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = output.clone();
            *stats
                .hw_pixel_format
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = hw_format.clone();
            *stats.codec.lock().unwrap_or_else(|e| e.into_inner()) =
                mpv.property(c"current-tracks/video/codec");
            *stats.source_size.lock().unwrap_or_else(|e| e.into_inner()) =
                width.zip(height).map(|(w, h)| [w, h]);
            let source_ready =
                width.zip(height).is_some() && available(&actual) && available(&decoded);
            decoded_seen |= source_ready;
            // Acknowledge initial repaint through render(), but never publish a
            // blank startup frame. A valid decoded software/copy identity fails
            // immediately, even while output metadata is still initializing.
            if decoder == DecoderMode::Vaapi
                && source_ready
                && (actual != "vaapi" || decoded != "vaapi")
            {
                validate_decoder(decoder, &actual, &decoded, &output, &hw_format)?;
            }
            if decoder == DecoderMode::Vaapi
                && sequence == 0
                && (!source_ready || !available(&output) || !available(&hw_format))
            {
                drop(permit);
                thread::sleep(Duration::from_millis(2));
                continue;
            }
            if decoder == DecoderMode::Vaapi && !source_ready {
                return Err(
                    "vaapi-capability: decoded-frame metadata disappeared before publication"
                        .into(),
                );
            }
            validate_decoder(decoder, &actual, &decoded, &output, &hw_format)?;
            let texture = interop.copy_frame()?;
            sequence += 1;
            stats.gpu_buffer_copies.fetch_add(1, Ordering::Relaxed);
            stats.texture_frames.fetch_add(1, Ordering::Relaxed);
            let frame = Arc::new(ProducedFrame {
                texture,
                size,
                sequence,
                _permit: permit,
            });
            match sender.try_send(Ok(frame)) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    stats.dropped_frames.fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
        if decoder == DecoderMode::Vaapi && sequence == 0 {
            return Err("vaapi-capability: no verified decoded frame was published".into());
        }
        Ok(stats.summary(start.elapsed()))
    })();
    let shutdown = interop.shutdown();
    match (result, shutdown) {
        (Ok(summary), Ok(())) => Ok(summary),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(shutdown)) => Err(format!("{error}; shutdown: {shutdown}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hwdec_policy_accepts_only_unset_or_no() {
        use std::ffi::OsStr;
        assert_eq!(validate_hwdec_policy(None), Ok(()));
        assert_eq!(validate_hwdec_policy(Some(OsStr::new("no"))), Ok(()));
        for value in ["auto", "yes", "vaapi", "vulkan-copy", "", "NO"] {
            assert_eq!(
                validate_hwdec_policy(Some(OsStr::new(value))),
                Err(UNQUALIFIED_HWDEC_ERROR)
            );
        }
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            validate_hwdec_policy(Some(OsStr::from_bytes(&[0xff]))),
            Err(UNQUALIFIED_HWDEC_ERROR)
        );
    }
    #[test]
    fn explicit_decoder_policy_rejects_copy_and_software_fallback() {
        assert_eq!(DecoderMode::default(), DecoderMode::Software);
        assert!(
            validate_decoder(
                DecoderMode::Software,
                "no",
                "yuv420p",
                "yuv420p",
                "unavailable"
            )
            .is_ok()
        );
        for storage in ["nv12", "p010"] {
            assert!(
                validate_decoder(DecoderMode::Vaapi, "vaapi", "vaapi", "vaapi", storage).is_ok()
            );
        }
        for actual in ["no", "vaapi-copy", "vulkan-copy", "unavailable", ""] {
            assert!(
                validate_decoder(DecoderMode::Vaapi, actual, "vaapi", "vaapi", "nv12").is_err()
            );
        }
        for (decoded, output, storage) in [
            ("nv12", "vaapi", "nv12"),
            ("vaapi", "nv12", "nv12"),
            ("unavailable", "vaapi", "nv12"),
            ("vaapi", "vaapi", "unavailable"),
            ("vaapi", "vaapi", "rgba"),
        ] {
            assert!(
                validate_decoder(DecoderMode::Vaapi, "vaapi", decoded, output, storage).is_err()
            );
        }
        // The same validator is applied before every frame, including switches.
        assert!(validate_decoder(DecoderMode::Vaapi, "vaapi", "vaapi", "vaapi", "nv12").is_ok());
        assert!(
            validate_decoder(
                DecoderMode::Vaapi,
                "no",
                "yuv420p",
                "yuv420p",
                "unavailable"
            )
            .is_err()
        );
    }
    #[test]
    fn permits_are_bounded_and_reclaimed() {
        let stats = Arc::new(Stats::default());
        let a = Permit::acquire(&stats).unwrap();
        let b = Permit::acquire(&stats).unwrap();
        let c = Permit::acquire(&stats).unwrap();
        assert!(Permit::acquire(&stats).is_none());
        assert_eq!(stats.max_live_frames.load(Ordering::Relaxed), 3);
        drop(b);
        assert!(Permit::acquire(&stats).is_none()); // not reusable before GPU completion
        let retired = stats.retired_frames.swap(0, Ordering::AcqRel);
        stats.live_frames.fetch_sub(retired, Ordering::AcqRel); // synthetic completion
        assert!(Permit::acquire(&stats).is_some());
        drop(a);
        drop(c);
        let retired = stats.retired_frames.swap(0, Ordering::AcqRel);
        stats.live_frames.fetch_sub(retired, Ordering::AcqRel);
        assert_eq!(stats.live_frames.load(Ordering::Relaxed), 0);
    }
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        struct WakeThread(thread::Thread);
        impl std::task::Wake for WakeThread {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(Arc::new(WakeThread(thread::current())));
        let mut context = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => thread::park(),
            }
        }
    }

    /// Explicit encoder parameters: no encoder substitution or playback fallback.
    /// AV1 fixtures require FFmpeg's libaom-av1 with 8/10-bit support; encoding
    /// is software-only fixture preparation, never a playback decoding fallback.
    #[derive(Clone, Copy)]
    struct NativeCodec {
        encoder: &'static str,
        codec: &'static str,
        pixel_format: &'static str,
        encoder_args: &'static [&'static str],
    }
    const FFV1: NativeCodec = NativeCodec {
        encoder: "ffv1",
        codec: "ffv1",
        pixel_format: "yuv444p",
        encoder_args: &[],
    };
    const H264: NativeCodec = NativeCodec {
        encoder: "libx264",
        codec: "h264",
        pixel_format: "yuv420p",
        encoder_args: &[
            "-preset",
            "ultrafast",
            "-profile:v",
            "high",
            "-x264-params",
            "cabac=1:8x8dct=1",
        ],
    };
    const HEVC: NativeCodec = NativeCodec {
        encoder: "libx265",
        codec: "hevc",
        pixel_format: "yuv420p",
        encoder_args: &[
            "-preset",
            "ultrafast",
            "-x265-params",
            "pools=2:frame-threads=2:log-level=error",
        ],
    };
    const HEVC10: NativeCodec = NativeCodec {
        pixel_format: "yuv420p10le",
        ..HEVC
    };
    const AV1: NativeCodec = NativeCodec {
        encoder: "libaom-av1",
        codec: "av1",
        pixel_format: "yuv420p",
        encoder_args: &["-cpu-used", "8", "-row-mt", "1", "-crf", "24", "-b:v", "0"],
    };
    const AV1_10: NativeCodec = NativeCodec {
        pixel_format: "yuv420p10le",
        ..AV1
    };

    /// TEST-ONLY GPU readback: verifies real mpv/PBO/Vulkan pixels and vertical
    /// orientation. This mapping path is deliberately not used by playback.
    #[test]
    #[ignore = "requires Intel Mesa Vulkan, patched HAL external semaphores, EGL, libmpv and ffmpeg"]
    fn native_synthetic_orientation_readback() {
        native_readback(FFV1, DecoderMode::Software, [64, 64]);
    }

    // Run each dedicated test exclusively (--ignored --exact --test-threads=1),
    // with Intel Mesa Vulkan/VAAPI, patched HAL, EGL, libmpv, validation layers,
    // and FFmpeg built with the named encoder and requested pixel format.
    // Missing encoder, VAAPI codec/profile/resolution, or runtime support FAILS;
    // ignored means opt-in, not a successful capability skip. All cases are SDR.
    macro_rules! vaapi_readback_test {
        ($name:ident, $codec:expr, $size:expr) => {
            #[test]
            #[ignore = "exclusive native Intel VAAPI SDR qualification; requires matching FFmpeg encoder, hardware codec/profile/size, Vulkan validation and TEST ONLY readback"]
            fn $name() {
                native_readback($codec, DecoderMode::Vaapi, $size);
            }
        };
    }
    vaapi_readback_test!(native_vaapi_h264_1080p, H264, [1920, 1080]);
    vaapi_readback_test!(native_vaapi_hevc_1080p, HEVC, [1920, 1080]);
    vaapi_readback_test!(native_vaapi_hevc10_1080p, HEVC10, [1920, 1080]);
    vaapi_readback_test!(native_vaapi_h264_4k, H264, [3840, 2160]);
    vaapi_readback_test!(native_vaapi_hevc_4k, HEVC, [3840, 2160]);
    vaapi_readback_test!(native_vaapi_hevc10_4k, HEVC10, [3840, 2160]);
    vaapi_readback_test!(native_vaapi_av1_1080p, AV1, [1920, 1080]);
    vaapi_readback_test!(native_vaapi_av1_10_1080p, AV1_10, [1920, 1080]);
    vaapi_readback_test!(native_vaapi_av1_4k, AV1, [3840, 2160]);
    vaapi_readback_test!(native_vaapi_av1_10_4k, AV1_10, [3840, 2160]);

    #[test]
    fn native_fixture_codec_parameters_are_explicit_sdr() {
        for codec in [H264, HEVC, HEVC10, AV1, AV1_10] {
            assert!(matches!(codec.pixel_format, "yuv420p" | "yuv420p10le"));
            assert!(!codec.encoder_args.is_empty());
        }
        assert_eq!(HEVC10.codec, "hevc");
        assert_eq!(AV1.codec, "av1");
        assert_eq!(AV1_10.codec, "av1");
        assert_eq!(AV1.encoder, "libaom-av1");
        assert_eq!(AV1_10.encoder, AV1.encoder);
        assert_eq!(AV1.pixel_format, "yuv420p");
        assert_eq!(AV1_10.pixel_format, "yuv420p10le");
        assert_eq!(AV1_10.encoder_args, AV1.encoder_args);
    }

    fn native_readback(codec: NativeCodec, decoder: DecoderMode, size: [u32; 2]) {
        assert!(size[0] > 0 && size[0] <= 3840 && size[0].is_multiple_of(2));
        assert!(size[1] > 0 && size[1] <= 2160 && size[1].is_multiple_of(2));
        let pixel_format = codec.pixel_format;
        // Run this ignored native test exclusively: the process-wide validator
        // must observe device creation, submissions, and complete teardown.
        crate::validation::install();
        assert_eq!(crate::validation::problem_count(), 0);
        // The generated manifest lives under target/gpui-mpv-gpu. Never write
        // fixtures to configuration directories or outside generated targets.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "kinema-gpu-smoke-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let media = root.join("synthetic.mkv");
        // Match qualify.py's explicit limited-range BT.709 SDR conversion:
        // lavfi colors originate in BT.601 YUV; tags alone would mislabel them.
        // Main10 here is 10-bit SDR, deliberately not an HDR qualification.
        let pattern = format!(
            "color=c=red:s={}x{}:r=10,drawbox=x=0:y={}:w={}:h={}:color=blue:t=fill,scale=in_color_matrix=bt601:out_color_matrix=bt709:in_range=tv:out_range=tv,format={},setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
            size[0],
            size[1],
            size[1] / 2,
            size[0],
            size[1] / 2,
            pixel_format
        );
        let mut ffmpeg = std::process::Command::new("ffmpeg");
        ffmpeg.args([
            "-nostdin",
            "-v",
            "error",
            "-filter_threads",
            "2",
            "-f",
            "lavfi",
            "-i",
            &pattern,
            "-t",
            "2",
            "-c:v",
            codec.encoder,
            "-threads",
            "2",
            "-pix_fmt",
            pixel_format,
            "-color_range",
            "tv",
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
        ]);
        ffmpeg.args(codec.encoder_args);
        // Bound both generated media (20 frames at 10 fps) and encoder wall
        // time. A missing/unsupported encoder is a failure, never a test skip.
        ffmpeg.args(["-frames:v", "20"]);
        let mut child = ffmpeg.arg(&media).spawn().unwrap_or_else(|error| {
            panic!("fixture encoder {} unavailable: {error}", codec.encoder)
        });
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "fixture encoder {} ({pixel_format}, {size:?}) failed: {status}; no substitution permitted",
                    codec.encoder
                );
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "fixture encoder {} ({pixel_format}, {size:?}) exceeded 120 seconds",
                    codec.encoder
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).unwrap();
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        let gate = Arc::new(Mutex::new(()));
        let producer = Producer::spawn_with_decoder(
            device.clone(),
            queue.clone(),
            gate.clone(),
            media,
            size,
            decoder,
        );
        let (row, bytes) = crate::interop::layout(size).unwrap();
        // Even a failing assertion must stop/join the native worker before the
        // test unwinds and tears down process-wide graphics loader state.
        let readback = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut verified = false;
            while Instant::now() < deadline {
                let frame = producer
                    .receiver
                    .recv_timeout(Duration::from_secs(15))
                    .unwrap()
                    .unwrap();
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("TEST ONLY pixel readback"),
                    size: bytes,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = device.create_command_encoder(&Default::default());
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &frame.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(row),
                            rows_per_image: Some(size[1]),
                        },
                    },
                    frame.texture.size(),
                );
                let command = encoder.finish();
                let submission = {
                    let _gate = gate.lock().unwrap();
                    queue.submit([command])
                };
                let (tx, rx) = mpsc::sync_channel(1);
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = tx.send(result);
                    });
                device
                    .poll(wgpu::PollType::Wait {
                        submission_index: Some(submission),
                        timeout: Some(Duration::from_secs(10)),
                    })
                    .unwrap();
                rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
                {
                    let pixels = buffer.slice(..).get_mapped_range();
                    let top_offset =
                        ((size[1] / 4) as usize) * (row as usize) + (size[0] / 2) as usize * 4;
                    let bottom_offset =
                        ((size[1] * 3 / 4) as usize) * (row as usize) + (size[0] / 2) as usize * 4;
                    let top = &pixels[top_offset..][..4];
                    let bottom = &pixels[bottom_offset..][..4];
                    if top[0] > 180 && top[2] < 60 && bottom[2] > 180 && bottom[0] < 60 {
                        verified = true;
                    }
                    if top[2] > 180 && bottom[0] > 180 {
                        panic!("synthetic native frame is vertically inverted");
                    }
                }
                buffer.unmap();
                if verified {
                    break;
                }
            }
            verified
        }));
        let observed = producer.stats.clone();
        let summary = producer.join().unwrap_or_else(|error| {
            panic!("{error}; observed={:?}", observed.summary(Duration::ZERO))
        });
        let verified = match readback {
            Ok(verified) => verified,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        assert!(
            verified,
            "synthetic top-red/bottom-blue pixels never arrived"
        );
        assert!(summary.gpu_buffer_copies > 0);
        assert_eq!(summary.texture_frames, summary.gpu_buffer_copies);
        assert_eq!(
            summary.cpu_pixel_maps, 0,
            "playback counter excludes this explicitly test-only readback"
        );
        assert!(summary.max_live_frames <= MAX_LIVE_FRAMES);
        assert_eq!(summary.requested_decoder, decoder.as_str());
        assert_eq!(summary.source_size, Some(size));
        assert_eq!(summary.render_size, size);
        assert_eq!(summary.codec, codec.codec);
        if decoder == DecoderMode::Vaapi {
            assert_eq!(summary.decoder, "vaapi");
            assert_eq!(summary.decoder_pixel_format, "vaapi");
            assert_eq!(summary.output_pixel_format, "vaapi");
            assert_eq!(
                summary.hw_pixel_format,
                if pixel_format == "yuv420p10le" {
                    "p010"
                } else {
                    "nv12"
                }
            );
        }
        drop(queue);
        drop(device);
        drop(adapter);
        drop(instance);
        assert_eq!(
            crate::validation::problem_count(),
            0,
            "native validation reported problems during creation, rendering, or shutdown"
        );
        eprintln!(
            "TEST ONLY native synthetic pixel/orientation readback verified (not playback CPU maps): {summary:?}"
        );
    }

    #[test]
    fn bridge_counter_does_not_claim_cpu_free_decode() {
        let stats = Stats::default();
        assert_eq!(stats.summary(Duration::ZERO).cpu_pixel_maps, 0);
        assert_eq!(stats.summary(Duration::ZERO).gpu_buffer_copies, 0);
    }
}
