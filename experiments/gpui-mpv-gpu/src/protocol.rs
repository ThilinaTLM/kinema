//! Pure launcher argument policy. Local media is opened only by the worker.
use std::{path::PathBuf, time::Duration};

use crate::producer::DecoderMode;

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub media: PathBuf,
    pub seconds: u64,
    pub decoder: DecoderMode,
    pub render_size: [u32; 2],
    pub stress_resize: bool,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let mut media = None;
        let mut seconds = None;
        let mut decoder = None;
        let mut render_size = None;
        let mut stress_resize = false;
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--media" if media.is_none() => {
                    let value = args.next().ok_or("--media requires a local file")?;
                    if value.is_empty() || value.contains("://") || value.contains('\0') {
                        return Err("--media requires an explicit local file, not a URL".into());
                    }
                    let path = PathBuf::from(value);
                    if !path.is_absolute() {
                        return Err("--media must be an absolute local file path".into());
                    }
                    media = Some(path);
                }
                "--seconds" if seconds.is_none() => {
                    let value = args.next().ok_or("--seconds requires an integer")?;
                    let duration: u64 = value.parse().map_err(|_| "invalid duration")?;
                    if !(1..=120).contains(&duration) {
                        return Err("--seconds must be between 1 and 120".into());
                    }
                    seconds = Some(duration);
                }
                "--hwdec" if decoder.is_none() => {
                    decoder = Some(match args.next().as_deref() {
                        Some("no") => DecoderMode::Software,
                        Some("vaapi") => DecoderMode::Vaapi,
                        _ => return Err("--hwdec must be no or explicit vaapi; automatic/copy modes are unsupported".into()),
                    });
                }
                "--render-size" if render_size.is_none() => {
                    let value = args.next().ok_or("--render-size requires WIDTHxHEIGHT")?;
                    render_size = Some(parse_render_size(&value)?);
                }
                "--stress-resize" if !stress_resize => stress_resize = true,
                _ => return Err("unknown or duplicate argument".into()),
            }
        }
        let seconds = seconds.unwrap_or(10);
        if stress_resize && seconds < 4 {
            return Err("--stress-resize requires --seconds >= 4".into());
        }
        Ok(Self {
            media: media.ok_or("--media is required (use run.sh run for a synthetic fixture)")?,
            seconds,
            decoder: decoder.unwrap_or(DecoderMode::Software),
            render_size: render_size.unwrap_or([640, 360]),
            stress_resize,
        })
    }
}

pub fn parse_render_size(value: &str) -> Result<[u32; 2], String> {
    let (width, height) = value
        .split_once('x')
        .ok_or("--render-size requires WIDTHxHEIGHT")?;
    if width.is_empty()
        || height.is_empty()
        || !width.bytes().all(|c| c.is_ascii_digit())
        || !height.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("--render-size requires unsigned decimal WIDTHxHEIGHT".into());
    }
    let width: u32 = width.parse().map_err(|_| "render width overflow")?;
    let height: u32 = height.parse().map_err(|_| "render height overflow")?;
    let bytes = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("render pixel byte size overflow")?;
    if width == 0 || height == 0 || width > 3840 || height > 2160 || bytes > 64 * 1024 * 1024 {
        return Err("render size must be 1..3840 by 1..2160 and at most 64 MiB RGBA".into());
    }
    Ok([width, height])
}

/// Codec evidence uses fixed labels, never arbitrary media metadata.
pub fn safe_codec(value: &str) -> &'static str {
    match value {
        "h264" => "h264",
        "hevc" => "hevc",
        "mpeg4" => "mpeg4",
        "av1" => "av1",
        _ => "other",
    }
}

/// Only reviewed format names may reach logs; never echo arbitrary media metadata.
pub fn safe_pixel_format(value: &str) -> &'static str {
    match value {
        "vaapi" => "vaapi",
        "nv12" => "nv12",
        "p010" => "p010",
        "p010le" => "p010le",
        "yuv420p" => "yuv420p",
        "yuv420p10le" => "yuv420p10le",
        _ => "unavailable",
    }
}

/// Probe-only bounded resize schedule; it never changes producer allocation.
#[derive(Default)]
pub struct ResizeStress {
    started: Option<Duration>,
    next_request: Duration,
    last_viewport: Option<[u32; 2]>,
    requests: u64,
    observed_changes: u64,
}

impl ResizeStress {
    pub fn tick(
        &mut self,
        elapsed: Duration,
        accepted: u64,
        viewport: [u32; 2],
    ) -> Option<[u32; 2]> {
        if self.requests > 0
            && self
                .last_viewport
                .is_some_and(|previous| previous != viewport)
        {
            self.observed_changes += 1;
        }
        self.last_viewport = Some(viewport);
        if accepted < 3 {
            return None;
        }
        let started = *self.started.get_or_insert(elapsed);
        if elapsed.saturating_sub(started) >= Duration::from_secs(2)
            || self.requests >= 14
            || elapsed < self.next_request
        {
            return None;
        }
        self.requests += 1;
        self.next_request = elapsed.saturating_add(Duration::from_millis(150));
        Some(if self.requests % 2 == 1 {
            [1100, 700]
        } else {
            [960, 600]
        })
    }

    pub fn requests(&self) -> u64 {
        self.requests
    }
    pub fn observed_changes(&self) -> u64 {
        self.observed_changes
    }
    pub fn sufficient(&self) -> bool {
        self.requests >= 2 && self.observed_changes >= 2
    }
}

/// Only strictly increasing producer sequence numbers demonstrate progress.
#[derive(Default)]
pub struct TemporalProgress {
    last: Option<u64>,
    count: u64,
}

impl TemporalProgress {
    pub fn accept(&mut self, sequence: u64) {
        if self.last.is_none_or(|previous| sequence > previous) {
            self.last = Some(sequence);
            self.count += 1;
        }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn sufficient(&self) -> bool {
        self.count >= 3
    }
}

/// Fixed reviewed category/operation codes only. Never return any error substring:
/// backend errors can contain local paths, dynamic loader paths or media metadata.
pub fn failure_code(error: &str) -> &'static str {
    const CATEGORIES: &[(&str, &str)] = &[
        ("local media:", "media.open"),
        ("media must be a regular", "media.file_type"),
        ("NUL in media path", "media.path"),
        ("load libmpv", "library.mpv"),
        ("mpv_create failed", "mpv.create"),
        ("mpv option", "mpv.options"),
        ("mpv initialize", "mpv.initialize"),
        ("mpv OpenGL render context", "mpv.render_context"),
        ("mpv load local media", "mpv.media_load"),
        ("mpv end-file", "mpv.media_end"),
        ("mpv render", "mpv.render"),
        ("vaapi-capability", "qualification.vaapi_evidence"),
        ("unqualified-capability", "mpv.hwdec_policy"),
        ("KINEMA_GPU_EXPERIMENT_HWDEC", "mpv.hwdec_policy"),
        ("UUID", "interop.device_identity"),
        ("GL_EXT_memory_object_fd", "capability.gl_external_fd"),
        (
            "external_memory_fd was not enabled",
            "capability.vk_external_memory_fd",
        ),
        (
            "external_semaphore_fd was not enabled",
            "capability.vk_external_semaphore_fd",
        ),
        (
            "opaque-FD semaphore export",
            "capability.vk_semaphore_export",
        ),
        ("opaque-FD buffer export", "capability.vk_buffer_export"),
        ("missing GL entry point", "library.gl_entrypoint"),
        ("Intel Mesa only", "capability.vendor"),
        ("Vulkan wgpu device", "capability.vulkan_device"),
        ("GL error", "interop.gl_operation"),
        ("create external buffer", "interop.buffer_create"),
        ("allocate external memory", "interop.memory_allocate"),
        ("bind external memory", "interop.memory_bind"),
        ("export memory FD", "interop.memory_export"),
        ("no exportable device-local", "interop.memory_type"),
        ("external buffer handoff submit", "interop.handoff_submit"),
        ("external buffer handoff fence", "interop.handoff_wait"),
        ("queue gate poisoned", "interop.queue_gate"),
        ("worker retirement wait", "interop.retirement_wait"),
        ("worker GPU wait failed", "interop.copy_wait"),
        ("shutdown", "interop.shutdown"),
        ("framebuffer", "interop.framebuffer"),
        ("dimensions", "interop.dimensions"),
        ("overflow", "interop.dimensions"),
        ("libEGL", "library.egl"),
        ("EGL", "interop.egl_initialize"),
        ("worker panicked", "worker.panic"),
    ];
    CATEGORIES
        .iter()
        .find_map(|(needle, code)| error.contains(needle).then_some(*code))
        .unwrap_or("backend.unknown")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn temporal_success_requires_three_unique_increasing_sequences() {
        let mut progress = TemporalProgress::default();
        assert!(!progress.sufficient());
        progress.accept(0);
        progress.accept(0);
        assert_eq!(progress.count(), 1);
        assert!(!progress.sufficient());
        progress.accept(2);
        progress.accept(1);
        assert!(!progress.sufficient());
        progress.accept(3);
        assert!(progress.sufficient());
        assert_eq!(progress.count(), 3);
    }

    #[test]
    fn errors_only_emit_fixed_privacy_safe_codes() {
        assert_eq!(
            failure_code("local media: /private/secret/movie: denied"),
            "media.open"
        );
        assert_eq!(
            failure_code("EGL enumeration: UUID mismatch private device name"),
            "interop.device_identity"
        );
        assert_eq!(
            failure_code("requires GL_EXT_memory_object_fd and GL_EXT_semaphore_fd"),
            "capability.gl_external_fd"
        );
        assert_eq!(
            failure_code("external buffer handoff fence (5s limit): ERROR_DEVICE_LOST"),
            "interop.handoff_wait"
        );
        assert_eq!(
            failure_code(crate::producer::UNQUALIFIED_HWDEC_ERROR),
            "mpv.hwdec_policy"
        );
        assert_eq!(
            failure_code("vaapi-capability: observed decoder=private-metadata"),
            "qualification.vaapi_evidence"
        );
        assert_eq!(safe_pixel_format("vaapi"), "vaapi");
        assert_eq!(safe_pixel_format("nv12"), "nv12");
        assert_eq!(safe_pixel_format("/private/secret"), "unavailable");
        assert_eq!(
            failure_code("/private/secret/unrecognized"),
            "backend.unknown"
        );
    }

    #[test]
    fn local_file_and_default_duration() {
        let options = parse(&["--media", "/tmp/synthetic clip.mkv"]).unwrap();
        assert_eq!(options.seconds, 10);
        assert_eq!(options.decoder, DecoderMode::Software);
        assert_eq!(options.render_size, [640, 360]);
        assert!(!options.stress_resize);
        assert_eq!(options.media, PathBuf::from("/tmp/synthetic clip.mkv"));
    }

    #[test]
    fn resize_stress_arguments_are_explicit_bounded_and_not_duplicate() {
        assert!(
            parse(&["--media", "/tmp/a", "--stress-resize"])
                .unwrap()
                .stress_resize
        );
        assert!(parse(&["--media", "/tmp/a", "--stress-resize", "--seconds", "3"]).is_err());
        assert!(parse(&["--media", "/tmp/a", "--seconds", "4", "--stress-resize"]).is_ok());
        assert!(parse(&["--media", "/tmp/a", "--stress-resize", "--stress-resize"]).is_err());
    }

    #[test]
    fn resize_stress_waits_for_frames_and_requires_real_changes() {
        let mut stress = ResizeStress::default();
        let base = [960, 600];
        assert_eq!(stress.tick(Duration::ZERO, 2, base), None);
        assert_eq!(
            stress.tick(Duration::from_secs(1), 3, base),
            Some([1100, 700])
        );
        assert_eq!(stress.tick(Duration::from_millis(1149), 3, base), None);
        assert_eq!(
            stress.tick(Duration::from_millis(1150), 3, base),
            Some(base)
        );
        assert!(!stress.sufficient(), "requests alone never prove resize");
        stress.tick(Duration::from_millis(1160), 3, [1100, 700]);
        stress.tick(Duration::from_millis(1170), 3, base);
        assert!(stress.sufficient());
        for millis in 1171..=4000 {
            stress.tick(Duration::from_millis(millis), 3, base);
        }
        assert!(stress.requests() <= 14);
        assert_eq!(stress.tick(Duration::from_secs(120), 3, base), None);
        assert_eq!(stress.observed_changes(), 2);
    }

    #[test]
    fn codec_evidence_includes_av1_without_echoing_unknown_metadata() {
        for codec in ["h264", "hevc", "mpeg4", "av1"] {
            assert_eq!(safe_codec(codec), codec);
        }
        for unknown in [
            "",
            "AV1",
            "av1-copy",
            "/private/media/av1.mkv",
            "secret\nav1",
        ] {
            assert_eq!(safe_codec(unknown), "other");
        }
    }

    #[test]
    fn explicit_vaapi_and_hd_size() {
        let options = parse(&[
            "--media",
            "/tmp/synthetic.mkv",
            "--hwdec",
            "vaapi",
            "--render-size",
            "1920x1080",
        ])
        .unwrap();
        assert_eq!(options.decoder, DecoderMode::Vaapi);
        assert_eq!(options.render_size, [1920, 1080]);
        assert_eq!(parse_render_size("3840x2160").unwrap(), [3840, 2160]);
        let uhd = parse(&[
            "--media",
            "/tmp/synthetic.mkv",
            "--hwdec",
            "vaapi",
            "--render-size",
            "3840x2160",
        ])
        .unwrap();
        assert_eq!(uhd.decoder, DecoderMode::Vaapi);
        assert_eq!(uhd.render_size, [3840, 2160]);
    }

    #[test]
    fn rejects_auto_copy_unknown_missing_and_duplicate_decoder_modes() {
        for mode in [
            "auto",
            "auto-safe",
            "vaapi-copy",
            "vulkan-copy",
            "software",
            "",
            "VAAPI",
        ] {
            assert!(parse(&["--media", "/tmp/a", "--hwdec", mode]).is_err());
        }
        assert!(parse(&["--media", "/tmp/a", "--hwdec"]).is_err());
        assert!(parse(&["--media", "/tmp/a", "--hwdec", "no", "--hwdec", "vaapi"]).is_err());
    }

    #[test]
    fn render_size_rejects_malformed_overflow_limits_and_duplicates() {
        for value in [
            "",
            "1920",
            "1920X1080",
            "1x2x3",
            "0x1",
            "1x0",
            "3841x1",
            "1x2161",
            "4294967296x1",
            "4294967295x4294967295",
            "65536x65536",
            "+1x1",
            "1x-1",
            " 1x1",
            "1x",
            "x1",
        ] {
            assert!(
                parse_render_size(value).is_err(),
                "unexpected accepted size: {value}"
            );
        }
        assert!(parse(&["--media", "/tmp/a", "--render-size"]).is_err());
        assert!(
            parse(&[
                "--media",
                "/tmp/a",
                "--render-size",
                "1x1",
                "--render-size",
                "2x2"
            ])
            .is_err()
        );
    }

    #[test]
    fn rejects_remote_relative_missing_and_duplicate_media() {
        for args in [
            vec![],
            vec!["--media"],
            vec!["--media", "https://example.invalid/a"],
            vec!["--media", "file:///tmp/a"],
            vec!["--media", "relative.mkv"],
            vec!["--media", "/tmp/a", "--media", "/tmp/b"],
            vec!["--media", "/tmp/a", "--unknown"],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn duration_is_bounded_and_single() {
        for value in ["0", "121", "-1", "1.5", "18446744073709551616"] {
            assert!(parse(&["--media", "/tmp/a", "--seconds", value]).is_err());
        }
        assert_eq!(
            parse(&["--media", "/tmp/a", "--seconds", "120"])
                .unwrap()
                .seconds,
            120
        );
        assert!(parse(&["--media", "/tmp/a", "--seconds", "1", "--seconds", "2"]).is_err());
    }
}
