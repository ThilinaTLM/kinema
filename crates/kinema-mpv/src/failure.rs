//! Reviewed, bounded diagnostics safe for presentation and structured logs.
//! Never construct messages from native errors, media metadata, or user input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackFailure {
    BackendNotCompiled,
    GpuContextUnavailable,
    DecoderNotDirect,
    ColorTransferUnqualified,
    HdrUnsupported,
    ColorMetadataUnavailable,
    DecodeMetadataUnavailable,
    GpuDeviceUnsupported,
    GpuInteropUnavailable,
    GpuCompletionFailed,
    LibmpvUnavailable,
    LibmpvIncompatible,
    RenderInitFailed,
    MediaLoadFailed,
    RenderSizeLimit,
    Unknown,
}

impl PlaybackFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::BackendNotCompiled => "backend-not-compiled",
            Self::GpuContextUnavailable => "gpu-context-unavailable",
            Self::DecoderNotDirect => "decoder-not-direct",
            Self::ColorTransferUnqualified => "color-transfer-unqualified",
            Self::HdrUnsupported => "hdr-unsupported",
            Self::ColorMetadataUnavailable => "color-metadata-unavailable",
            Self::DecodeMetadataUnavailable => "decode-metadata-unavailable",
            Self::GpuDeviceUnsupported => "gpu-device-unsupported",
            Self::GpuInteropUnavailable => "gpu-interop-unavailable",
            Self::GpuCompletionFailed => "gpu-completion-failed",
            Self::LibmpvUnavailable => "libmpv-unavailable",
            Self::LibmpvIncompatible => "libmpv-incompatible",
            Self::RenderInitFailed => "render-init-failed",
            Self::MediaLoadFailed => "media-load-failed",
            Self::RenderSizeLimit => "render-size-limit",
            Self::Unknown => "unknown",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::BackendNotCompiled => "This playback backend is not included in this build.",
            Self::GpuContextUnavailable => "The application's GPU context is unavailable.",
            Self::DecoderNotDirect => {
                "The decoder did not provide the required direct hardware surface."
            }
            Self::ColorTransferUnqualified => {
                "The video's color transfer is not qualified for GPU surface SDR playback."
            }
            Self::HdrUnsupported => {
                "HDR playback is not supported by this GPU surface SDR backend."
            }
            Self::ColorMetadataUnavailable => {
                "Required color metadata was unavailable after five seconds."
            }
            Self::DecodeMetadataUnavailable => {
                "Required decode metadata was unavailable after five seconds."
            }
            Self::GpuDeviceUnsupported => {
                "The GPU device is not supported by this GPU surface backend."
            }
            Self::GpuInteropUnavailable => {
                "Required GPU interop or matched-device support is unavailable."
            }
            Self::GpuCompletionFailed => {
                "GPU work could not be confirmed complete. Recreate the player."
            }
            Self::LibmpvUnavailable => "The libmpv playback library could not be loaded.",
            Self::LibmpvIncompatible => {
                "The libmpv playback library is incompatible with this build."
            }
            Self::RenderInitFailed => "The playback renderer could not be initialized.",
            Self::MediaLoadFailed => "The media could not be loaded or playback failed.",
            Self::RenderSizeLimit => "The requested video dimensions exceed rendering limits.",
            Self::Unknown => "Playback failed for an unclassified reason.",
        }
    }

    /// Only controlled native prefixes are recognized. No substring matching:
    /// a URL/path containing a diagnostic phrase must not influence attribution.
    pub(crate) fn from_gpu_message(message: &str) -> Self {
        if let Some(gamma) =
            message.strip_prefix("GPU surface SDR only; unqualified transfer function: ")
            && matches!(gamma, "pq" | "hlg" | "st2084" | "arib-std-b67")
        {
            return Self::HdrUnsupported;
        }
        for &(prefix, reason) in GPU_PREFIXES {
            if message.starts_with(prefix) {
                return reason;
            }
        }
        Self::Unknown
    }
}

// This allowlist translates existing native boundary messages without exposing
// their dynamic suffixes. Unknown messages deliberately stay unclassified.
const GPU_PREFIXES: &[(&str, PlaybackFailure)] = &[
    (
        "GPU surface SDR only; HDR transfer detected",
        PlaybackFailure::HdrUnsupported,
    ),
    (
        "expected exactly one GL device UUID, got ",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "GL/Vulkan device UUID mismatch: ",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "GL device UUID: GL error ",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "active accelerated CGL registry identity is missing, ambiguous, or differs from GPUI Metal renderer",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "active CGL renderer changed; refusing adapter mismatch",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "active CGL renderer is not hardware accelerated",
        PlaybackFailure::GpuDeviceUnsupported,
    ),
    (
        "prior GL fence completion unknown;",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "GL fence allocation failed;",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "GL completion unproven (",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "macOS interop poisoned;",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "GPU surface macOS frame dimensions exceed GL rectangle capabilities",
        PlaybackFailure::RenderSizeLimit,
    ),
    (
        "no accelerated CGL core 3.2 pixel format",
        PlaybackFailure::RenderInitFailed,
    ),
    (
        "CGLCreateContext returned null",
        PlaybackFailure::RenderInitFailed,
    ),
    (
        "direct hardware surface required: ",
        PlaybackFailure::DecoderNotDirect,
    ),
    (
        "GPU surface SDR only; unqualified transfer function: ",
        PlaybackFailure::ColorTransferUnqualified,
    ),
    (
        "direct decode evidence unavailable after five seconds",
        PlaybackFailure::DecodeMetadataUnavailable,
    ),
    (
        "color metadata unavailable after five seconds",
        PlaybackFailure::ColorMetadataUnavailable,
    ),
    (
        "this proof currently supports Intel Mesa only",
        PlaybackFailure::GpuDeviceUnsupported,
    ),
    (
        "requires a Vulkan wgpu device",
        PlaybackFailure::GpuDeviceUnsupported,
    ),
    (
        "GPU surface Windows playback requires the actual GPUI WGPU Dx12 device;",
        PlaybackFailure::GpuDeviceUnsupported,
    ),
    (
        "gpu-render feature is disabled;",
        PlaybackFailure::BackendNotCompiled,
    ),
    (
        "EGL device enumeration",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "no EGL device matches",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    ("vaapi-capability: ", PlaybackFailure::GpuInteropUnavailable),
    (
        "open verified DRM render node: ",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "DRM render node changed during open",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "requires GL_EXT_memory_object_fd and GL_EXT_semaphore_fd",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "Vulkan external_memory_fd was not enabled",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "Vulkan external_semaphore_fd was not enabled",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "Vulkan opaque-FD semaphore export unsupported",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "opaque-FD buffer export unsupported or dedicated-only",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "missing GL entry point ",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "ANGLE missing required extension ",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "ANGLE did not report required extensions",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "ANGLE device missing EGL_ANGLE_device_d3d",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "ANGLE did not retain the supplied D3D11On12 device",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "supplied WGPU Dx12 device/queue identities differ",
        PlaybackFailure::GpuInteropUnavailable,
    ),
    (
        "completion proof requires the private EGL context current",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "completion GL fence returned null",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "completion GL wait incomplete: ",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "GPU completion unproven after early failure;",
        PlaybackFailure::GpuCompletionFailed,
    ),
    ("Dx12 device lost;", PlaybackFailure::GpuCompletionFailed),
    (
        "Dx12 interop fence timeout (5s);",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "GPU queue gate poisoned",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "Windows GPU queue gate poisoned;",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "WGPU native import/GPU copy diagnostics timed out (5s)",
        PlaybackFailure::GpuCompletionFailed,
    ),
    (
        "render dimensions exceed device limits",
        PlaybackFailure::RenderSizeLimit,
    ),
    (
        "invalid render dimensions",
        PlaybackFailure::RenderSizeLimit,
    ),
    (
        "buffer exceeds GL address range",
        PlaybackFailure::RenderSizeLimit,
    ),
    (
        "GPU surface video target size is outside GPU limits",
        PlaybackFailure::RenderSizeLimit,
    ),
    (
        "null GPU surface render context",
        PlaybackFailure::RenderInitFailed,
    ),
    (
        "surfaceless EGL display unavailable",
        PlaybackFailure::RenderInitFailed,
    ),
    (
        "EGL desktop OpenGL unavailable",
        PlaybackFailure::RenderInitFailed,
    ),
    (
        "no EGL OpenGL pbuffer config",
        PlaybackFailure::RenderInitFailed,
    ),
    (
        "EGL OpenGL context creation failed",
        PlaybackFailure::RenderInitFailed,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdr_requires_an_exact_recognized_transfer_not_untrusted_substrings() {
        let prefix = "GPU surface SDR only; unqualified transfer function: ";
        for gamma in ["pq", "hlg", "st2084", "arib-std-b67"] {
            assert_eq!(
                PlaybackFailure::from_gpu_message(&format!("{prefix}{gamma}")),
                PlaybackFailure::HdrUnsupported
            );
            assert_eq!(
                PlaybackFailure::from_gpu_message(&format!(
                    "{prefix}{gamma} https://SECRET/private"
                )),
                PlaybackFailure::ColorTransferUnqualified
            );
        }
        for gamma in [
            "",
            "unknown",
            "auto",
            "https://user:SECRET@example.invalid/pq",
            "/private/hlg",
            "PQ",
        ] {
            assert_eq!(
                PlaybackFailure::from_gpu_message(&format!("{prefix}{gamma}")),
                PlaybackFailure::ColorTransferUnqualified
            );
        }
    }

    #[test]
    fn reviewed_prefixes_never_echo_dynamic_details() {
        let secret = "SECRET_MARKER https://user:password@example.invalid/media?token=SECRET /private/user/media C:\\private\\media";
        for &(prefix, expected) in GPU_PREFIXES {
            let input = format!("{prefix}{secret}");
            let failure = PlaybackFailure::from_gpu_message(&input);
            assert_eq!(failure, expected, "{prefix}");
            assert!(!failure.code().contains("SECRET"));
            assert!(!failure.message().contains("SECRET"));
            assert!(!failure.message().contains("https://"));
            assert_eq!(
                PlaybackFailure::from_gpu_message(&format!("{secret}{prefix}")),
                PlaybackFailure::Unknown
            );
        }
    }

    #[test]
    fn every_public_category_has_a_unique_fixed_safe_code_and_message() {
        use PlaybackFailure::*;
        let all = [
            BackendNotCompiled,
            GpuContextUnavailable,
            DecoderNotDirect,
            ColorTransferUnqualified,
            HdrUnsupported,
            ColorMetadataUnavailable,
            DecodeMetadataUnavailable,
            GpuDeviceUnsupported,
            GpuInteropUnavailable,
            GpuCompletionFailed,
            LibmpvUnavailable,
            LibmpvIncompatible,
            RenderInitFailed,
            MediaLoadFailed,
            RenderSizeLimit,
            Unknown,
        ];
        let mut codes = std::collections::HashSet::new();
        for reason in all {
            assert!(codes.insert(reason.code()));
            assert!(
                reason
                    .code()
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c == b'-')
            );
            assert!(!reason.message().is_empty());
            assert!(!reason.message().contains(['/', '\\', '@']));
        }
    }
}
