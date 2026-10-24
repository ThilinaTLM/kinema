//! Minimal process-wide GPU diagnostic sink. Never emits raw log arguments/targets.
use std::{
    fmt::{self, Write},
    sync::{
        Once,
        atomic::{AtomicU64, Ordering},
    },
};

static INSTALL: Once = Once::new();
static LOGGER: Validator = Validator;
static PROBLEMS: AtomicU64 = AtomicU64::new(0);
static WARNINGS: AtomicU64 = AtomicU64::new(0);

pub fn install() {
    INSTALL.call_once(|| {
        if log::set_logger(&LOGGER).is_err() {
            PROBLEMS.fetch_add(1, Ordering::Relaxed);
            eprintln!("gpu_validation category=logger_install vuid=unavailable");
            return;
        }
        // Inspect every enabled VUID-bearing record, including warnings, without
        // enabling per-frame Debug/Trace formatting in this performance probe.
        log::set_max_level(log::LevelFilter::Warn);
    });
}

pub fn problem_count() -> u64 {
    PROBLEMS.load(Ordering::Relaxed)
}
pub fn warning_count() -> u64 {
    WARNINGS.load(Ordering::Relaxed)
}

fn category(target: &str) -> Option<&'static str> {
    if target.starts_with("wgpu") {
        Some("wgpu")
    } else if target.starts_with("gpui")
        && (target.contains("render")
            || target.contains("surface")
            || target.starts_with("gpui_wgpu"))
    {
        Some("gpui_render")
    } else {
        None
    }
}

fn is_problem(level: log::Level, gpu_target: bool, has_vuid: bool) -> bool {
    has_vuid || (level == log::Level::Error && gpu_target)
}

struct Validator;
impl log::Log for Validator {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        let mut scan = VuidScan::default();
        let _ = scan.write_fmt(*record.args());
        scan.finish();
        let target = category(record.target());
        if record.level() == log::Level::Warn && target.is_some() {
            WARNINGS.fetch_add(1, Ordering::Relaxed);
        }
        if is_problem(
            record.level(),
            target.is_some(),
            scan.seen || scan.diagnostic.is_some(),
        ) {
            PROBLEMS.fetch_add(1, Ordering::Relaxed);
            eprintln!(
                "gpu_validation category={} vuid={} diagnostic={}",
                target.unwrap_or("validation"),
                scan.code.as_deref().unwrap_or("unavailable"),
                scan.diagnostic.unwrap_or("unavailable"),
            );
        }
    }
    fn flush(&self) {}
}

/// Streaming, bounded extraction: even formatting huge records cannot retain paths.
#[derive(Default)]
struct VuidScan {
    prefix: usize,
    seen: bool,
    token: Option<String>,
    overflow: bool,
    code: Option<String>,
    write_match: usize,
    read_match: usize,
    diagnostic: Option<&'static str>,
}
impl VuidScan {
    fn finish(&mut self) {
        if let Some(token) = self.token.take()
            && !self.overflow
            && self.code.is_none()
            && valid_code(&token)
        {
            self.code = Some(token);
        }
        self.overflow = false;
    }
}
impl Write for VuidScan {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        const PREFIX: &[u8] = b"VUID-";
        for byte in text.bytes() {
            const WRITE: &str = "UNASSIGNED-Threading-MultipleThreads-Write";
            const READ: &str = "UNASSIGNED-Threading-MultipleThreads-Read";
            if advance_match(byte, WRITE.as_bytes(), &mut self.write_match) {
                self.diagnostic = Some(WRITE);
            }
            if advance_match(byte, READ.as_bytes(), &mut self.read_match) {
                self.diagnostic = Some(READ);
            }
            if let Some(token) = self.token.as_mut() {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') {
                    if token.len() < 192 {
                        token.push(byte as char);
                    } else {
                        self.overflow = true;
                    }
                    continue;
                }
                self.finish();
            }
            if byte == PREFIX[self.prefix] {
                self.prefix += 1;
                if self.prefix == PREFIX.len() {
                    self.seen = true;
                    self.token = Some("VUID-".into());
                    self.prefix = 0;
                }
            } else {
                self.prefix = usize::from(byte == b'V');
            }
        }
        Ok(())
    }
}

fn advance_match(byte: u8, pattern: &[u8], progress: &mut usize) -> bool {
    if byte == pattern[*progress] {
        *progress += 1;
        if *progress == pattern.len() {
            *progress = 0;
            return true;
        }
    } else {
        *progress = usize::from(byte == pattern[0]);
    }
    false
}

fn valid_code(token: &str) -> bool {
    let parts: Vec<_> = token.split('-').collect();
    parts.len() >= 4
        && parts[0] == "VUID"
        && (parts[1].starts_with("vk")
            || parts[1].starts_with("Vk")
            || matches!(parts[1], "RuntimeSpirv" | "StandaloneSpirv"))
        && parts[1..parts.len() - 1].iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        && parts
            .last()
            .is_some_and(|last| last.len() == 5 && last.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_only_reviewed_vuid_shape_across_chunks() {
        let mut scan = VuidScan::default();
        scan.write_str("/private/path VU").unwrap();
        scan.write_str("ID-vkQueueSubmit-pWaitSemaphores-03238: secret")
            .unwrap();
        scan.finish();
        assert!(scan.seen);
        assert_eq!(
            scan.code.as_deref(),
            Some("VUID-vkQueueSubmit-pWaitSemaphores-03238")
        );
    }
    #[test]
    fn malformed_and_oversized_identifiers_still_flag_without_emitting_data() {
        for text in [
            "VUID-private-secret-12345".to_string(),
            format!("VUID-{}", "x".repeat(1000)),
        ] {
            let mut scan = VuidScan::default();
            scan.write_str(&text).unwrap();
            scan.finish();
            assert!(scan.seen);
            assert!(scan.code.is_none());
        }
    }
    #[test]
    fn fixed_threading_diagnostics_survive_chunking_without_raw_text() {
        for suffix in ["Write", "Read"] {
            let mut scan = VuidScan::default();
            scan.write_str("private data UNASSIGNED-Threading-Multiple")
                .unwrap();
            scan.write_str(&format!("Threads-{suffix} /private/media"))
                .unwrap();
            scan.finish();
            assert_eq!(
                scan.diagnostic,
                Some(if suffix == "Write" {
                    "UNASSIGNED-Threading-MultipleThreads-Write"
                } else {
                    "UNASSIGNED-Threading-MultipleThreads-Read"
                })
            );
            assert!(scan.code.is_none());
        }
    }

    #[test]
    fn gpu_errors_and_any_vuid_fail_but_plain_warnings_do_not() {
        assert!(is_problem(log::Level::Error, true, false));
        assert!(is_problem(log::Level::Warn, false, true));
        assert!(!is_problem(log::Level::Warn, true, false));
        assert!(!is_problem(log::Level::Error, false, false));
    }

    #[test]
    fn categories_never_echo_targets() {
        assert_eq!(category("wgpu_hal::vulkan::instance"), Some("wgpu"));
        assert_eq!(category("gpui_wgpu::wgpu_renderer"), Some("gpui_render"));
        assert_eq!(category("/private/media"), None);
    }
}
