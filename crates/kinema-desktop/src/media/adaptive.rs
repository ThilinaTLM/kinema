//! Bounded fetch waves; only the coordinator writes payloads or publishes checkpoints.
use super::*;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::AtomicUsize,
    time::Instant,
};

const GLOBAL_SLOTS: usize = 16;
const URGENT: u64 = 256 * 1024;
static SLOTS: AtomicUsize = AtomicUsize::new(0);

// Ownership follows the body through the completion queue, not merely the request.
struct Permit;
impl Permit {
    fn take() -> Option<Self> {
        SLOTS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < GLOBAL_SLOTS).then_some(n + 1)
            })
            .ok()
            .map(|_| Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        SLOTS.fetch_sub(1, Ordering::AcqRel);
    }
}

struct Chunk {
    start: u64,
    end: u64,
    bytes: Vec<u8>,
    headers: Duration,
    body: Duration,
    started: Instant,
    etag: Option<reqwest::header::HeaderValue>,
    _permit: Permit,
}
enum Failure {
    Error(MediaError),
    Renew,
    Throttle(Duration),
}
impl From<MediaError> for Failure {
    fn from(value: MediaError) -> Self {
        Self::Error(value)
    }
}

const MAX_READERS: usize = 8;
static NEXT_READER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

struct Reader {
    id: u64,
    cursor: u64,
    waiting: bool,
    window: Instant,
    bytes: u64,
    rate: f64,
}
#[derive(Default)]
pub(super) struct Readers(Vec<Reader>);
#[derive(Clone, Copy, Default)]
struct Feedback {
    active: bool,
    waiting: bool,
    ahead: u64,
    rate: f64,
}
impl Readers {
    fn feedback(&self, ranges: &RangeSet) -> Feedback {
        let mut feedback = Feedback {
            ahead: u64::MAX,
            ..Feedback::default()
        };
        for reader in &self.0 {
            feedback.active = true;
            // Decay stale burst hints when socket delivery stops.
            feedback.rate += reader.rate * (2.0 / reader.window.elapsed().as_secs_f64().max(2.0));
            let ahead = ranges
                .ranges()
                .iter()
                .find(|r| r.start() <= reader.cursor && r.end() > reader.cursor)
                .map_or(0, |r| r.end() - reader.cursor);
            feedback.waiting |= reader.waiting && ahead == 0;
            feedback.ahead = feedback.ahead.min(ahead);
        }
        feedback
    }
    pub(super) fn ready(&mut self, id: u64) {
        if let Some(reader) = self.0.iter_mut().find(|r| r.id == id) {
            reader.waiting = false;
        }
    }
    pub(super) fn waiting(&mut self, id: u64, cursor: u64) {
        if let Some(reader) = self.0.iter_mut().find(|r| r.id == id) {
            reader.cursor = cursor;
            reader.waiting = true;
        }
    }
}
// RAII keeps the observations bounded and removes inactive/aborted streams.
// Delivery is a demand hint only: socket buffering and demux probing are not bitrate.
pub(super) struct ReaderTracker {
    asset: Arc<Asset>,
    pub(super) id: u64,
}
impl ReaderTracker {
    pub(super) fn new(asset: Arc<Asset>, cursor: u64) -> Result<Self, MediaError> {
        let id = NEXT_READER.fetch_add(1, Ordering::Relaxed);
        {
            let mut state = asset.state.lock().map_err(|_| MediaError::Storage)?;
            if state.readers.0.len() < MAX_READERS {
                state.readers.0.push(Reader {
                    id,
                    cursor,
                    waiting: false,
                    window: Instant::now(),
                    bytes: 0,
                    rate: 0.0,
                });
            }
        }
        Ok(Self { asset, id })
    }
    pub(super) fn delivered(&self, cursor: u64, bytes: u64) -> Result<(), MediaError> {
        let mut state = self.asset.state.lock().map_err(|_| MediaError::Storage)?;
        if let Some(reader) = state.readers.0.iter_mut().find(|r| r.id == self.id) {
            reader.cursor = cursor;
            reader.waiting = false;
            reader.bytes = reader.bytes.saturating_add(bytes);
            let elapsed = reader.window.elapsed();
            if elapsed >= Duration::from_secs(1) {
                // Bound the effect of fast local bursts; require sustained buffer
                // pressure as well as this signal before increasing requests.
                let sample = (reader.bytes as f64 / elapsed.as_secs_f64()).min((CHUNK * 4) as f64);
                reader.rate = if reader.rate == 0.0 {
                    sample
                } else {
                    reader.rate * 0.75 + sample * 0.25
                };
                reader.bytes = 0;
                reader.window = Instant::now();
            }
        }
        Ok(())
    }
}
impl Drop for ReaderTracker {
    fn drop(&mut self) {
        if let Ok(mut state) = self.asset.state.lock() {
            state.readers.0.retain(|r| r.id != self.id);
        }
    }
}

struct Controller {
    width: usize,
    failures: u32,
    low_since: Option<Duration>,
    high_since: Option<Duration>,
    changed: Duration,
    healthy_waves: u32,
    useful_rate: f64,
}
impl Controller {
    fn new() -> Self {
        Self {
            width: 1,
            failures: 0,
            low_since: None,
            high_since: None,
            changed: Duration::ZERO,
            healthy_waves: 0,
            useful_rate: 0.0,
        }
    }
    fn success(&mut self, feedback: Feedback, now: Duration, bytes: u64, elapsed: Duration) {
        self.failures = 0;
        self.healthy_waves = self.healthy_waves.saturating_add(1);
        let sample = bytes as f64 / elapsed.as_secs_f64().max(0.001);
        self.useful_rate = if self.useful_rate == 0.0 {
            sample
        } else {
            self.useful_rate * 0.75 + sample * 0.25
        };
        if !feedback.active {
            // Separate background policy: still complete the full file, but do
            // not infer playback pressure from an absent reader.
            self.width = self.width.min(2);
            if self.healthy_waves >= 2 {
                self.width = 2;
            }
            self.low_since = None;
            self.high_since = None;
            return;
        }
        let low = (feedback.rate * 4.0).clamp((URGENT * 2) as f64, (CHUNK * 8) as f64) as u64;
        let high = (feedback.rate * 12.0).clamp(CHUNK as f64, (CHUNK * 16) as f64) as u64;
        let pressure = feedback.waiting
            || (feedback.ahead < low
                && (feedback.ahead < URGENT * 2 || self.useful_rate < feedback.rate * 1.5));
        if pressure {
            self.high_since = None;
            let since = *self.low_since.get_or_insert(now);
            if now.saturating_sub(since) >= Duration::from_secs(1)
                && now.saturating_sub(self.changed) >= Duration::from_secs(2)
            {
                self.width = (self.width + 1).min(4);
                self.changed = now;
                self.low_since = Some(now);
            }
        } else if feedback.ahead >= high {
            self.low_since = None;
            let since = *self.high_since.get_or_insert(now);
            if now.saturating_sub(since) >= Duration::from_secs(2)
                && now.saturating_sub(self.changed) >= Duration::from_secs(2)
            {
                self.width = self.width.saturating_sub(1).max(1);
                self.changed = now;
                self.high_since = Some(now);
            }
        } else {
            self.low_since = None;
            self.high_since = None;
        }
    }
    fn retry(&mut self, throttled: bool, now: Duration) -> Result<Duration, MediaError> {
        self.failures += 1;
        if throttled {
            self.low_since = None;
            self.high_since = None;
            self.width = (self.width / 2).max(1);
            self.changed = now;
            self.healthy_waves = 0;
        }
        if self.failures > 3 {
            return Err(MediaError::Upstream);
        }
        Ok(Duration::from_millis(250 * (1 << (self.failures - 1))))
    }
}
const MIN_RANGE: u64 = 64 * 1024;
struct RangeSizing {
    bytes: u64,
    rate: f64,
}
impl RangeSizing {
    fn new() -> Self {
        Self {
            bytes: URGENT,
            rate: 0.0,
        }
    }
    fn success(&mut self, bytes: u64, body: Duration) {
        let sample = bytes as f64 / body.as_secs_f64().max(0.001);
        self.rate = if self.rate == 0.0 {
            sample
        } else {
            self.rate * 0.5 + sample * 0.5
        };
        let target = (self.rate * 2.0).clamp(MIN_RANGE as f64, CHUNK as f64) as u64;
        self.bytes = target
            .min(self.bytes.saturating_mul(2))
            .clamp(MIN_RANGE, CHUNK);
    }
    fn failed(&mut self) {
        self.bytes = (self.bytes / 2).max(MIN_RANGE);
        self.rate = 0.0;
    }
}
fn strong_etag(value: &reqwest::header::HeaderValue) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2
        && bytes[0] == b'"'
        && bytes[bytes.len() - 1] == b'"'
        && bytes[1..bytes.len() - 1]
            .iter()
            .all(|&b| b == 0x21 || (0x23..=0x7e).contains(&b) || b >= 0x80)
}
fn transient(error: &MediaError) -> bool {
    matches!(
        error,
        MediaError::Upstream
            | MediaError::ConnectTimeout
            | MediaError::RequestTimeout
            | MediaError::BodyRead
    )
}
fn remember_failure(current: &mut Option<MediaError>, next: MediaError) {
    if current.as_ref().is_none_or(transient) {
        *current = Some(next);
    }
}

// A seek invalidates chunk results, never the asset's rate-limit state.
fn rate_limit(failure: &Failure) -> Option<Result<Duration, MediaError>> {
    match failure {
        Failure::Throttle(delay) => Some(Ok(*delay)),
        Failure::Error(MediaError::RetryAfterExceedsBudget) => {
            Some(Err(MediaError::RetryAfterExceedsBudget))
        }
        _ => None,
    }
}
fn stopped(shared: &Shared, asset: &Asset) -> bool {
    shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed)
}
fn canceled(shared: &Shared, asset: &Asset, generation: u64) -> bool {
    stopped(shared, asset)
        || asset
            .state
            .lock()
            .map_or(true, |s| s.generation != generation)
}
fn wait(shared: &Shared, asset: &Asset, duration: Duration) -> Result<(), MediaError> {
    let deadline = Instant::now() + duration;
    loop {
        if shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(());
        }
        let state = asset.state.lock().map_err(|_| MediaError::Storage)?;
        drop(
            asset
                .changed
                .wait_timeout(state, (deadline - now).min(Duration::from_millis(100)))
                .map_err(|_| MediaError::Storage)?,
        );
    }
}

// Never format headers, URLs, or transport error objects.
struct Fetch<'a> {
    shared: &'a Shared,
    asset: &'a Asset,
    resolved: &'a ResolvedMedia,
    generation: u64,
    validator: Option<&'a reqwest::header::HeaderValue>,
}
fn fetch(
    context: Fetch<'_>,
    range: (u64, u64),
    permit: Permit,
    observation: Option<diagnostics::FetchObservation>,
) -> Result<Chunk, Failure> {
    let Fetch {
        shared,
        asset,
        resolved,
        generation,
        validator,
    } = context;
    let (start, end) = range;
    if canceled(shared, asset, generation) {
        return Err(MediaError::Canceled.into());
    }
    let started = Instant::now();
    if let Some(o) = &observation {
        o.phase(diagnostics::Phase::Headers, start, end);
    }
    let mut request = shared
        .client
        .get(resolved.url.as_url().clone())
        .header(reqwest::header::RANGE, format!("bytes={start}-{}", end - 1))
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    if let Some(validator) = validator {
        request = request.header(reqwest::header::IF_RANGE, validator);
    }
    let mut response = request.send().map_err(request_error)?;
    let headers = started.elapsed();
    match response.status().as_u16() {
        401 | 403 => return Err(Failure::Renew),
        429 | 503 => {
            return Err(match retry_after(response.headers()) {
                Some(delay) => Failure::Throttle(delay),
                None => Failure::Error(MediaError::RetryAfterExceedsBudget),
            });
        }
        500 | 502 | 504 => return Err(MediaError::Upstream.into()),
        206 => {}
        200 if validator.is_some() => return Err(MediaError::IdentityMismatch.into()),
        _ => {
            tracing::warn!(
                status = response.status().as_u16(),
                category = "range_status",
                "Media upstream validation failed"
            );
            return Err(MediaError::UnsupportedRange.into());
        }
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .filter(|v| strong_etag(v))
        .cloned();
    if validator.is_some_and(|expected| etag.as_ref() != Some(expected)) {
        return Err(MediaError::IdentityMismatch.into());
    }
    let expected = format!("bytes {start}-{}/{}", end - 1, asset.identity.size);
    if response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        != Some(expected.as_str())
        || response.content_length() != Some(end - start)
        || response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .is_some_and(|v| v != "identity")
    {
        return Err(MediaError::IdentityMismatch.into());
    }
    if let Some(o) = &observation {
        o.phase(diagnostics::Phase::Body, start, end);
    }
    let body_started = Instant::now();
    let mut bytes = Vec::with_capacity((end - start + 1) as usize);
    let mut buffer = [0u8; 64 * 1024];
    while bytes.len() as u64 <= end - start {
        if canceled(shared, asset, generation) {
            return Err(MediaError::Canceled.into());
        }
        let remaining = (end - start + 1 - bytes.len() as u64) as usize;
        let n = response
            .read(&mut buffer[..remaining.min(64 * 1024)])
            .map_err(|_| Failure::Error(MediaError::BodyRead))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    if bytes.len() as u64 != end - start {
        return Err(MediaError::BodyRead.into());
    }
    Ok(Chunk {
        start,
        end,
        bytes,
        headers,
        body: body_started.elapsed(),
        started,
        etag,
        _permit: permit,
    })
}
// Retry-After accepts delta seconds or the HTTP IMF-fixdate form. Requested
// provider delays beyond our budget are terminal, never shortened for a retry.
fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let Some(value) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
    else {
        return Some(Duration::from_secs(1));
    };
    let value = value.trim();
    let seconds = if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        // Numeric overflow is necessarily beyond the budget, not an invalid
        // header to replace with a shorter default delay.
        value.parse::<u64>().ok()?
    } else {
        // Refuse unrecognized date formats rather than risk retrying before
        // a provider's intended minimum. Missing headers alone use a default.
        http_date(value)?.saturating_sub(now)
    };
    (seconds <= 30).then(|| Duration::from_secs(seconds))
}
fn http_date(value: &str) -> Option<u64> {
    let fields: Vec<_> = value.split_ascii_whitespace().take(7).collect();
    if fields.len() != 6 || fields[5] != "GMT" || !fields[0].ends_with(',') {
        return None;
    }
    let day: u64 = fields[1].parse().ok()?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|&m| m == fields[2])?;
    let year: u64 = fields[3].parse().ok()?;
    if !(1970..=9999).contains(&year) {
        return None;
    }
    let leap = |y: u64| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let lengths = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || day > lengths[month] {
        return None;
    }
    let clock: Vec<_> = fields[4].split(':').take(4).collect();
    if clock.len() != 3 {
        return None;
    }
    let hour: u64 = clock[0].parse().ok()?;
    let minute: u64 = clock[1].parse().ok()?;
    let second: u64 = clock[2].parse().ok()?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let leaps_before = |y: u64| (y - 1) / 4 - (y - 1) / 100 + (y - 1) / 400;
    let days = (year - 1970) * 365 + leaps_before(year) - leaps_before(1970)
        + lengths[..month].iter().sum::<u64>()
        + day
        - 1;
    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

pub(super) fn acquire(
    shared: &Shared,
    asset: &Asset,
    mut resolved: ResolvedMedia,
    renew: Renew,
    observation: &mut Option<diagnostics::Observation>,
) -> Result<(), MediaError> {
    storage::check_payload(asset)?;
    let mut payload = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(asset.dir.join("payload"))?;
    let started = Instant::now();
    let mut validator = None;
    let mut validator_initialized = false;
    let mut sizing = RangeSizing::new();
    let mut controller = Controller::new();
    let mut renewals = 0;
    let mut link_age = Instant::now();
    let mut request_id = 0u64;
    let mut telemetry = (std::env::var_os("KINEMA_PLAYER_DIAGNOSTICS").as_deref()
        == Some(std::ffi::OsStr::new("1")))
    .then(|| storage::AcquisitionTelemetry::new(Instant::now()));
    loop {
        if shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
        }
        let (generation, mut covered, priority) = {
            let state = asset.state.lock().map_err(|_| MediaError::Storage)?;
            // Completed cached assets need neither a usable signed URL nor
            // provider renewal; do this before any expiry/callback work.
            if storage::available(&state.ranges, 0, asset.identity.size) {
                return Ok(());
            }
            (state.generation, state.ranges.clone(), state.priority)
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|n| n.as_secs());
        if storage::needs_renewal(resolved.expires_at, link_age.elapsed(), now) {
            if let Some(o) = observation.as_ref() {
                o.phase(diagnostics::Phase::Renewal, request_id);
            }
            storage::renew_media(&mut resolved, &renew, &mut renewals, &asset.identity)?;
            link_age = Instant::now();
        }
        let mut jobs = Vec::with_capacity(4);
        // Begin at reader demand; continue contiguous-ahead before filling old holes.
        let mut preferred = priority;
        for index in 0..controller.width {
            let ahead = preferred
                .filter(|&p| p < asset.identity.size)
                .and_then(|p| ByteRange::new(p, asset.identity.size))
                .and_then(|r| covered.missing(r).first().copied())
                .map(|r| {
                    (
                        r.start(),
                        r.end().min(r.start().saturating_add(sizing.bytes)),
                    )
                });
            let Some((start, mut end)) =
                ahead.or_else(|| storage::next_range(&covered, asset.identity.size, None))
            else {
                break;
            };
            end = end.min(start.saturating_add(sizing.bytes));
            if index == 0
                && (covered.total_bytes() == 0
                    || priority.is_some_and(|p| !storage::available(&covered, p, p + 1)))
            {
                end = end.min(start.saturating_add(URGENT));
            }
            let Some(permit) = Permit::take() else {
                break;
            };
            covered.insert(ByteRange::new(start, end).ok_or(MediaError::Storage)?);
            jobs.push((start, end, permit));
            preferred = Some(end);
        }
        if jobs.is_empty() {
            if covered.total_bytes() == asset.identity.size {
                return Ok(());
            }
            wait(shared, asset, Duration::from_millis(25))?;
            continue;
        }
        let count = jobs.len();
        let wave_started = Instant::now();
        let mut committed = 0u64;
        let mut body_time = Duration::ZERO;
        let mut witnessed_wait = false;
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        let mut failure = None;
        let mut shrink_range = false;
        let mut auth = false;
        let mut throttle: Option<Duration> = None;
        let mut stale = false;
        // Scope joins every fetch even on commit failure; queue and worker count are bounded.
        let wave_validator = validator.clone();
        std::thread::scope(|scope| -> Result<(), MediaError> {
            for (start, end, permit) in jobs {
                let sender = sender.clone();
                let resolved = &resolved;
                let validator = wave_validator.as_ref();
                let fetch_observation = observation.as_ref().map(|o| o.fetch_observation());
                let dispatcher = tracing::dispatcher::get_default(Clone::clone);
                std::thread::Builder::new()
                    .name("media-fetch".into())
                    .spawn_scoped(scope, move || {
                        let result = tracing::dispatcher::with_default(&dispatcher, || {
                            fetch(
                                Fetch {
                                    shared,
                                    asset,
                                    resolved,
                                    generation,
                                    validator,
                                },
                                (start, end),
                                permit,
                                fetch_observation,
                            )
                        });
                        let _ = sender.send(result);
                    })
                    .map_err(|_| MediaError::Storage)?;
            }
            drop(sender);
            for _ in 0..count {
                let result = receiver.recv().map_err(|_| MediaError::Upstream)?;
                if stopped(shared, asset) {
                    return Err(MediaError::Canceled);
                }
                // Origin instructions apply to this asset regardless of which seek
                // issued the request. Classify them before rejecting stale chunks.
                if let Err(error) = &result
                    && let Some(limit) = rate_limit(error)
                {
                    match limit {
                        Ok(delay) => throttle = Some(throttle.unwrap_or_default().max(delay)),
                        Err(error) => remember_failure(&mut failure, error),
                    }
                    continue;
                }
                if canceled(shared, asset, generation) {
                    stale = true;
                    continue;
                }
                match result {
                    Err(Failure::Renew) => auth = true,
                    Err(Failure::Throttle(delay)) => {
                        throttle = Some(throttle.unwrap_or_default().max(delay))
                    }
                    Err(Failure::Error(error)) => {
                        shrink_range |=
                            matches!(error, MediaError::BodyRead | MediaError::RequestTimeout);
                        remember_failure(&mut failure, error);
                    }
                    Ok(chunk) => {
                        if let Some(expected) = validator.as_ref() {
                            if chunk.etag.as_ref() != Some(expected) {
                                remember_failure(&mut failure, MediaError::IdentityMismatch);
                                continue;
                            }
                        } else if !validator_initialized {
                            validator = chunk.etag.clone();
                        }
                        validator_initialized = true;
                        {
                            let state = asset.state.lock().map_err(|_| MediaError::Storage)?;
                            witnessed_wait |= state.readers.feedback(&state.ranges).waiting;
                        }
                        request_id += 1;
                        if let Some(o) = observation.as_ref() {
                            o.range(chunk.start, chunk.end, asset.identity.size);
                            o.phase(diagnostics::Phase::PayloadSync, request_id);
                        }
                        let write_started = Instant::now();
                        payload.seek(SeekFrom::Start(chunk.start))?;
                        payload.write_all(&chunk.bytes)?;
                        payload.sync_data()?;
                        let write_time = write_started.elapsed();
                        if canceled(shared, asset, generation) {
                            stale = true;
                            continue;
                        }
                        let mut ranges = asset
                            .state
                            .lock()
                            .map_err(|_| MediaError::Storage)?
                            .ranges
                            .clone();
                        ranges.insert(
                            ByteRange::new(chunk.start, chunk.end).ok_or(MediaError::Storage)?,
                        );
                        let durable: Vec<_> = ranges
                            .ranges()
                            .iter()
                            .map(|r| (r.start(), r.end()))
                            .collect();
                        let checkpoint_started = Instant::now();
                        if let Some(o) = observation.as_ref() {
                            o.phase(diagnostics::Phase::Checkpoint, request_id);
                        }
                        storage::checkpoint(asset, &durable)?;
                        let checkpoint_time = checkpoint_started.elapsed();
                        let mut state = asset.state.lock().map_err(|_| MediaError::Storage)?;
                        // A seek/cancel during checkpoint must not publish an obsolete
                        // completion. Its validated bytes may safely survive restart;
                        // live availability remains a subset of the durable manifest.
                        if state.generation != generation
                            || asset.cancel.load(Ordering::Relaxed)
                            || shared.canceled.load(Ordering::Relaxed)
                        {
                            stale = true;
                            continue;
                        }
                        state.ranges = ranges;
                        committed += chunk.bytes.len() as u64;
                        body_time += chunk.body;
                        drop(state);
                        asset.changed.notify_all();
                        // The renewal budget is per durable forward progress, not per wave.
                        renewals = 0;

                        if let Some(o) = observation.as_mut() {
                            o.first_durable(
                                chunk.bytes.len() as u64,
                                request_id,
                                [chunk.headers, chunk.body, write_time, checkpoint_time],
                            );
                        }
                        if let Some(t) = telemetry.as_mut()
                            && let Some(report) = t.record(
                                Instant::now(),
                                chunk.bytes.len() as u64,
                                storage::AcquisitionTimings {
                                    request_headers: chunk.headers,
                                    body_read: chunk.body,
                                    payload_write_sync: write_time,
                                    checkpoint: checkpoint_time,
                                    cycle: chunk.started.elapsed(),
                                },
                            )
                        {
                            report.log();
                        }
                    }
                }
            }
            Ok(())
        })?;
        if stopped(shared, asset) {
            return Err(MediaError::Canceled);
        }
        if let Some(error) = failure.as_ref()
            && !transient(error)
        {
            return Err(error.clone());
        }
        // Per-asset cooldown, not an account/origin-global limiter. Waiting from
        // completion of the entire wave is conservative: never earlier than the
        // provider's response-relative minimum. wait() ignores seek generations
        // but exits on real cancellation; the controller retry budget also survives.
        if let Some(delay) = throttle {
            let backoff = controller.retry(true, started.elapsed())?;
            wait(shared, asset, delay.max(backoff))?;
        }
        if stale {
            continue;
        }
        if shrink_range {
            sizing.failed();
        }
        if auth {
            if let Some(o) = observation.as_ref() {
                o.phase(diagnostics::Phase::Renewal, request_id);
            }
            storage::renew_media(&mut resolved, &renew, &mut renewals, &asset.identity)?;
            link_age = Instant::now();
            if failure.is_none() {
                continue;
            }
        }
        if throttle.is_some() {
            continue;
        }
        if failure.is_some() {
            let delay = controller
                .retry(false, started.elapsed())
                .map_err(|_| failure.unwrap_or(MediaError::Upstream))?;
            wait(shared, asset, delay)?;
        } else {
            sizing.success(committed, body_time);
            let mut feedback = {
                let state = asset.state.lock().map_err(|_| MediaError::Storage)?;
                state.readers.feedback(&state.ranges)
            };
            // Sampling only after publication misses the wait that this wave
            // just relieved, and can leave a genuinely starved stream at one.
            feedback.waiting |= witnessed_wait;
            controller.success(
                feedback,
                started.elapsed(),
                committed,
                wave_started.elapsed(),
            );
            renewals = 0;
        }
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    fn tick(c: &mut Controller, feedback: Feedback, seconds: u64) {
        c.success(
            feedback,
            Duration::from_secs(seconds),
            CHUNK,
            Duration::from_secs(2),
        );
    }
    fn pressure() -> Feedback {
        Feedback {
            active: true,
            waiting: true,
            ahead: 0,
            rate: 0.0,
        }
    }
    fn buffered() -> Feedback {
        Feedback {
            active: true,
            waiting: false,
            ahead: CHUNK * 32,
            rate: 128.0 * 1024.0,
        }
    }
    #[test]
    fn completed_cache_never_renews_expired_url() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let service = MediaService::start(
            &super::super::tests::paths(root.path()),
            super::super::tests::budget(CHUNK),
        )?;
        let identity = super::super::tests::identity(16);
        let asset = storage::create(
            &service.shared.root,
            &storage::key(&identity),
            identity.clone(),
        )?;
        storage::checkpoint(&asset, &[(0, 16)])?;
        asset
            .state
            .lock()
            .map_err(|_| "asset lock")?
            .ranges
            .insert(ByteRange::new(0, 16).ok_or("range")?);
        let resolved = ResolvedMedia {
            identity,
            file_name: "movie.mp4".into(),
            url: crate::providers::SensitiveUrl::for_test(
                "http://127.0.0.1:9/unreachable".parse()?,
            ),
            expires_at: Some(0),
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let recorded = calls.clone();
        let renew: Renew = Arc::new(move || {
            recorded.fetch_add(1, Ordering::Relaxed);
            Err(MediaError::Upstream)
        });
        acquire(&service.shared, &asset, resolved, renew, &mut None)?;
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        Ok(())
    }
    #[test]
    fn feedback_uses_contiguous_ahead_not_total_downloaded() {
        let mut ranges = RangeSet::default();
        if let Some(range) = ByteRange::new(0, CHUNK) {
            ranges.insert(range);
        }
        if let Some(range) = ByteRange::new(CHUNK * 2, CHUNK * 4) {
            ranges.insert(range);
        }
        let mut readers = Readers(vec![Reader {
            id: 1,
            cursor: CHUNK / 2,
            waiting: true,
            window: Instant::now(),
            bytes: 0,
            rate: 0.0,
        }]);
        let feedback = readers.feedback(&ranges);
        assert_eq!(feedback.ahead, CHUNK / 2);
        assert!(!feedback.waiting); // a just-satisfied wait is no longer pressure
        readers.waiting(1, CHUNK);
        let feedback = readers.feedback(&ranges);
        assert_eq!(feedback.ahead, 0);
        assert!(feedback.waiting);
    }
    #[test]
    fn low_demand_stays_at_floor_and_high_buffer_shrinks() {
        let mut c = Controller::new();
        for second in 0..12 {
            tick(&mut c, buffered(), second);
        }
        assert_eq!(c.width, 1);
        c.width = 4;
        for second in 12..24 {
            tick(&mut c, buffered(), second);
        }
        assert_eq!(c.width, 1);
    }
    #[test]
    fn sustained_pressure_grows_with_hysteresis() {
        let mut c = Controller::new();
        tick(&mut c, pressure(), 0);
        tick(&mut c, pressure(), 0);
        assert_eq!(c.width, 1); // bursts are not sustained pressure
        tick(&mut c, pressure(), 2);
        assert_eq!(c.width, 2);
        tick(&mut c, pressure(), 3);
        assert_eq!(c.width, 2);
        tick(&mut c, pressure(), 4);
        tick(&mut c, pressure(), 6);
        assert_eq!(c.width, 4);
        tick(&mut c, pressure(), 8);
        assert_eq!(c.width, 4);
    }
    #[test]
    fn sufficient_useful_throughput_does_not_overreact_to_rate_hint() {
        let mut c = Controller::new();
        let feedback = Feedback {
            active: true,
            waiting: false,
            ahead: CHUNK * 3,
            rate: (CHUNK * 2) as f64,
        };
        for second in 0..10 {
            c.success(
                feedback,
                Duration::from_secs(second),
                CHUNK * 4,
                Duration::from_secs(1),
            );
        }
        assert_eq!(c.width, 1);
    }
    #[test]
    fn isolated_failure_does_not_pin_and_throttle_recovers() {
        let mut c = Controller::new();
        for second in 0..7 {
            tick(&mut c, pressure(), second);
        }
        assert_eq!(c.width, 4);
        assert!(c.retry(false, Duration::from_secs(7)).is_ok());
        assert_eq!(c.width, 4);
        assert!(c.retry(true, Duration::from_secs(8)).is_ok());
        assert_eq!(c.width, 2);
        tick(&mut c, pressure(), 8);
        tick(&mut c, pressure(), 9);
        assert_eq!(c.width, 2);
        tick(&mut c, pressure(), 10);
        assert_eq!(c.width, 3);
    }
    #[test]
    fn isolated_failure_during_startup_does_not_erase_pressure() {
        let mut c = Controller::new();
        tick(&mut c, pressure(), 0);
        assert!(c.retry(false, Duration::from_secs(1)).is_ok());
        tick(&mut c, pressure(), 2);
        assert_eq!(c.width, 2);
    }
    #[test]
    fn background_has_separate_conservative_cap() {
        let mut c = Controller::new();
        for second in 0..10 {
            tick(&mut c, Feedback::default(), second);
        }
        assert_eq!(c.width, 2);
        c.width = 4;
        tick(&mut c, Feedback::default(), 11);
        assert_eq!(c.width, 2);
    }
    #[test]
    fn rate_limit_classification_is_independent_of_seek_generation() {
        let delay = Duration::from_secs(2);
        assert_eq!(rate_limit(&Failure::Throttle(delay)), Some(Ok(delay)));
        assert_eq!(
            rate_limit(&Failure::Error(MediaError::RetryAfterExceedsBudget)),
            Some(Err(MediaError::RetryAfterExceedsBudget))
        );
        assert_eq!(rate_limit(&Failure::Error(MediaError::BodyRead)), None);
        // Superseded waves still consume the same throttle retry budget and
        // cannot reset it by changing chunk generation between responses.
        let mut controller = Controller::new();
        controller.width = 4;
        for generation in 0..3 {
            assert!(
                controller
                    .retry(true, Duration::from_secs(generation * 2))
                    .is_ok()
            );
        }
        assert_eq!(controller.width, 1);
        assert!(controller.retry(true, Duration::from_secs(8)).is_err());
    }
    #[test]
    fn terminal_failure_survives_transient_completion_order() {
        for errors in [
            [MediaError::IdentityMismatch, MediaError::BodyRead],
            [MediaError::BodyRead, MediaError::IdentityMismatch],
        ] {
            let mut current = None;
            for error in errors {
                remember_failure(&mut current, error);
            }
            assert_eq!(current, Some(MediaError::IdentityMismatch));
        }
    }
    #[test]
    fn range_sizes_follow_low_rates_and_timeout_recovery() {
        let mut sizing = RangeSizing::new();
        assert_eq!(sizing.bytes, URGENT);
        sizing.success(128 * 1024, Duration::from_secs(2));
        assert_eq!(sizing.bytes, 128 * 1024);
        sizing.failed();
        assert_eq!(sizing.bytes, MIN_RANGE);
        sizing.failed();
        assert_eq!(sizing.bytes, MIN_RANGE);
        sizing.success(1024, Duration::from_secs(10));
        assert_eq!(sizing.bytes, MIN_RANGE);
        for _ in 0..20 {
            sizing.success(CHUNK, Duration::from_millis(1));
        }
        assert_eq!(sizing.bytes, CHUNK);
        sizing.failed();
        assert_eq!(sizing.bytes, CHUNK / 2);
    }
    #[test]
    fn only_strong_entity_tags_bind() {
        for (value, valid) in [
            ("\"abc\"", true),
            ("\"\"", true),
            ("W/\"abc\"", false),
            ("abc", false),
            ("\"a b\"", false),
        ] {
            assert_eq!(
                strong_etag(&reqwest::header::HeaderValue::from_static(value)),
                valid
            );
        }
    }
    #[test]
    fn dates_are_validated() {
        assert_eq!(http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(http_date("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784111777));
        assert_eq!(http_date("Thu, 29 Feb 2024 00:00:00 GMT"), Some(1709164800));
        assert_eq!(http_date("Wed, 29 Feb 2023 00:00:00 GMT"), None);
        assert_eq!(http_date("Thu, 01 Jan 1970 24:00:00 GMT"), None);
    }
    #[test]
    fn bounded_retry_never_shortens_server_minimum() {
        let mut c = Controller::new();
        for _ in 0..3 {
            assert!(c.retry(false, Duration::ZERO).is_ok());
        }
        assert!(c.retry(false, Duration::ZERO).is_err());
        let mut h = reqwest::header::HeaderMap::new();
        for value in [
            "999",
            "999999999999999999999999999999999999",
            "unrecognized-date",
        ] {
            h.insert(
                reqwest::header::RETRY_AFTER,
                reqwest::header::HeaderValue::from_static(value),
            );
            assert_eq!(retry_after(&h), None);
        }
        h.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("2"),
        );
        assert_eq!(retry_after(&h), Some(Duration::from_secs(2)));
    }
}
