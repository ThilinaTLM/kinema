//! Per-test, thread-scoped diagnostic capture. No global subscriber or buffers.
use std::{
    io::Write,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Run the adapter synchronously on this test's thread. Spawned fixture servers
/// need no subscriber; assertions must not rely on their worker-thread output.
pub(crate) fn capture<T>(action: impl FnOnce() -> T) -> (T, String) {
    let buffer = Buffer::default();
    let writer = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .without_time()
        .with_writer(move || writer.clone())
        .finish();
    let result = tracing::subscriber::with_default(subscriber, action);
    let bytes = buffer
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    (result, String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn excludes(output: &str, secrets: &[&str]) {
    assert!(!output.is_empty(), "diagnostic capture must not be vacuous");
    for secret in secrets {
        assert!(
            !output.contains(secret),
            "adapter diagnostic leaked a sensitive fixture value"
        );
    }
}

/// Tracing's callsite-interest registry is process-global even for scoped
/// subscribers. Run log assertions in an exact-test subprocess so unrelated
/// parallel tests cannot initialize/rebuild the same production callsites.
/// The child still uses `with_default` on its own test thread; no global
/// subscriber is installed and no fixture worker's output is relied upon.
pub(crate) fn isolated(module: &str, name: &str) -> bool {
    const CHILD: &str = "KINEMA_DIAGNOSTIC_TEST_CHILD";
    let module = module.split_once("::").map_or(module, |(_, module)| module);
    let name = format!("{module}::{name}");
    if std::env::var_os(CHILD).is_some_and(|value| value == name.as_str()) {
        return true;
    }
    let executable = std::env::current_exe()
        .unwrap_or_else(|error| panic!("test executable unavailable: {:?}", error.kind()));
    let output = std::process::Command::new(executable)
        .args(["--exact", &name, "--nocapture", "--test-threads=1"])
        .env(CHILD, &name)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "isolated diagnostic test could not start: {:?}",
                error.kind()
            )
        });
    assert!(
        output.status.success(),
        "isolated diagnostic test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "isolated diagnostic test was not selected"
    );
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn capture_is_thread_scoped() {
        if !super::isolated(module_path!(), "capture_is_thread_scoped") {
            return;
        }
        let (_, output) = super::capture(|| {
            tracing::warn!("same-thread-marker");
            std::thread::spawn(|| tracing::warn!("unrelated-thread-secret"))
                .join()
                .unwrap_or_else(|_| panic!("fixture thread panicked"));
        });
        assert!(output.contains("same-thread-marker"));
        assert!(!output.contains("unrelated-thread-secret"));
    }
}
