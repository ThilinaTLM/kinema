//! Exclusive Linux native Player tests. Run only in the generated patched app
//! build, one exact ignored test per process, with --test-threads=1. These are
//! capability assertions, never successful skips. Lead supplies validation-layer
//! environment/log collection. Windows/macOS and HDR are not qualified here.
//!
//! The sole pixel mapping below is explicitly TEST ONLY. Playback readback
//! counters must remain zero; each Player owns only its ordinary client/workers.
use crate::{
    Config, ExperimentalFrame, ExperimentalGpuContext, HardwareDecoding, PlaybackFailure,
    PlaybackState, Player, RenderBackend, ResolutionPolicy,
};
use std::{
    fs::{self, File},
    future::Future,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static EXCLUSIVE: Mutex<()> = Mutex::new(());
const HD: [u32; 2] = [1920, 1080];
const UHD: [u32; 2] = [3840, 2160];
const WAIT: Duration = Duration::from_secs(20);

fn block_on<F: Future>(future: F) -> F::Output {
    struct WakeThread(thread::Thread);
    impl Wake for WakeThread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(WakeThread(thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    let deadline = Instant::now() + WAIT;
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                assert!(Instant::now() < deadline, "native GPU future timed out");
                thread::park_timeout(Duration::from_millis(10));
            }
        }
    }
}

struct Gpu {
    // Retain the instance/adapter until all Players, frames and device work end.
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    gate: Arc<Mutex<()>>,
    errors: Arc<Mutex<Vec<String>>>,
}
impl Gpu {
    fn new() -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::VALIDATION | wgpu::InstanceFlags::DEBUG,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .expect("native capability: Vulkan adapter required");
        let info = adapter.get_info();
        assert_eq!(info.backend, wgpu::Backend::Vulkan);
        assert_eq!(
            info.vendor, 0x8086,
            "native capability: Intel Mesa required: {info:?}"
        );
        assert_ne!(
            info.device_type,
            wgpu::DeviceType::Cpu,
            "software adapter rejected"
        );
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("native capability: Vulkan device required");
        let errors = Arc::new(Mutex::new(Vec::new()));
        let observed = errors.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            observed.lock().unwrap().push(error.to_string());
        }));
        Self {
            _instance: instance,
            _adapter: adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            gate: Arc::new(Mutex::new(())),
            errors,
        }
    }
    fn player(&self) -> Player {
        Player::new(Config {
            experimental_gpu: Some(ExperimentalGpuContext::new(
                self.device.clone(), self.queue.clone(), self.gate.clone(),
            )),
            render_backend: RenderBackend::Gpu,
            hardware_decoding: HardwareDecoding::Auto,
            resolution: ResolutionPolicy::Source,
            initial_volume: 0.0,
            ..Config::default()
        })
        .expect("native capability: Player requires matching EGL/Vulkan UUID, external FD memory/semaphores and direct VAAPI; use the isolated patched build")
    }
    fn assert_clean(&self) {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(WAIT),
            })
            .expect("native test final device retirement");
        let errors = self.errors.lock().unwrap();
        assert!(errors.is_empty(), "wgpu validation errors: {errors:?}");
    }

    /// TEST ONLY: four one-pixel copies, never a playback feature or fallback.
    /// Buffer maps are deliberately separate from Player readback diagnostics.
    fn corners(&self, frame: &ExperimentalFrame) -> [[u8; 4]; 4] {
        let [width, height] = frame.size();
        assert_eq!(frame.texture().width(), width);
        assert_eq!(frame.texture().height(), height);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("TEST ONLY native Player corner orientation readback"),
            size: 4 * 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("TEST ONLY copy four native Player corners"),
            });
        for (i, [x, y]) in [
            [width / 4, height / 4],
            [width * 3 / 4, height / 4],
            [width / 4, height * 3 / 4],
            [width * 3 / 4, height * 3 / 4],
        ]
        .into_iter()
        .enumerate()
        {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: frame.texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: i as u64 * 256,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
        let commands = encoder.finish();
        let submission = {
            let _gate = self.gate.lock().unwrap();
            self.queue.submit([commands])
        };
        let (tx, rx) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        // Never hold the shared gate during fence/poll/map waits.
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(WAIT),
            })
            .expect("TEST ONLY corner copy completion");
        rx.recv_timeout(WAIT)
            .expect("TEST ONLY map callback timeout")
            .expect("TEST ONLY corner map failed");
        let pixels = {
            let mapped = buffer.slice(..).get_mapped_range();
            std::array::from_fn(|i| mapped[i * 256..i * 256 + 4].try_into().unwrap())
        };
        buffer.unmap();
        pixels
    }
    fn verified_frame(&self, player: &Player, size: [u32; 2]) -> ExperimentalFrame {
        let deadline = Instant::now() + WAIT;
        let mut last = None;
        loop {
            assert_healthy(player);
            if let Some(frame) = player.take_gpu_frame() {
                // After observing new source metadata, reject any stale old-size
                // publication rather than silently discarding it in the test.
                assert_eq!(frame.size(), size, "stale/source-scaled GPU frame");
                let pixels = self.corners(&frame);
                if oriented(pixels) {
                    assert_direct(player, size);
                    return frame;
                }
                last = Some(pixels);
            }
            assert!(
                Instant::now() < deadline,
                "native pixels absent/wrong orientation: {last:?}; state={:?}",
                player.state()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn oriented(p: [[u8; 4]; 4]) -> bool {
    let [red, green, blue, white] = p;
    red[0] > 180
        && red[1] < 80
        && red[2] < 80
        && green[1] > 150
        && green[0] < 90
        && green[2] < 90
        && blue[2] > 180
        && blue[0] < 80
        && blue[1] < 80
        && white[..3].iter().all(|&n| n > 180)
        && p.iter().all(|rgba| rgba[3] > 240)
}

#[test]
fn corner_predicate_rejects_vertical_and_horizontal_inversion() {
    let red = [255, 0, 0, 255];
    let green = [0, 255, 0, 255];
    let blue = [0, 0, 255, 255];
    let white = [255; 4];
    assert!(oriented([red, green, blue, white]));
    assert!(!oriented([blue, white, red, green]));
    assert!(!oriented([green, red, white, blue]));
    assert!(!oriented([[0; 4]; 4]));
}

#[derive(Clone, Copy)]
struct Codec {
    name: &'static str,
    encoder: &'static str,
    pixels: &'static str,
    args: &'static [&'static str],
}
const H264: Codec = Codec {
    name: "h264",
    encoder: "libx264",
    pixels: "yuv420p",
    args: &[
        "-preset",
        "ultrafast",
        "-profile:v",
        "high",
        "-x264-params",
        "cabac=1:8x8dct=1",
    ],
};
const HEVC: Codec = Codec {
    name: "hevc",
    encoder: "libx265",
    pixels: "yuv420p",
    args: &[
        "-preset",
        "ultrafast",
        "-x265-params",
        "pools=2:frame-threads=2:log-level=error",
    ],
};
const HEVC10: Codec = Codec {
    pixels: "yuv420p10le",
    ..HEVC
};
const AV1: Codec = Codec {
    name: "av1",
    encoder: "libaom-av1",
    pixels: "yuv420p",
    args: &["-cpu-used", "8", "-row-mt", "1", "-crf", "24", "-b:v", "0"],
};
const AV1_10: Codec = Codec {
    pixels: "yuv420p10le",
    ..AV1
};

// Metadata-only HDR gate stimulus: the source pattern remains SDR, NOT an HDR
// visual qualification. Untagged deliberately omits both filter and encoder tags.
#[derive(Clone, Copy, Debug)]
enum Metadata {
    Bt709,
    Untagged,
    PqTestOnly,
}
impl Metadata {
    fn tags(self) -> &'static [&'static str] {
        match self {
            Self::Bt709 => &[
                "-colorspace",
                "bt709",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709",
            ],
            Self::Untagged => &[],
            Self::PqTestOnly => &[
                "-colorspace",
                "bt709",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "smpte2084",
            ],
        }
    }
}
const H264_HIGH10: Codec = Codec {
    pixels: "yuv420p10le",
    args: &[
        "-preset",
        "ultrafast",
        "-profile:v",
        "high10",
        "-x264-params",
        "cabac=1:8x8dct=1",
    ],
    ..H264
};

struct Fixtures(PathBuf);
impl Fixtures {
    fn new() -> Self {
        let parent =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/kinema-mpv-native-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "owned-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn encode(&self, codec: Codec, size: [u32; 2]) -> PathBuf {
        self.encode_metadata(codec, size, Metadata::Bt709)
    }
    fn encode_metadata(&self, codec: Codec, size: [u32; 2], metadata: Metadata) -> PathBuf {
        let stem = format!("{}-{}-{}x{}", codec.name, codec.pixels, size[0], size[1]);
        let path = self.0.join(format!("{stem}.mkv"));
        let log = self.0.join(format!("{stem}.stderr"));
        // Lavfi colors originate in BT.601 YUV: convert, do not merely tag as
        // BT.709. Ten-bit variants are SDR, never an HDR qualification.
        let mut pattern = format!(
            "color=c=red:s={}x{}:r=24,drawbox=x=iw/2:y=0:w=iw/2:h=ih/2:color=lime:t=fill,drawbox=x=0:y=ih/2:w=iw/2:h=ih/2:color=blue:t=fill,drawbox=x=iw/2:y=ih/2:w=iw/2:h=ih/2:color=white:t=fill,scale=in_color_matrix=bt601:out_color_matrix=bt709:in_range=tv:out_range=tv,format={},setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709",
            size[0], size[1], codec.pixels,
        );
        match metadata {
            Metadata::Bt709 => {}
            Metadata::Untagged => {
                // Even scale's out_color_matrix can propagate a matrix tag.
                // Keep the raw SDR pattern and pixel format, with no color tags.
                pattern.truncate(pattern.find(",scale=").unwrap());
                pattern.push_str(&format!(",format={}", codec.pixels));
            }
            Metadata::PqTestOnly => {
                pattern = pattern.replace("color_trc=bt709", "color_trc=smpte2084")
            }
        }
        let child = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-filter_threads",
                "2",
                "-filter_complex_threads",
                "2",
                "-f",
                "lavfi",
                "-i",
                &pattern,
                "-an",
                "-sn",
                "-dn",
                "-t",
                "10",
                "-frames:v",
                "240",
                "-c:v",
                codec.encoder,
                "-threads",
                "2",
                "-pix_fmt",
                codec.pixels,
                "-color_range",
                "tv",
            ])
            .args(metadata.tags())
            .args(codec.args)
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(File::create(&log).unwrap())
            .spawn()
            .unwrap_or_else(|e| panic!("fixture capability {} unavailable: {e}", codec.encoder));
        let mut child = OwnedChild(child);
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "fixture encoder {} failed: {}",
                    codec.encoder,
                    fs::read_to_string(&log).unwrap_or_default()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fixture encoder {} exceeded 120 seconds",
                codec.encoder
            );
            thread::sleep(Duration::from_millis(20));
        }
        path
    }
    fn verify_metadata(&self, path: &Path, codec: Codec, metadata: Metadata) {
        // Probe only this test's owned synthetic file. Never print raw probe
        // output: the fixed assertions below are the complete metadata evidence.
        assert_eq!(path.parent(), Some(self.0.as_path()));
        let output = self.0.join("probe.txt");
        let mut child = OwnedChild(
            Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=codec_name,profile,pix_fmt,color_space,color_transfer,color_primaries",
                    "-of",
                    "default=noprint_wrappers=1",
                ])
                .arg(path)
                .stdin(Stdio::null())
                .stdout(File::create(&output).unwrap())
                .stderr(Stdio::null())
                .spawn()
                .expect("synthetic ffprobe required"),
        );
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "synthetic ffprobe failed");
                break;
            }
            assert!(Instant::now() < deadline, "synthetic ffprobe timed out");
            thread::sleep(Duration::from_millis(20));
        }
        let text = fs::read_to_string(output).unwrap();
        let has = |key, value| text.lines().any(|line| line == format!("{key}={value}"));
        assert!(has("codec_name", "h264"), "synthetic codec mismatch");
        assert!(
            has("pix_fmt", codec.pixels),
            "synthetic pixel format mismatch"
        );
        assert!(
            has(
                "profile",
                if codec.pixels == "yuv420p10le" {
                    "High 10"
                } else {
                    "High"
                }
            ),
            "synthetic H264 profile mismatch"
        );
        let (space, transfer, primaries) = match metadata {
            Metadata::Bt709 => ("bt709", "bt709", "bt709"),
            Metadata::Untagged => ("unknown", "unknown", "unknown"),
            Metadata::PqTestOnly => ("bt709", "smpte2084", "bt709"),
        };
        assert!(has("color_space", space), "synthetic matrix mismatch");
        assert!(
            has("color_transfer", transfer),
            "synthetic transfer mismatch"
        );
        assert!(
            has("color_primaries", primaries),
            "synthetic primaries mismatch"
        );
    }
}
impl Drop for Fixtures {
    fn drop(&mut self) {
        // Delete only the unique directory created and owned by this test.
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn assert_healthy(player: &Player) {
    let state = player.state();
    assert!(
        !state.stopped && state.fatal_error.is_none() && state.error.is_none(),
        "native Player failed (no capability skip/fallback): {state:?}"
    );
}
fn wait_state(player: &Player, predicate: impl Fn(&PlaybackState) -> bool) {
    let deadline = Instant::now() + WAIT;
    loop {
        assert_healthy(player);
        if predicate(&player.state()) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native state timeout: {:?}",
            player.state()
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn load(player: &Player, path: &Path, size: [u32; 2]) {
    // A zero viewport means hidden/suspended even under Source resolution.
    // Exercise the public visibility contract while keeping the viewport much
    // smaller than HD/UHD: all output assertions must still match source size.
    player.resize(640, 360).unwrap();
    player
        .load(path.to_str().expect("generated fixture path must be UTF-8"))
        .unwrap();
    wait_state(player, |state| {
        !state.loading && state.source_size == Some(size) && state.render_size == size
    });
}
fn assert_direct(player: &Player, size: [u32; 2]) {
    assert_healthy(player);
    let state = player.state();
    let d = player.diagnostics();
    assert_eq!(state.hardware_decoder.as_deref(), Some("vaapi"));
    assert_eq!(state.active_backend, RenderBackend::Gpu);
    assert_eq!(state.source_size, Some(size));
    assert_eq!(state.render_size, size);
    assert!(state.fallback_reason.is_none());
    assert_eq!(d.render_size, size);
    assert_eq!(d.source_size, Some(size));
    assert!(
        d.gpu_surface_frames > 0,
        "metadata alone is not GPU rendering"
    );
    assert_eq!(
        d.readback_bytes, 0,
        "experimental PLAYBACK must not read back pixels"
    );
    assert_eq!(d.upload_bytes_estimate, 0);
    assert_eq!(d.readback_submit_ms, 0.0);
    assert_eq!(d.readback_copy_ms, 0.0);
    assert!(
        player.take_frame().is_none(),
        "experimental playback published CPU pixels"
    );
    // Production Renderer validates actual decoded/output vaapi and NV12/P010
    // before publishing these GPU surfaces. The public API exposes hwdec only;
    // this test intentionally does not open a second mpv client for metadata.
}
fn close(player: Player) {
    let start = Instant::now();
    drop(player); // Own/join every worker; never detach a timed-out native worker.
    assert!(
        start.elapsed() < Duration::from_secs(30),
        "Player teardown exceeded native wait budget"
    );
}

fn codec_case(codec: Codec) {
    let _exclusive = EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner());
    let fixtures = Fixtures::new();
    let media = fixtures.encode(codec, HD);
    let gpu = Gpu::new();
    let player = gpu.player();
    load(&player, &media, HD);
    drop(gpu.verified_frame(&player, HD));
    player.set_paused(true).unwrap();
    wait_state(&player, |s| s.paused);
    thread::sleep(Duration::from_millis(150));
    let paused = player.state().position;
    thread::sleep(Duration::from_millis(200));
    assert!((player.state().position - paused).abs() < 0.15);
    player.seek_absolute(1.5).unwrap();
    wait_state(&player, |s| s.paused && (s.position - 1.5).abs() < 0.25);
    player.set_paused(false).unwrap();
    wait_state(&player, |s| !s.paused && s.position > 1.7);
    drop(gpu.verified_frame(&player, HD));
    assert_direct(&player, HD);
    close(player);
    gpu.assert_clean();
}
macro_rules! codec_test {
    ($name:ident, $codec:expr) => {
        #[test]
        #[ignore = "EXCLUSIVE Intel Mesa direct VAAPI SDR Player; requires isolated patched Vulkan HAL, EGL, libmpv, FFmpeg encoder and validation environment; TEST ONLY corner readback"]
        fn $name() { codec_case($codec); }
    };
}
codec_test!(native_player_vaapi_h264_1080, H264);
codec_test!(native_player_vaapi_hevc8_1080, HEVC);
codec_test!(native_player_vaapi_hevc10_1080, HEVC10);
codec_test!(native_player_vaapi_av1_8_1080, AV1);
codec_test!(native_player_vaapi_av1_10_1080, AV1_10);

#[test]
#[ignore = "EXCLUSIVE native Player source switch + three retained GPU frames; requires patched Intel Vulkan/VAAPI and FFmpeg; TEST ONLY corner readback"]
fn native_player_source_switch_retained_three() {
    let _exclusive = EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner());
    let fixtures = Fixtures::new();
    let hd = fixtures.encode(H264, HD);
    let uhd = fixtures.encode(H264, UHD);
    let gpu = Gpu::new();
    let player = gpu.player();
    load(&player, &hd, HD);
    let mut held: Vec<_> = (0..3).map(|_| gpu.verified_frame(&player, HD)).collect();
    let full = player.diagnostics().gpu_surface_frames;
    thread::sleep(Duration::from_millis(300));
    assert!(
        player.take_gpu_frame().is_none(),
        "fourth surface escaped three-frame cap"
    );
    assert_eq!(player.diagnostics().gpu_surface_frames, full);
    drop(held.pop());
    held.push(gpu.verified_frame(&player, HD));
    assert!(
        player.diagnostics().gpu_surface_frames > full,
        "release did not resume publication"
    );
    // Keep all three old native surfaces alive across the source replacement.
    player.load(uhd.to_str().unwrap()).unwrap();
    assert!(
        player.take_gpu_frame().is_none(),
        "source replacement did not clear mailbox"
    );
    wait_state(&player, |s| {
        s.source_size == Some(UHD) && s.render_size == UHD && !s.loading
    });
    assert!(
        player.take_gpu_frame().is_none(),
        "held old frames did not bound new allocation"
    );
    drop(held);
    drop(gpu.verified_frame(&player, UHD));
    assert_direct(&player, UHD);
    // Reverse switch catches stale 4K results, not only stale initial frames.
    load(&player, &hd, HD);
    let retained = gpu.verified_frame(&player, HD);
    close(player);
    // Published immutable frames may outlive their Player and private EGL context.
    assert!(oriented(gpu.corners(&retained)));
    drop(retained);
    gpu.assert_clean();
}

fn concurrent_players(mixed: bool) {
    let _exclusive = EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner());
    let fixtures = Fixtures::new();
    let media = fixtures.encode(H264, HD);
    let gpu = Gpu::new();
    let a = gpu.player();
    let b = gpu.player();
    load(&a, &media, HD);
    load(&b, &media, HD);
    drop(gpu.verified_frame(&a, HD));
    drop(gpu.verified_frame(&b, HD));
    let normal = mixed.then(|| {
        let c = Player::new(Config {
            render_backend: RenderBackend::Gpu,
            hardware_decoding: HardwareDecoding::Disabled,
            initial_volume: 0.0,
            ..Config::default()
        })
        .expect(
            "mixed-owner capability: normal hardware EGL GPU Player required (no CPU fallback)",
        );
        load(&c, &media, HD);
        let deadline = Instant::now() + WAIT;
        loop {
            assert_healthy(&c);
            if let Some(frame) = c.take_frame() {
                assert_eq!([frame.width, frame.height], HD);
                assert_eq!(c.state().active_backend, RenderBackend::Gpu);
                assert!(c.diagnostics().readback_bytes > 0);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "mixed-owner normal GPU readback absent"
            );
            thread::sleep(Duration::from_millis(10));
        }
        c
    });
    close(a);
    wait_state(&b, |s| !s.paused && !s.media_stopped);
    // Baseline AFTER close, so an old queued frame alone cannot pass.
    drop(b.take_gpu_frame());
    let before = b.diagnostics().gpu_surface_frames;
    wait_state(&b, |_| b.diagnostics().gpu_surface_frames > before);
    drop(gpu.verified_frame(&b, HD));
    if let Some(c) = normal {
        close(c);
        drop(b.take_gpu_frame());
        let before = b.diagnostics().gpu_surface_frames;
        wait_state(&b, |_| b.diagnostics().gpu_surface_frames > before);
        drop(gpu.verified_frame(&b, HD));
    }
    assert_direct(&b, HD);
    close(b);
    gpu.assert_clean();
    // EGL_PLATFORM_DEVICE_EXT and normal native/surfaceless EGLDisplays may be
    // distinct on Mesa. This is real coexistence/close-survivor coverage, not a
    // claim that display handles alias. The pure registry test forces one key.
}
#[test]
#[ignore = "EXCLUSIVE two public Players sharing one Vulkan device/queue gate; requires patched Intel VAAPI native stack; TEST ONLY corner readback"]
fn native_player_two_players_close_survivor() {
    concurrent_players(false);
}
#[test]
#[ignore = "EXCLUSIVE mixed EGL owners: two experimental + normal GPU readback Player; fails clearly if normal EGL capability unavailable"]
fn native_player_mixed_egl_close_survivor() {
    concurrent_players(true);
}

// These cases diagnose controlled synthetic inputs, never the user's media.
// First publication OR fatal is the bounded outcome; no sleep-and-guess success.
fn failure_case(codec: Codec, metadata: Metadata) -> Option<PlaybackFailure> {
    let _exclusive = EXCLUSIVE.lock().unwrap_or_else(|e| e.into_inner());
    let fixtures = Fixtures::new();
    let media = fixtures.encode_metadata(codec, HD, metadata);
    fixtures.verify_metadata(&media, codec, metadata);
    eprintln!("synthetic verified metadata: {metadata:?}");
    let gpu = Gpu::new();
    let player = gpu.player(); // Initialization failure is not a fixture outcome.
    player.resize(640, 360).expect("synthetic viewport");
    player
        .load(media.to_str().unwrap())
        .expect("synthetic load queued");
    let deadline = Instant::now() + WAIT;
    let reason = loop {
        let state = player.state();
        if state.fatal_error.is_some() || state.error.is_some() {
            let reason = state
                .failure_reason
                .expect("fatal must have safe classification");
            eprintln!("synthetic failure: {}", reason.code());
            break Some(reason);
        }
        if let Some(frame) = player.take_gpu_frame() {
            assert_eq!(frame.size(), HD);
            assert_direct(&player, HD);
            drop(frame);
            eprintln!("synthetic outcome: direct-gpu-frame");
            break None;
        }
        assert!(Instant::now() < deadline, "synthetic outcome timed out");
        thread::sleep(Duration::from_millis(10));
    };
    let diagnostics = player.diagnostics();
    assert_eq!(diagnostics.readback_bytes, 0);
    assert_eq!(diagnostics.upload_bytes_estimate, 0);
    assert!(player.take_frame().is_none(), "CPU fallback forbidden");
    assert!(player.state().fallback_reason.is_none());
    if reason.is_some() {
        assert_eq!(
            diagnostics.gpu_surface_frames, 0,
            "rejected fixture published GPU frames"
        );
    }
    close(player);
    gpu.assert_clean();
    reason
}

#[test]
#[ignore = "EXCLUSIVE synthetic untagged H264; lead-only patched Intel native execution"]
fn native_player_failure_untagged_sdr() {
    // validate_direct rejects unknown gamma, but libmpv may infer bt.1886 from
    // untagged HD. Source alone does NOT establish that this fixture must fail.
    if let Some(reason) = failure_case(H264, Metadata::Untagged) {
        assert!(matches!(
            reason.code(),
            "color-transfer-unqualified" | "color-metadata-unavailable"
        ));
    }
}

#[test]
#[ignore = "EXCLUSIVE synthetic H264 High10 rejection; lead-only patched Intel native execution"]
fn native_player_failure_h264_high10() {
    let reason = failure_case(H264_HIGH10, Metadata::Bt709)
        .expect("High10 must reject without software/copy fallback on the qualified Intel device");
    assert!(high10_rejection(Some(reason)));
}

fn high10_rejection(reason: Option<PlaybackFailure>) -> bool {
    // Lead observed absent decode properties with software fallback disabled:
    // the five-second metadata gate legitimately reports decode-metadata-unavailable.
    // mpv can also fail decoding before a non-direct surface reaches validation.
    // All accepted outcomes are fatal rejection; a GPU frame (None) never passes.
    matches!(
        reason.map(PlaybackFailure::code),
        Some("decoder-not-direct" | "media-load-failed" | "decode-metadata-unavailable")
    )
}

#[test]
fn synthetic_high10_policy_requires_known_fatal_rejection() {
    for reason in [
        PlaybackFailure::DecoderNotDirect,
        PlaybackFailure::MediaLoadFailed,
        PlaybackFailure::DecodeMetadataUnavailable,
    ] {
        assert!(high10_rejection(Some(reason)));
    }
    assert!(!high10_rejection(None), "GPU publication is not rejection");
    for reason in [
        PlaybackFailure::Unknown,
        PlaybackFailure::HdrUnsupported,
        PlaybackFailure::ColorMetadataUnavailable,
        PlaybackFailure::GpuInteropUnavailable,
    ] {
        assert!(!high10_rejection(Some(reason)));
    }
}

#[test]
#[ignore = "EXCLUSIVE TEST ONLY PQ metadata on SDR pixels, NOT HDR visuals; lead-only native execution"]
fn native_player_failure_hdr_transfer() {
    let reason = failure_case(H264, Metadata::PqTestOnly)
        .expect("PQ transfer must be rejected by the experimental SDR gate");
    assert_eq!(reason.code(), "hdr-unsupported");
}

#[test]
fn synthetic_failure_codes_never_echo_driver_strings() {
    for (message, expected) in [
        (
            "direct hardware surface required: driver=/private/SECRET",
            "decoder-not-direct",
        ),
        (
            "experimental SDR only; unqualified transfer function: pq",
            "hdr-unsupported",
        ),
        (
            "driver=/private/SECRET https://SECRET.invalid/?token=SECRET",
            "unknown",
        ),
    ] {
        let reason = crate::Error::Gpu(message.into()).playback_failure();
        assert_eq!(reason.code(), expected);
        assert!(!reason.code().contains("SECRET"));
        assert!(!reason.message().contains("SECRET"));
        assert!(!reason.message().contains("/private/"));
    }
}
