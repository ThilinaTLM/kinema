//! Thread-local capture for adapter redaction regression tests only.
use std::{
    cell::RefCell,
    io::Write,
    sync::{Arc, Mutex, OnceLock},
};
use tracing_subscriber::{Layer, layer::SubscriberExt};

// Share one retained dispatcher across parallel tests. Creating/dropping many
// scoped subscribers while other tests exercise the same callsites can race
// interest-cache updates. Buffers, unlike the dispatcher, remain thread-local.
struct Recheck;
impl<S: tracing::Subscriber> Layer<S> for Recheck {
    fn register_callsite(
        &self,
        _: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::sometimes()
    }
}

type Buffer = Arc<Mutex<Vec<u8>>>;
thread_local! {
    static OUTPUT: RefCell<Option<Buffer>> = const { RefCell::new(None) };
}
struct Writer;
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        OUTPUT.with_borrow(|output| {
            if let Some(buffer) = output {
                buffer
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .extend_from_slice(bytes);
            }
        });
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Restore(Option<Buffer>);
impl Drop for Restore {
    fn drop(&mut self) {
        OUTPUT.set(self.0.take());
    }
}

pub(crate) fn capture<T>(action: impl FnOnce() -> T) -> (T, String) {
    static CAPTURE: Mutex<()> = Mutex::new(());
    let _capture = CAPTURE.lock().unwrap_or_else(|error| error.into_inner());
    static DISPATCH: OnceLock<tracing::Dispatch> = OnceLock::new();
    let dispatch = DISPATCH.get_or_init(|| {
        tracing::Dispatch::new(
            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .without_time()
                        .with_writer(|| Writer),
                )
                .with(Recheck),
        )
    });
    let buffer = Buffer::default();
    let _restore = Restore(OUTPUT.replace(Some(buffer.clone())));
    let result = tracing::dispatcher::with_default(dispatch, || {
        tracing::callsite::rebuild_interest_cache();
        action()
    });
    let bytes = buffer
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
