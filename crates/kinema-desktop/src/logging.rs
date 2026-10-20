//! Desktop diagnostics and explicit redaction helpers.
//!
//! Never log credentials, bearer tokens, debrid/signed URLs, magnet links, full
//! query strings, or `Display`/`Debug` of reqwest errors (including their sources).
//! IMDb/TMDB IDs, durations, status codes, counts, sizes and categories are fine.
//! Helpers protect only their own output, not arbitrary tracing fields or messages.
//!
//! Levels: error = bug/unrecoverable; warn = failed operation the user sees;
//! info = lifecycle (startup, worker/session start/stop, settings saved);
//! debug = per-request timing/outcomes; trace = noisy detail.

use std::{fmt::Write as _, path::Path, sync::Once};

use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::paths::AppPaths;

/// Dependencies stay at info, except particularly noisy transport/render internals.
pub const DEFAULT_FILTER: &str = "info,kinema_desktop=info,kinema_core=info,wgpu_hal=warn,naga=warn,zbus=warn,reqwest=warn,hyper=warn,hyper_util=warn";
const MAX_LOG_FILES: usize = 7;

/// Install stderr + best-effort daily file logging and the chained panic hook.
/// Keep the returned guard in `main` until the desktop exits so queued logs flush.
/// Filesystem setup happens before entering the UI event loop, never in rendering.
/// Only subscriber installation failure is fatal; file/path failures use stderr.
pub fn init() -> Result<Option<WorkerGuard>, Box<dyn std::error::Error + Send + Sync>> {
    let log_dir = AppPaths::resolve().map(|paths| paths.cache.join("logs"));
    let file = match &log_dir {
        Ok(path) => file_writer(path),
        Err(_) => Err(std::io::Error::other("application paths unavailable")),
    };
    let (writer, guard, file_error) = match file {
        Ok((writer, guard)) => (Some(writer), Some(guard), None),
        Err(error) => (None, None, Some(error.kind())),
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(writer.map(|writer| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
        }))
        .try_init()?;
    install_panic_hook();
    if let Some(kind) = file_error {
        tracing::warn!(
            ?kind,
            "File logging unavailable; continuing with stderr only"
        );
    }
    let directory = log_dir
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unavailable".into());
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        build_profile = if cfg!(debug_assertions) { "debug" } else { "release" },
        log_directory = %directory,
        "Kinema starting"
    );
    Ok(guard)
}

fn file_writer(
    directory: &Path,
) -> std::io::Result<(tracing_appender::non_blocking::NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(directory)?;
    let appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("kinema")
        .max_log_files(MAX_LOG_FILES)
        .build(directory)
        .map_err(std::io::Error::other)?;
    Ok(tracing_appender::non_blocking(appender))
}

fn install_panic_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            // Panic messages must follow the same no-secrets policy as log messages.
            tracing::error!(
                thread = thread.name().unwrap_or("unnamed"),
                location = %info.location().map(|location| location.to_string()).unwrap_or_default(),
                message,
                "Thread panicked"
            );
            previous(info);
        }));
    });
}

/// Default URL diagnostic: scheme + host + explicit port only, never userinfo,
/// path, query or fragment. Use this for all provider/debrid/untrusted URLs.
/// Non-hierarchical URLs (including magnets) reveal only their scheme.
pub fn safe_url(url: &reqwest::Url) -> String {
    let mut output = format!("{}://", url.scheme());
    if let Some(host) = url.host() {
        // Host's Display preserves IPv6 brackets; host_str alone does not.
        let _ = write!(output, "{host}");
        if let Some(port) = url.port() {
            let _ = write!(output, ":{port}");
        }
    }
    output
}

/// Include the path ONLY when the caller knows it contains no tokens, signed
/// components, magnet links or personal data (e.g. a fixed public API route).
/// Never includes userinfo, query or fragment. Prefer `safe_url` by default.
/// Non-hierarchical URLs do not expose their potentially secret opaque path.
pub fn safe_url_with_path(url: &reqwest::Url) -> String {
    let mut output = safe_url(url);
    if url.has_host() {
        output.push_str(url.path());
    }
    output
}

/// Classify without formatting the error or any source, which may contain URLs.
/// Reqwest does not expose stable DNS/TLS predicates: these remain `connect`
/// (or `request`), rather than inspecting potentially secret source text.
pub fn http_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_redirect() {
        "redirect"
    } else if error.is_status() {
        "status"
    } else if error.is_decode() {
        "decode"
    } else if error.is_body() {
        "body"
    } else if error.is_request() || error.is_builder() {
        "request"
    } else {
        "other"
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test fixtures and assertions")]
mod tests {
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };

    use super::*;

    #[test]
    fn url_helpers_exclude_credentials_and_handle_ipv6_and_opaque_urls() {
        let url = reqwest::Url::parse(
            "https://user:password@[::1]:8443/public/route?token=secret#private",
        )
        .unwrap();
        assert_eq!(safe_url(&url), "https://[::1]:8443");
        assert_eq!(safe_url_with_path(&url), "https://[::1]:8443/public/route");
        let url = reqwest::Url::parse("magnet:?xt=urn:btih:secret").unwrap();
        assert_eq!(safe_url(&url), "magnet://");
        assert_eq!(safe_url_with_path(&url), "magnet://");
        let url = reqwest::Url::parse("https://example.com/debrid-secret").unwrap();
        assert_eq!(safe_url(&url), "https://example.com");
    }

    #[derive(Clone)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn captured_diagnostics_do_not_leak_url_or_error_secrets() {
        let buffer = Buffer(Arc::default());
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_writer(move || writer.clone())
            .finish();
        let url = reqwest::Url::parse(
            "https://username:password@example.com/opaque-secret?token=query-secret#fragment-secret",
        ).unwrap();
        let error = reqwest::blocking::Client::new()
            .get("not a url")
            .build()
            .unwrap_err()
            .with_url(url.clone());
        assert_eq!(http_error_kind(&error), "request");
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(url = %safe_url(&url), kind = http_error_kind(&error), "Request failed");
            let public = reqwest::Url::parse(
                "https://username:password@example.com/public?token=query-secret#fragment-secret",
            )
            .unwrap();
            tracing::debug!(url = %safe_url_with_path(&public), "Public route");
        });
        let output = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert!(output.contains("https://example.com"));
        assert!(output.contains("request"));
        for secret in [
            "username",
            "password",
            "opaque-secret",
            "query-secret",
            "fragment-secret",
            "?token=",
        ] {
            assert!(!output.contains(secret), "leaked {secret}");
        }
    }

    #[test]
    fn file_writer_creates_directory_and_flushes_without_ansi() {
        let root = tempfile::tempdir().unwrap();
        let logs = root.path().join("logs");
        let (writer, guard) = file_writer(&logs).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(writer)
            .finish();
        tracing::subscriber::with_default(subscriber, || tracing::info!("flush-marker"));
        drop(guard);
        let files: Vec<_> = std::fs::read_dir(logs).unwrap().collect();
        assert_eq!(files.len(), 1);
        let path = files[0].as_ref().unwrap().path();
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("kinema.")
        );
        let output = std::fs::read_to_string(path).unwrap();
        assert!(output.contains("flush-marker"));
        assert!(!output.contains('\u{1b}'));
    }

    #[test]
    fn file_writer_reports_unusable_directory_without_panicking() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        std::fs::write(&file, b"not a directory").unwrap();
        assert!(file_writer(&file.join("logs")).is_err());
    }
}
