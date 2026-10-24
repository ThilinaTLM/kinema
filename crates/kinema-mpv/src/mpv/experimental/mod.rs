//! Opt-in GPU-resident rendering on the existing libmpv client.
use super::{ffi::Param, library::Client};
use crate::{Error, Result};
#[cfg(not(target_os = "macos"))]
use std::sync::Mutex;
use std::{
    ffi::{CStr, c_char, c_void},
    sync::Arc,
    thread::{self, Thread},
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(test, target_os = "linux"))]
mod native_tests;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux::Interop as NativeInterop;
#[cfg(target_os = "macos")]
use macos::NativeInterop;
#[cfg(target_os = "macos")]
use objc2_core_foundation::CFRetained;
#[cfg(target_os = "macos")]
use objc2_core_video::CVPixelBuffer;
#[cfg(target_os = "windows")]
use windows::NativeInterop;

#[derive(Clone)]
pub struct ExperimentalGpuContext {
    #[cfg(not(target_os = "macos"))]
    pub(super) device: Arc<wgpu::Device>,
    #[cfg(not(target_os = "macos"))]
    pub(super) queue: Arc<wgpu::Queue>,
    #[cfg(not(target_os = "macos"))]
    pub(super) queue_gate: Arc<Mutex<()>>,
    #[cfg(target_os = "macos")]
    pub(super) registry_id: u64,
}
impl std::fmt::Debug for ExperimentalGpuContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExperimentalGpuContext")
            .finish_non_exhaustive()
    }
}
impl ExperimentalGpuContext {
    #[cfg(not(target_os = "macos"))]
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        queue_gate: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            device,
            queue,
            queue_gate,
        }
    }
    #[cfg(target_os = "macos")]
    pub fn new_metal(registry_id: u64) -> Self {
        Self { registry_id }
    }
}

pub(super) enum NativeFrame {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    Texture(wgpu::Texture),
    #[cfg(target_os = "macos")]
    PixelBuffer(CFRetained<CVPixelBuffer>),
}
struct FrameOwner {
    native: NativeFrame,
    size: [u32; 2],
}
// CVPixelBuffer is a retained immutable published image here. Native writes
// finish before publication; clones only retain/release the CoreVideo owner.
// No pixel mapping or mutation API is exposed by this wrapper.
#[cfg(target_os = "macos")]
unsafe impl Send for FrameOwner {}
#[cfg(target_os = "macos")]
unsafe impl Sync for FrameOwner {}

/// Immutable native surface. Clones retain the same bounded worker-owned slot.
#[derive(Clone)]
pub struct ExperimentalFrame(Arc<FrameOwner>);
impl ExperimentalFrame {
    pub fn size(&self) -> [u32; 2] {
        self.0.size
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub fn texture(&self) -> &wgpu::Texture {
        let NativeFrame::Texture(texture) = &self.0.native;
        texture
    }
    #[cfg(target_os = "macos")]
    pub fn pixel_buffer(&self) -> CFRetained<CVPixelBuffer> {
        let NativeFrame::PixelBuffer(buffer) = &self.0.native;
        buffer.clone()
    }
}

#[repr(C)]
struct OpenGlInit {
    resolver: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    userdata: *mut c_void,
}
#[repr(C)]
struct Fbo {
    fbo: i32,
    w: i32,
    h: i32,
    internal_format: i32,
}
#[repr(C)]
struct DrmParams {
    fd: i32,
    crtc_id: i32,
    connector_id: i32,
    atomic_request_ptr: *mut *mut c_void,
    render_fd: i32,
}
unsafe extern "C" fn resolve(data: *mut c_void, name: *const c_char) -> *mut c_void {
    if data.is_null() || name.is_null() {
        return std::ptr::null_mut();
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        (&*data.cast::<NativeInterop>())
            .get_proc_address(CStr::from_ptr(name))
            .cast_mut()
    }))
    .unwrap_or(std::ptr::null_mut())
}
unsafe extern "C" fn wake(data: *mut c_void) {
    if !data.is_null() {
        unsafe { &*data.cast::<Thread>() }.unpark();
    }
}
struct MpvContext {
    client: Arc<Client>,
    ptr: *mut super::ffi::RenderContext,
    native: Box<NativeInterop>,
    wake: Box<Thread>,
    drm: Option<Box<DrmParams>>,
    #[cfg(target_os = "windows")]
    queue_gate: Arc<Mutex<()>>,
}
impl MpvContext {
    /// ANGLE may flush D3D11On12 work from inside libmpv's GL entrypoints.
    /// Only Windows shares the UI's native queue. No native interop method or
    /// explicit completion wait may run inside this guard. libmpv render itself
    /// can still audio-time its call (potentially blocking the UI queue ~40ms).
    fn render_call<T>(&mut self, call: impl FnOnce(&mut Self) -> T) -> Result<T> {
        #[cfg(target_os = "windows")]
        {
            let gate = self.queue_gate.clone();
            with_queue_gate(&gate, || call(self)).map_err(Error::Gpu)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(call(self))
        }
    }
    fn free(&mut self) -> std::result::Result<(), String> {
        if !self.ptr.is_null() {
            self.render_call(|mpv| {
                unsafe {
                    (mpv.client.loaded.api.render_callback)(mpv.ptr, None, std::ptr::null_mut());
                    (mpv.client.loaded.api.render_free)(mpv.ptr);
                }
                mpv.ptr = std::ptr::null_mut();
            })
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[cfg(any(target_os = "windows", test))]
fn with_queue_gate<T>(
    gate: &std::sync::Mutex<()>,
    call: impl FnOnce() -> T,
) -> std::result::Result<T, String> {
    let _guard = gate
        .lock()
        .map_err(|_| "Windows GPU queue gate poisoned; refusing libmpv GL work".to_owned())?;
    Ok(call())
}
/// Owns all native refs together so uncertain teardown can quarantine all of them.
pub(super) struct Renderer {
    mpv: Option<Box<MpvContext>>,
    retained: Vec<Arc<FrameOwner>>,
    evidence_wait: EvidenceWait,
}
impl Renderer {
    pub fn new(client: Arc<Client>, context: &ExperimentalGpuContext) -> Result<Self> {
        let native = Box::new(NativeInterop::new(context).map_err(Error::Gpu)?);
        let mut this = Self {
            mpv: Some(Box::new(MpvContext {
                client,
                ptr: std::ptr::null_mut(),
                native,
                wake: Box::new(thread::current()),
                drm: None,
                #[cfg(target_os = "windows")]
                queue_gate: context.queue_gate.clone(),
            })),
            retained: Vec::with_capacity(3),
            evidence_wait: EvidenceWait::default(),
        };
        let mpv = this.mpv.as_mut().unwrap();
        let mut init = OpenGlInit {
            resolver: resolve,
            userdata: (&mut *mpv.native as *mut NativeInterop).cast(),
        };
        let mut params = vec![
            Param::new(1, c"opengl".as_ptr().cast_mut().cast()),
            Param::new(2, (&mut init as *mut OpenGlInit).cast()),
        ];
        if let Some(fd) = mpv.native.drm_render_fd() {
            mpv.drm = Some(Box::new(DrmParams {
                fd: -1,
                crtc_id: 0,
                connector_id: 0,
                atomic_request_ptr: std::ptr::null_mut(),
                render_fd: fd,
            }));
            params.push(Param::new(
                16,
                (&mut **mpv.drm.as_mut().unwrap() as *mut DrmParams).cast(),
            ));
        }
        params.push(Param::end());
        let code = mpv.render_call(|mpv| unsafe {
            (mpv.client.loaded.api.render_create)(
                &mut mpv.ptr,
                mpv.client.handle,
                params.as_mut_ptr(),
            )
        })?;
        mpv.client
            .loaded
            .check(code, "create experimental GPU render context")?;
        if mpv.ptr.is_null() {
            return Err(Error::Gpu("null experimental render context".into()));
        }
        mpv.render_call(|mpv| unsafe {
            (mpv.client.loaded.api.render_callback)(
                mpv.ptr,
                Some(wake),
                (&*mpv.wake as *const Thread).cast_mut().cast(),
            );
        })?;
        Ok(this)
    }
    pub fn update(&self) -> u64 {
        let mpv = self.mpv.as_ref().unwrap();
        unsafe { (mpv.client.loaded.api.render_update)(mpv.ptr) }
    }
    pub fn skip(&mut self) -> Result<()> {
        let mpv = self.mpv.as_mut().unwrap();
        let mut skip = 1i32;
        let mut params = [Param::new(13, (&mut skip as *mut i32).cast()), Param::end()];
        let code = mpv.render_call(|mpv| unsafe {
            (mpv.client.loaded.api.render)(mpv.ptr, params.as_mut_ptr())
        })?;
        mpv.client.loaded.check(code, "skip experimental frame")
    }
    pub fn evidence_ready(&mut self, generation: u64, evidence: &[String; 5]) -> Result<bool> {
        if !self
            .evidence_wait
            .ready(generation, evidence, std::time::Instant::now())?
        {
            return Ok(false);
        }
        validate_direct(
            &evidence[0],
            &evidence[1],
            &evidence[2],
            &evidence[3],
            &evidence[4],
        )?;
        Ok(true)
    }
    pub fn render(&mut self, size: [u32; 2]) -> Result<Option<ExperimentalFrame>> {
        let mpv = self.mpv.as_mut().unwrap();
        // Snapshot liveness BEFORE waiting. A UI owner can submit and release a
        // frame during the wait; that frame must survive until a later pass.
        // Wait only on this worker, never while holding the shared queue gate.
        retire_completed_frames(&mut self.retained, || mpv.native.complete_submissions())
            .map_err(Error::Gpu)?;
        if self.retained.len() == 3 {
            self.skip()?;
            return Ok(None);
        }
        let mpv = self.mpv.as_mut().unwrap();
        let fbo = mpv.native.begin_frame(size).map_err(Error::Gpu)?;
        let mut fbo = Fbo {
            fbo,
            w: size[0] as i32,
            h: size[1] as i32,
            internal_format: 0,
        };
        let mut flip = 0i32;
        let mut params = [
            Param::new(3, (&mut fbo as *mut Fbo).cast()),
            Param::new(4, (&mut flip as *mut i32).cast()),
            Param::end(),
        ];
        let code = mpv.render_call(|mpv| unsafe {
            (mpv.client.loaded.api.render)(mpv.ptr, params.as_mut_ptr())
        })?;
        mpv.client
            .loaded
            .check(code, "render experimental GPU frame")?;
        let native = mpv.native.finish_frame().map_err(Error::Gpu)?;
        let frame = ExperimentalFrame(Arc::new(FrameOwner { native, size }));
        self.retained.push(frame.0.clone());
        Ok(Some(frame))
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(mut mpv) = self.mpv.take()
            && !finish_teardown(
                &mut *mpv,
                |mpv| mpv.native.complete_submissions(),
                MpvContext::free,
                |mpv| mpv.native.shutdown(),
            )
        {
            std::mem::forget(mpv);
            std::mem::forget(std::mem::take(&mut self.retained));
        }
    }
}

/// A unique owner cannot be resurrected: FrameOwner is private and no Weak
/// handles are exposed. Thus UI submissions preceding its last external drop
/// are covered by the completion wait following this snapshot. Never rescan
/// liveness after waiting, nor mutate retained owners when completion fails.
fn retire_completed_frames<T>(
    retained: &mut Vec<Arc<T>>,
    complete: impl FnOnce() -> std::result::Result<(), String>,
) -> std::result::Result<(), String> {
    let candidates: Vec<bool> = retained
        .iter()
        .map(|owner| Arc::strong_count(owner) == 1)
        .collect();
    complete()?;
    let mut candidates = candidates.into_iter();
    retained.retain(|_| !candidates.next().unwrap());
    Ok(())
}

// False means the entire owner graph must be quarantined, not dropped.
fn finish_teardown<T>(
    owner: &mut T,
    complete: impl FnOnce(&T) -> std::result::Result<(), String>,
    free: impl FnOnce(&mut T) -> std::result::Result<(), String>,
    shutdown: impl FnOnce(&mut T) -> std::result::Result<(), String>,
) -> bool {
    if complete(owner).is_err() {
        return false;
    }
    if free(owner).is_err() {
        return false;
    }
    shutdown(owner).is_ok()
}

pub(super) fn hwdec() -> &'static str {
    if cfg!(target_os = "linux") {
        "vaapi"
    } else if cfg!(target_os = "windows") {
        "d3d11va"
    } else {
        "videotoolbox"
    }
}

#[derive(Default)]
struct EvidenceWait(Option<(u64, std::time::Instant)>);
impl EvidenceWait {
    fn ready(
        &mut self,
        generation: u64,
        evidence: &[String; 5],
        now: std::time::Instant,
    ) -> Result<bool> {
        let started = match self.0 {
            Some((old, started)) if old == generation => started,
            _ => {
                self.0 = Some((generation, now));
                now
            }
        };
        if evidence
            .iter()
            .any(|value| value.is_empty() || value == "unavailable")
        {
            if now.duration_since(started) >= std::time::Duration::from_secs(5) {
                return Err(Error::Gpu(
                    "direct decode evidence unavailable after five seconds".into(),
                ));
            }
            return Ok(false);
        }
        Ok(true)
    }
}

// FFmpeg libavutil/pixdesc.c identifies AV_PIX_FMT_VIDEOTOOLBOX as
// videotoolbox_vld and modern AV_PIX_FMT_D3D11 as d3d11. Legacy
// d3d11va_vld is deliberately not accepted by this bridge.
fn validate_direct(
    actual: &str,
    decoded: &str,
    output: &str,
    storage: &str,
    gamma: &str,
) -> Result<()> {
    let native = if cfg!(target_os = "linux") {
        "vaapi"
    } else if cfg!(target_os = "windows") {
        "d3d11"
    } else {
        "videotoolbox_vld"
    };
    if actual != hwdec()
        || decoded != native
        || output != native
        || !matches!(storage, "nv12" | "p010" | "p010le")
    {
        return Err(Error::Gpu(format!(
            "direct hardware surface required: hwdec={actual}, decoded={decoded}, output={output}, storage={storage}"
        )));
    }
    if !matches!(
        gamma,
        "bt.1886" | "bt.709" | "srgb" | "gamma2.2" | "gamma2.8"
    ) {
        return Err(Error::Gpu(format!(
            "experimental SDR only; unqualified transfer function: {gamma}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_released_during_completion_wait_is_not_retired() {
        let mut retained = vec![Arc::new(())];
        let external = retained[0].clone();
        retire_completed_frames(&mut retained, || {
            drop(external);
            Ok(())
        })
        .unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(Arc::strong_count(&retained[0]), 1);
        retire_completed_frames(&mut retained, || Ok(())).unwrap();
        assert!(retained.is_empty());
    }
    #[test]
    fn unique_frame_is_released_only_after_positive_completion() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct ObserveDrop(Arc<AtomicBool>);
        impl Drop for ObserveDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let mut retained = vec![Arc::new(ObserveDrop(dropped.clone()))];
        retire_completed_frames(&mut retained, || {
            assert!(!dropped.load(Ordering::SeqCst));
            Ok(())
        })
        .unwrap();
        assert!(retained.is_empty());
        assert!(dropped.load(Ordering::SeqCst));
    }
    #[test]
    fn failed_completion_preserves_all_retained_owners_for_quarantine() {
        let mut retained = vec![Arc::new(1), Arc::new(2)];
        let external = retained[1].clone();
        assert!(
            retire_completed_frames(&mut retained, || {
                drop(external);
                Err("completion uncertain".into())
            })
            .is_err()
        );
        assert_eq!(retained.len(), 2);
        assert_eq!(*retained[0], 1);
        assert_eq!(*retained[1], 2);
        assert!(retained.iter().all(|owner| Arc::strong_count(owner) == 1));
    }
    #[test]
    fn queue_gate_covers_only_the_call_and_poison_refuses_work() {
        let gate = std::sync::Mutex::new(());
        with_queue_gate(&gate, || assert!(gate.try_lock().is_err())).unwrap();
        assert!(gate.try_lock().is_ok());
        let _ = std::panic::catch_unwind(|| {
            let _guard = gate.lock().unwrap();
            panic!("simulate queue user panic");
        });
        assert!(with_queue_gate(&gate, || panic!("poisoned gate must refuse work")).is_err());
    }
    #[test]
    fn failed_mpv_free_refuses_native_shutdown() {
        assert!(!finish_teardown(
            &mut (),
            |_| Ok(()),
            |_| Err("queue gate poisoned".into()),
            |_| panic!("native shutdown after failed mpv free")
        ));
    }
    #[test]
    fn uncertain_completion_never_frees_mpv_and_shutdown_failure_quarantines() {
        let mut freed = false;
        assert!(!finish_teardown(
            &mut freed,
            |_| Err("timeout".into()),
            |freed| {
                *freed = true;
                Ok(())
            },
            |_| panic!("shutdown before completion")
        ));
        assert!(!freed);
        assert!(!finish_teardown(
            &mut freed,
            |_| Ok(()),
            |freed| {
                *freed = true;
                Ok(())
            },
            |_| Err("shutdown uncertain".into())
        ));
        assert!(freed);
        assert!(finish_teardown(
            &mut freed,
            |_| Ok(()),
            |freed| {
                *freed = true;
                Ok(())
            },
            |_| Ok(())
        ));
    }
    #[test]
    fn evidence_wait_is_bounded_and_generation_scoped() {
        let mut wait = EvidenceWait::default();
        let now = std::time::Instant::now();
        let missing = Default::default();
        assert!(!wait.ready(1, &missing, now).unwrap());
        assert!(
            wait.ready(1, &missing, now + std::time::Duration::from_secs(5))
                .is_err()
        );
        assert!(
            !wait
                .ready(2, &missing, now + std::time::Duration::from_secs(5))
                .unwrap()
        );
        assert!(
            wait.ready(
                2,
                &std::array::from_fn(|_| "present".into()),
                now + std::time::Duration::from_secs(6)
            )
            .unwrap()
        );
    }
    #[test]
    fn rejects_software_copy_and_unknown_surfaces() {
        for (actual, decoded, output, storage) in [
            ("no", "yuv420p", "yuv420p", "nv12"),
            ("vaapi-copy", "vaapi", "vaapi", "nv12"),
            (hwdec(), "unknown", "unknown", "nv12"),
            (hwdec(), "vaapi", "vaapi", "yuv444p"),
        ] {
            assert!(validate_direct(actual, decoded, output, storage, "bt.1886").is_err());
        }
    }
    #[test]
    fn platform_native_precision_and_hdr_rejection() {
        let native = if cfg!(target_os = "linux") {
            "vaapi"
        } else if cfg!(target_os = "windows") {
            "d3d11"
        } else {
            "videotoolbox_vld"
        };
        for storage in ["nv12", "p010", "p010le"] {
            assert!(validate_direct(hwdec(), native, native, storage, "bt.1886").is_ok());
        }
        for gamma in ["pq", "hlg", "", "unknown"] {
            assert!(validate_direct(hwdec(), native, native, "nv12", gamma).is_err());
        }
    }
}
