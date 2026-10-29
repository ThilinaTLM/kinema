//! Opt-in, bounded startup observations. Never accept native/provider text.
use super::MediaError;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub(super) fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
pub(super) fn enabled() -> bool {
    enabled_value(std::env::var_os("KINEMA_PLAYER_DIAGNOSTICS").as_deref())
}
fn enabled_value(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Phase {
    Open,
    Renewal,
    Headers,
    Body,
    PayloadSync,
    Checkpoint,
    ReaderWait,
    ReaderSend,
}

#[derive(Clone, Copy)]
struct Snapshot {
    phase: Phase,
    request_id: u64,
    since: Instant,
    range_start: u64,
    range_end: u64,
    near_tail: bool,
}

// One timer owner per opt-in acquisition/reader; a single latest snapshot, not a
// per-chunk queue. At most twelve pending reports over this owner's lifetime.
// It never waits for provider I/O and is explicitly stopped and joined on drop.
pub(super) struct Observation {
    asset_id: u64,
    operation_id: u64,
    started: Instant,
    state: std::sync::Arc<std::sync::Mutex<Snapshot>>,
    stop: mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
    first: bool,
    wait_count: u64,
    wait_total: Duration,
    wait_max: Duration,
}
#[derive(Clone)]
pub(super) struct FetchObservation(std::sync::Arc<std::sync::Mutex<Snapshot>>);
impl FetchObservation {
    pub(super) fn phase(&self, phase: Phase, start: u64, end: u64) {
        if let Ok(mut state) = self.0.lock() {
            state.phase = phase;
            state.since = Instant::now();
            state.range_start = start;
            state.range_end = end;
        }
    }
}
impl Observation {
    // Concurrent fetches share a latest-progress snapshot, not per-request history.
    pub(super) fn fetch_observation(&self) -> FetchObservation {
        FetchObservation(self.state.clone())
    }
    pub(super) fn start(asset_id: u64, phase: Phase) -> Option<Self> {
        if !enabled() {
            return None;
        }
        Self::with_interval(asset_id, phase, Duration::from_secs(5))
    }
    fn with_interval(asset_id: u64, phase: Phase, interval: Duration) -> Option<Self> {
        let started = Instant::now();
        let operation_id = next_id();
        let state = std::sync::Arc::new(std::sync::Mutex::new(Snapshot {
            phase,
            request_id: 0,
            since: started,
            range_start: 0,
            range_end: 0,
            near_tail: false,
        }));
        let snapshot = state.clone();
        let (stop, stopped) = mpsc::channel();
        let dispatcher = tracing::dispatcher::get_default(Clone::clone);
        let worker = std::thread::Builder::new()
            .name("media-diagnostics".into())
            .spawn(move || {
                tracing::dispatcher::with_default(&dispatcher, || {
                    for _ in 0..12 {
                        match stopped.recv_timeout(interval) {
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                let Ok(current) = snapshot.lock().map(|s| *s) else {
                                    return;
                                };
                                tracing::info!(target: "kinema_desktop::media_performance",
                            asset_id, operation_id, request_id = current.request_id,
                            phase = ?current.phase,
                            range_start = current.range_start, range_end = current.range_end,
                            near_tail = current.near_tail,
                            phase_ms = current.since.elapsed().as_millis() as u64,
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            "Media operation still pending");
                            }
                            _ => return,
                        }
                    }
                })
            })
            .ok()?;
        Some(Self {
            asset_id,
            operation_id,
            started,
            state,
            stop,
            worker: Some(worker),
            first: true,
            wait_count: 0,
            wait_total: Duration::ZERO,
            wait_max: Duration::ZERO,
        })
    }
    pub(super) fn phase(&self, phase: Phase, request_id: u64) {
        if let Ok(mut state) = self.state.lock() {
            *state = Snapshot {
                phase,
                request_id,
                since: Instant::now(),
                ..*state
            };
        }
    }
    pub(super) fn range(&self, start: u64, end: u64, size: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.range_start = start;
            state.range_end = end;
            state.near_tail = start >= size.saturating_sub(super::CHUNK);
        }
    }
    pub(super) fn range_wait(&mut self, elapsed: Duration) {
        self.wait_count = self.wait_count.saturating_add(1);
        self.wait_total = self.wait_total.saturating_add(elapsed);
        self.wait_max = self.wait_max.max(elapsed);
    }
    pub(super) fn first_durable(&mut self, bytes: u64, request_id: u64, stages: [Duration; 4]) {
        if !self.first {
            return;
        }
        self.first = false;
        tracing::info!(target: "kinema_desktop::media_performance",
            asset_id = self.asset_id, operation_id = self.operation_id,
            bytes, request_id,
            header_ms = stages[0].as_secs_f64() * 1000.0,
            body_ms = stages[1].as_secs_f64() * 1000.0,
            payload_sync_ms = stages[2].as_secs_f64() * 1000.0,
            checkpoint_ms = stages[3].as_secs_f64() * 1000.0,
            elapsed_ms = self.started.elapsed().as_millis() as u64,
            "Media first durable range");
    }
    pub(super) fn first_delivery(&mut self) {
        if !self.first {
            return;
        }
        self.first = false;
        tracing::info!(target: "kinema_desktop::media_performance",
            asset_id = self.asset_id, operation_id = self.operation_id,
            elapsed_ms = self.started.elapsed().as_millis() as u64,
            "Media reader first queued bytes");
    }
    pub(super) fn finish(&self, result: &Result<(), MediaError>) {
        if let Ok(state) = self.state.lock() {
            tracing::info!(target: "kinema_desktop::media_performance",
                asset_id = self.asset_id, operation_id = self.operation_id,
                request_id = state.request_id, phase = ?state.phase,
                phase_ms = state.since.elapsed().as_millis() as u64,
                elapsed_ms = self.started.elapsed().as_millis() as u64,
                range_start = state.range_start, range_end = state.range_end,
                near_tail = state.near_tail,
                range_wait_count = self.wait_count,
                range_wait_total_ms = self.wait_total.as_secs_f64() * 1000.0,
                range_wait_max_ms = self.wait_max.as_secs_f64() * 1000.0,
                outcome = if result.is_ok() { "success" } else { "failure" },
                category = ?result.as_ref().err(),
                "Media operation ended");
        }
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };

    #[test]
    fn diagnostics_are_exact_opt_in() {
        assert!(!enabled_value(None));
        for value in ["", "0", "true", "01", " 1", "1 "] {
            assert!(!enabled_value(Some(std::ffi::OsStr::new(value))));
        }
        assert!(enabled_value(Some(std::ffi::OsStr::new("1"))));
    }

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| std::io::Error::other("capture lock"))?
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn pending_reports_are_capped_correlated_and_joined() -> Result<(), Box<dyn std::error::Error>>
    {
        let capture = Capture::default();
        let writer = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(
            subscriber,
            || -> Result<(), Box<dyn std::error::Error>> {
                // Test-only short clock; no global environment mutation or native work.
                let mut observation =
                    Observation::with_interval(41, Phase::Headers, Duration::from_millis(1))
                        .ok_or("observer unavailable")?;
                observation.range(900, 1000, 1000);
                observation.phase(Phase::Body, 7);
                if let Some(worker) = observation.worker.take() {
                    worker.join().map_err(|_| "timer panicked")?;
                }
                let state = observation.state.lock().map_err(|_| "state lock")?;
                assert_eq!(state.request_id, 7);
                assert!(state.near_tail);
                drop(state);
                observation.range_wait(Duration::from_millis(30));
                observation.range_wait(Duration::from_millis(20));
                observation.finish(&Err(MediaError::BodyRead));
                drop(observation);
                Ok(())
            },
        )?;
        let output = String::from_utf8(capture.0.lock().map_err(|_| "capture lock")?.clone())?;
        assert_eq!(output.matches("Media operation still pending").count(), 12);
        for field in [
            "asset_id=41",
            "request_id=7",
            "phase=Body",
            "range_start=900",
            "range_end=1000",
            "near_tail=true",
            "range_wait_count=2",
            "range_wait_total_ms=50",
            "range_wait_max_ms=30",
            "outcome=\"failure\"",
            "Some(BodyRead)",
        ] {
            assert!(
                output.contains(field),
                "missing fixed/numeric field: {field}"
            );
        }
        for forbidden in [
            "http://",
            "https://",
            "token=",
            "session_id",
            "file_path",
            "url=",
        ] {
            assert!(!output.contains(forbidden));
        }
        Ok(())
    }

    #[test]
    fn first_events_once_and_drop_interrupts_timer_on_worker()
    -> Result<(), Box<dyn std::error::Error>> {
        let capture = Capture::default();
        let writer = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(
            subscriber,
            || -> Result<(), Box<dyn std::error::Error>> {
                let mut observation =
                    Observation::with_interval(42, Phase::Open, Duration::from_secs(3600))
                        .ok_or("observer unavailable")?;
                let state = observation.state.clone();
                observation.first_durable(64, 1, [Duration::from_millis(2); 4]);
                observation.first_durable(64, 2, [Duration::ZERO; 4]);
                observation.finish(&Ok(()));
                // Drop's join is exercised on a synthetic acquisition worker, not UI.
                std::thread::spawn(move || drop(observation))
                    .join()
                    .map_err(|_| "drop panicked")?;
                assert_eq!(Arc::strong_count(&state), 1);
                Ok(())
            },
        )?;
        let output = String::from_utf8(capture.0.lock().map_err(|_| "capture lock")?.clone())?;
        assert_eq!(output.matches("Media first durable range").count(), 1);
        assert!(!output.contains("Media operation still pending"));
        assert!(output.contains("outcome=\"success\""));
        assert!(output.contains("header_ms=2"));
        Ok(())
    }
}
