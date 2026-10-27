//! Default GPU-resident rendering on the existing libmpv client.
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

/// The host's actual presentation device, shared with the render worker.
/// This does not create or qualify a native device. Windows requires Dx12;
/// Linux requires matching Vulkan/EGL devices and external FD capabilities.
#[derive(Clone)]
pub struct GpuSurfaceContext {
    #[cfg(not(target_os = "macos"))]
    pub(super) device: Arc<wgpu::Device>,
    #[cfg(not(target_os = "macos"))]
    pub(super) queue: Arc<wgpu::Queue>,
    #[cfg(not(target_os = "macos"))]
    pub(super) queue_gate: Arc<Mutex<()>>,
    #[cfg(target_os = "macos")]
    pub(super) registry_id: u64,
}
impl std::fmt::Debug for GpuSurfaceContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuSurfaceContext").finish_non_exhaustive()
    }
}
impl GpuSurfaceContext {
    /// Use the presentation device/queue and its shared queue-serialization gate.
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
    /// Use the registry ID of the host's actual Metal presentation device.
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
pub struct GpuSurfaceFrame(Arc<FrameOwner>);
impl GpuSurfaceFrame {
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
    diagnostic_id: u64,
    mpv: Option<Box<MpvContext>>,
    retained: Vec<Arc<FrameOwner>>,
    evidence_wait: EvidenceWait,
    evidence_events: u8,
}
impl Renderer {
    pub fn new(
        client: Arc<Client>,
        context: &GpuSurfaceContext,
        diagnostic_id: u64,
    ) -> Result<Self> {
        let native = Box::new(NativeInterop::new(context).map_err(Error::Gpu)?);
        let mut this = Self {
            diagnostic_id,
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
            evidence_events: 0,
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
            .check(code, "create GPU surface render context")?;
        if mpv.ptr.is_null() {
            return Err(Error::Gpu("null GPU surface render context".into()));
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
        mpv.client.loaded.check(code, "skip GPU surface frame")
    }
    pub fn evidence_ready(&mut self, generation: u64, evidence: &[String; 6]) -> Result<bool> {
        let previous = (self.evidence_wait.0.map(|v| v.0), self.evidence_wait.1);
        let now = std::time::Instant::now();
        let result = self.evidence_wait.ready(generation, evidence, now);
        if previous != (Some(generation), self.evidence_wait.1) && self.evidence_events < 32 {
            tracing::info!(
                player_id = self.diagnostic_id,
                generation,
                qualification = self.evidence_wait.1.unwrap_or("unknown"),
                qualification_wait_ms = self.evidence_wait.elapsed_ms(now),
                "mpv GPU surface metadata qualification"
            );
            self.evidence_events += 1;
        }
        result
    }
    pub fn render(&mut self, size: [u32; 2]) -> Result<Option<GpuSurfaceFrame>> {
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
        mpv.client.loaded.check(code, "render GPU surface frame")?;
        let native = mpv.native.finish_frame().map_err(Error::Gpu)?;
        let frame = GpuSurfaceFrame(Arc::new(FrameOwner { native, size }));
        self.retained.push(frame.0.clone());
        Ok(Some(frame))
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(mut mpv) = self.mpv.take() {
            let complete = finish_teardown(
                &mut *mpv,
                |mpv| mpv.native.complete_submissions(),
                MpvContext::free,
                |mpv| mpv.native.shutdown(),
            );
            let outcome = if complete {
                super::diagnostics::NativeCleanup::Complete
            } else {
                super::diagnostics::NativeCleanup::Quarantined
            };
            if !complete {
                std::mem::forget(mpv);
                std::mem::forget(std::mem::take(&mut self.retained));
            }
            // Quarantine must be established even if a subscriber unwinds.
            super::diagnostics::native_cleanup(self.diagnostic_id, outcome);
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
struct EvidenceWait(Option<(u64, std::time::Instant)>, Option<&'static str>);

fn pending(value: &str) -> bool {
    matches!(value, "" | "unavailable" | "auto" | "unknown")
}
fn hdr(value: &str) -> bool {
    matches!(value, "pq" | "hlg" | "st2084" | "arib-std-b67")
}
fn sdr(value: &str) -> bool {
    matches!(
        value,
        "bt.1886" | "bt.709" | "srgb" | "gamma2.2" | "gamma2.8"
    )
}
impl EvidenceWait {
    // Numeric only, emitted with the existing transition-only, 32-event budget.
    // This is time since first evidence check in the generation, not load-to-frame
    // latency or a GPU/display timing. Requalification keeps that same origin.
    fn elapsed_ms(&self, now: std::time::Instant) -> u64 {
        self.0.map_or(0, |(_, started)| {
            now.saturating_duration_since(started)
                .as_millis()
                .min(u64::MAX as u128) as u64
        })
    }

    fn ready(
        &mut self,
        generation: u64,
        evidence: &[String; 6],
        now: std::time::Instant,
    ) -> Result<bool> {
        let started = match self.0 {
            Some((old, started)) if old == generation => started,
            _ => {
                self.0 = Some((generation, now));
                self.1 = None;
                now
            }
        };
        let result = qualify(evidence, now.duration_since(started));
        let reason = match &result {
            Ok(true) => "ready",
            Ok(false) if evidence[..4].iter().any(|v| pending(v)) => "waiting-decode",
            Ok(false) => "waiting-effective-transfer",
            Err(error) => error.playback_failure().code(),
        };
        if self.1 != Some(reason) {
            self.1 = Some(reason);
        }
        result
    }
}

fn qualify(evidence: &[String; 6], elapsed: std::time::Duration) -> Result<bool> {
    // Never let a guessed/overridden SDR effective value conceal decoded HDR.
    if evidence[4..].iter().any(|v| hdr(v)) {
        return Err(Error::Gpu(
            "GPU surface SDR only; HDR transfer detected".into(),
        ));
    }
    // Explicit unsupported transfers are not startup absence. Only mpv's
    // resolved effective SDR can qualify unspecified decoder metadata.
    if (!pending(&evidence[4]) && !sdr(&evidence[4]))
        || (!pending(&evidence[5]) && !sdr(&evidence[5]))
    {
        return Err(Error::Gpu(
            "GPU surface SDR only; unqualified transfer function: unsupported".into(),
        ));
    }
    let decode_pending = evidence[..4].iter().any(|v| pending(v));
    if decode_pending || pending(&evidence[5]) {
        if elapsed >= std::time::Duration::from_secs(5) {
            return Err(Error::Gpu(
                if decode_pending {
                    "direct decode evidence unavailable after five seconds"
                } else {
                    "color metadata unavailable after five seconds"
                }
                .into(),
            ));
        }
        return Ok(false);
    }
    validate_direct(
        &evidence[0],
        &evidence[1],
        &evidence[2],
        &evidence[3],
        &evidence[5],
    )?;
    Ok(true)
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
        return Err(Error::Gpu(
            "direct hardware surface required: unsupported".into(),
        ));
    }
    if !matches!(
        gamma,
        "bt.1886" | "bt.709" | "srgb" | "gamma2.2" | "gamma2.8"
    ) {
        return Err(Error::Gpu(
            "GPU surface SDR only; unqualified transfer function: unsupported".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn direct_evidence(decoded: &str, effective: &str) -> [String; 6] {
        let native = if cfg!(target_os = "linux") {
            "vaapi"
        } else if cfg!(target_os = "windows") {
            "d3d11"
        } else {
            "videotoolbox_vld"
        };
        [hwdec(), native, native, "nv12", decoded, effective].map(str::to_owned)
    }
    #[test]
    fn qualification_timing_and_requalification_are_generation_scoped() {
        let now = std::time::Instant::now();
        let later = now + std::time::Duration::from_millis(250);
        let mut wait = EvidenceWait::default();
        assert_eq!(wait.elapsed_ms(now), 0);
        assert!(
            !wait
                .ready(1, &direct_evidence("auto", "auto"), now)
                .unwrap()
        );
        assert_eq!(wait.1, Some("waiting-effective-transfer"));
        assert!(
            wait.ready(1, &direct_evidence("auto", "bt.1886"), later)
                .unwrap()
        );
        assert_eq!(wait.1, Some("ready"));
        assert_eq!(wait.elapsed_ms(later), 250);
        // A paused source can change metadata without a new frame/generation.
        // No ready result is cached, and resolved SDR never conceals decoded HDR.
        assert_eq!(
            wait.ready(1, &direct_evidence("pq", "bt.1886"), later)
                .unwrap_err()
                .playback_failure(),
            crate::PlaybackFailure::HdrUnsupported
        );
        assert_eq!(wait.elapsed_ms(later), 250);
        assert!(
            !wait
                .ready(2, &direct_evidence("auto", "auto"), later)
                .unwrap()
        );
        assert_eq!(wait.elapsed_ms(later), 0);
        assert_eq!(wait.1, Some("waiting-effective-transfer"));
    }

    #[test]
    fn auto_requires_effective_resolution_and_unknown_is_bounded() {
        let now = std::time::Instant::now();
        let mut resolving = EvidenceWait::default();
        assert!(
            !resolving
                .ready(1, &direct_evidence("auto", "auto"), now)
                .unwrap()
        );
        assert!(
            resolving
                .ready(
                    1,
                    &direct_evidence("auto", "bt.1886"),
                    now + std::time::Duration::from_secs(1)
                )
                .unwrap()
        );
        for value in ["auto", "unknown", "", "unavailable"] {
            let mut wait = EvidenceWait::default();
            assert!(!wait.ready(1, &direct_evidence("auto", value), now).unwrap());
            assert_eq!(
                wait.ready(
                    1,
                    &direct_evidence("auto", value),
                    now + std::time::Duration::from_secs(5)
                )
                .unwrap_err()
                .playback_failure(),
                crate::PlaybackFailure::ColorMetadataUnavailable
            );
            assert!(
                !wait
                    .ready(
                        2,
                        &direct_evidence("auto", value),
                        now + std::time::Duration::from_secs(5)
                    )
                    .unwrap()
            );
            assert!(
                wait.ready(
                    2,
                    &direct_evidence("auto", "bt.1886"),
                    now + std::time::Duration::from_secs(6)
                )
                .unwrap()
            );
        }
    }
    #[test]
    fn decoded_sdr_does_not_bypass_pending_effective_and_unsupported_is_immediate() {
        for pending in ["auto", "unknown", "", "unavailable"] {
            let evidence = direct_evidence("bt.709", pending);
            assert!(!qualify(&evidence, std::time::Duration::ZERO).unwrap());
            assert_eq!(
                qualify(&evidence, std::time::Duration::from_secs(5))
                    .unwrap_err()
                    .playback_failure(),
                crate::PlaybackFailure::ColorMetadataUnavailable
            );
        }
        for (decoded, effective) in [
            ("linear", "bt.1886"),
            ("auto", "linear"),
            ("https://SECRET/path", "bt.1886"),
        ] {
            let error = qualify(
                &direct_evidence(decoded, effective),
                std::time::Duration::ZERO,
            )
            .unwrap_err();
            assert_eq!(
                error.playback_failure(),
                crate::PlaybackFailure::ColorTransferUnqualified
            );
            assert!(!error.to_string().contains("SECRET"));
        }
    }
    #[test]
    fn either_hdr_transfer_rejects_even_if_other_is_sdr_or_missing() {
        for hdr in ["pq", "hlg", "st2084", "arib-std-b67"] {
            for other in ["bt.1886", "auto", ""] {
                for (decoded, effective) in [(hdr, other), (other, hdr)] {
                    assert_eq!(
                        qualify(
                            &direct_evidence(decoded, effective),
                            std::time::Duration::ZERO
                        )
                        .unwrap_err()
                        .playback_failure(),
                        crate::PlaybackFailure::HdrUnsupported
                    );
                }
            }
        }
    }
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
    fn missing_color_and_decode_metadata_have_distinct_bounded_reasons() {
        use crate::PlaybackFailure as F;
        let now = std::time::Instant::now();
        for (evidence, expected) in [
            (
                [
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ],
                F::DecodeMetadataUnavailable,
            ),
            (
                [
                    hwdec().into(),
                    "native".into(),
                    "native".into(),
                    "nv12".into(),
                    String::new(),
                    String::new(),
                ],
                F::ColorMetadataUnavailable,
            ),
        ] {
            let mut wait = EvidenceWait::default();
            assert!(!wait.ready(1, &evidence, now).unwrap());
            assert!(
                !wait
                    .ready(1, &evidence, now + std::time::Duration::from_millis(4999))
                    .unwrap()
            );
            assert_eq!(
                wait.ready(1, &evidence, now + std::time::Duration::from_secs(5))
                    .unwrap_err()
                    .playback_failure(),
                expected
            );
        }
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
                &direct_evidence("auto", "bt.1886"),
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
