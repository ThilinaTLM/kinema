use super::*;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    torrent: String,
    file_path: String,
    size: u64,
    ranges: Vec<(u64, u64)>,
}
fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn key(identity: &MediaIdentity) -> String {
    let mut hash = Sha256::new();
    hash.update(identity.torrent.as_str());
    hash.update([0]);
    hash.update(identity.file_path.as_bytes());
    hash.update(identity.size.to_le_bytes());
    format!("{:x}", hash.finalize())
}
fn regular(path: &Path) -> Result<(), MediaError> {
    if !path.symlink_metadata()?.file_type().is_file() {
        tracing::warn!(
            category = "not_regular_file",
            "Media storage validation failed"
        );
        return Err(MediaError::Storage);
    }
    Ok(())
}
// Only these exact names belong to us. Never recurse or delete an unknown entry.
fn remove_owned(dir: &Path) -> Result<(), MediaError> {
    if !dir.symlink_metadata()?.file_type().is_dir() {
        tracing::warn!(
            category = "not_directory",
            "Media storage validation failed"
        );
        return Err(MediaError::Storage);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file()
            || !matches!(
                entry.file_name().to_str(),
                Some("payload" | "manifest.json" | "manifest.next")
            )
        {
            tracing::warn!(
                category = "unexpected_owned_entry",
                "Media cleanup refused unsafe entry"
            );
            return Err(MediaError::Storage);
        }
    }
    // Keep manifest until last so failed deletion remains diagnostically recoverable.
    for name in ["manifest.next", "payload", "manifest.json"] {
        match std::fs::remove_file(dir.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    std::fs::remove_dir(dir)?;
    sync_directory(dir.parent().ok_or(MediaError::Storage)?)?;
    Ok(())
}
fn sync_directory(dir: &Path) -> Result<(), MediaError> {
    #[cfg(unix)]
    File::open(dir)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}
pub(super) fn resolve_root(cache: &Path) -> Result<PathBuf, MediaError> {
    // Native parents may legitimately contain OS aliases (macOS /var and /tmp).
    // Resolve them once; subsequent owned operations retain this stable path.
    std::fs::create_dir_all(cache)?;
    let root = cache.canonicalize()?.join("media");
    ensure_root(&root)?;
    Ok(root.canonicalize()?)
}
fn ensure_root(root: &Path) -> Result<(), MediaError> {
    match root.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => {
            tracing::warn!(
                category = "root_type",
                "Media storage root validation failed"
            );
            Err(MediaError::Storage)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(root)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}
pub(super) fn open(root: &Path) -> Result<(File, HashMap<String, Arc<Asset>>), MediaError> {
    let _span = tracing::info_span!("media_storage", operation = "open").entered();
    let started = std::time::Instant::now();
    let result = (|| {
        ensure_root(root)?;
        let lock_path = root.join(".lock");
        if lock_path.symlink_metadata().is_ok() {
            regular(&lock_path)?;
        }
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.try_lock().map_err(|_| {
            tracing::warn!(category = "lock_unavailable", "Media storage lock failed");
            MediaError::Storage
        })?;
        let mut assets = HashMap::new();
        let mut reserved = 0u64;
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(key) = name
                .strip_prefix(".create-")
                .or_else(|| name.strip_prefix(".delete-"))
                && valid_key(key)
            {
                remove_owned(&entry.path())?;
                continue;
            }
            // Preserve unrelated entries. Only known opaque asset directories are interpreted.
            if !valid_key(&name) {
                continue;
            }
            if !entry.file_type()?.is_dir() {
                return Err(MediaError::Storage);
            }
            let dir = entry.path();
            regular(&dir.join("manifest.json"))?;
            regular(&dir.join("payload"))?;
            let pending = dir.join("manifest.next");
            if pending.symlink_metadata().is_ok() {
                regular(&pending)?;
                std::fs::remove_file(pending)?;
            }
            let file = File::open(dir.join("manifest.json"))?;
            if file.metadata()?.len() > 1024 * 1024 {
                return Err(MediaError::Storage);
            }
            let m: Manifest = serde_json::from_reader(file).map_err(|_| {
                tracing::warn!(category = "manifest_json", "Media manifest decode failed");
                MediaError::Storage
            })?;
            let identity = MediaIdentity {
                torrent: kinema_core::sources::InfoHash::new(m.torrent).map_err(|_| {
                    tracing::warn!(
                        category = "manifest_hash",
                        "Media manifest validation failed"
                    );
                    MediaError::Storage
                })?,
                file_path: m.file_path,
                size: m.size,
            };
            if m.version != 1
                || identity.size == 0
                || key(&identity) != name
                || File::open(dir.join("payload"))?.metadata()?.len() != identity.size
            {
                tracing::warn!(
                    category = "manifest_identity_or_size",
                    "Media manifest validation failed"
                );
                return Err(MediaError::Storage);
            }
            reserved = reserved
                .checked_add(identity.size)
                .ok_or(MediaError::Storage)?;
            let mut last = 0;
            let mut ranges = RangeSet::new();
            for (start, end) in m.ranges {
                if start < last || end > identity.size {
                    tracing::warn!(
                        category = "manifest_ranges",
                        "Media manifest validation failed"
                    );
                    return Err(MediaError::Storage);
                }
                ranges.insert(ByteRange::new(start, end).ok_or(MediaError::Storage)?);
                last = end;
            }
            assets.insert(
                name,
                Arc::new(Asset {
                    diagnostic_id: diagnostics::next_id(),
                    dir,
                    identity,
                    state: Mutex::new(AssetState {
                        ranges,
                        priority: None,
                        error: None,
                    }),
                    changed: Condvar::new(),
                    cancel: AtomicBool::new(true),
                    running: AtomicBool::new(false),
                }),
            );
        }
        tracing::info!(
            assets = assets.len(),
            reserved_bytes = reserved,
            "Managed media recovered"
        );
        Ok((lock, assets))
    })();
    match &result {
        Ok(_) => tracing::debug!(
            duration_ms = started.elapsed().as_millis() as u64,
            "media_storage operation completed"
        ),
        Err(category) => {
            if *category == MediaError::Canceled {
                tracing::debug!("media_storage operation canceled");
            } else {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "media_storage operation failed"
                );
            }
        }
    }
    result
}
pub(super) fn create(
    root: &Path,
    key: &str,
    identity: MediaIdentity,
) -> Result<Arc<Asset>, MediaError> {
    let _span = tracing::info_span!("media_storage", operation = "create").entered();
    let started = std::time::Instant::now();
    let result = (|| {
        let staged = root.join(format!(".create-{key}"));
        std::fs::create_dir(&staged)?;
        let result = (|| {
            let file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(staged.join("payload"))?;
            file.set_len(identity.size)?;
            file.sync_all()?;
            // Windows cannot rename a directory with an open descendant, even
            // when that file was opened with delete sharing. Finish payload
            // creation before publishing the staged directory.
            drop(file);
            let asset = Asset {
                diagnostic_id: diagnostics::next_id(),
                dir: staged.clone(),
                identity: identity.clone(),
                state: Mutex::new(AssetState {
                    ranges: RangeSet::new(),
                    priority: None,
                    error: None,
                }),
                changed: Condvar::new(),
                cancel: AtomicBool::new(false),
                running: AtomicBool::new(false),
            };
            checkpoint(&asset, &[])?;
            std::fs::rename(&staged, root.join(key))?;
            sync_directory(root)?;
            tracing::info!(size = identity.size, "Media asset created");
            Ok(Arc::new(Asset {
                dir: root.join(key),
                ..asset
            }))
        })();
        if result.is_err()
            && staged.exists()
            && let Err(category) = remove_owned(&staged)
        {
            tracing::warn!(?category, "Media staging cleanup failed");
        }
        result
    })();
    match &result {
        Ok(_) => tracing::debug!(
            duration_ms = started.elapsed().as_millis() as u64,
            "media_storage operation completed"
        ),
        Err(category) => {
            if *category == MediaError::Canceled {
                tracing::debug!("media_storage operation canceled");
            } else {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "media_storage operation failed"
                );
            }
        }
    }
    result
}
pub(super) fn failed_asset(root: &Path, key: &str, identity: MediaIdentity) -> Option<Arc<Asset>> {
    let final_path = root.join(key);
    let staged = root.join(format!(".create-{key}"));
    let dir = if final_path.symlink_metadata().is_ok() {
        final_path
    } else if staged.symlink_metadata().is_ok() {
        staged
    } else {
        return None;
    };
    Some(Arc::new(Asset {
        diagnostic_id: diagnostics::next_id(),
        dir,
        identity,
        state: Mutex::new(AssetState {
            ranges: RangeSet::new(),
            priority: None,
            error: Some(MediaError::Storage),
        }),
        changed: Condvar::new(),
        cancel: AtomicBool::new(true),
        running: AtomicBool::new(false),
    }))
}
pub(super) fn checkpoint(asset: &Asset, ranges: &[(u64, u64)]) -> Result<(), MediaError> {
    let _span = tracing::info_span!("media_storage", operation = "checkpoint").entered();
    let started = std::time::Instant::now();
    let result = (|| {
        let m = Manifest {
            version: 1,
            torrent: asset.identity.torrent.as_str().into(),
            file_path: asset.identity.file_path.clone(),
            size: asset.identity.size,
            ranges: ranges.to_vec(),
        };
        let tmp = asset.dir.join("manifest.next");
        if tmp.symlink_metadata().is_ok() {
            regular(&tmp)?;
            std::fs::remove_file(&tmp)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)?;
        serde_json::to_writer(&mut file, &m).map_err(|_| {
            tracing::warn!(
                category = "manifest_encoding",
                "Media manifest encode failed"
            );
            MediaError::Storage
        })?;
        file.flush()?;
        file.sync_all()?;
        let temporary =
            tempfile::NamedTempFile::from_parts(file, tempfile::TempPath::try_from_path(tmp)?);
        temporary
            .persist(asset.dir.join("manifest.json"))
            .map_err(|error| MediaError::from(error.error))?;
        sync_directory(&asset.dir)
    })();
    match &result {
        Ok(_) => tracing::trace!(
            duration_ms = started.elapsed().as_millis() as u64,
            "media_storage operation completed"
        ),
        Err(category) => {
            if *category == MediaError::Canceled {
                tracing::debug!("media_storage operation canceled");
            } else {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "media_storage operation failed"
                );
            }
        }
    }
    result
}
pub(super) fn available(ranges: &RangeSet, start: u64, end: u64) -> bool {
    ByteRange::new(start, end).is_some_and(|range| ranges.contains(range))
}
// A reader buffer can overlap durable and missing bytes. Its priority must
// identify actual demand, not the already-durable beginning of that buffer.
pub(super) fn reader_priority(ranges: &RangeSet, start: u64, end: u64) -> Option<u64> {
    ranges
        .missing(ByteRange::new(start, end)?)
        .first()
        .map(|range| range.start())
}
fn missing(ranges: &RangeSet, size: u64, preferred: u64) -> Option<(u64, u64)> {
    let range = ByteRange::new(preferred, size)?;
    ranges
        .missing(range)
        .first()
        .map(|r| (r.start(), r.end().min(r.start().saturating_add(CHUNK))))
}
// A reader republishes its offset while an in-flight range is unavailable.
// Once that offset is durable the stale hint must not steer background prefetch
// beyond it: only an actually missing byte represents outstanding reader demand.
fn next_range(ranges: &RangeSet, size: u64, priority: Option<u64>) -> Option<(u64, u64)> {
    priority
        .filter(|&start| start < size && !available(ranges, start, start + 1))
        .and_then(|start| missing(ranges, size, start))
        .or_else(|| missing(ranges, size, 0))
}
pub(super) fn check_payload(asset: &Asset) -> Result<(), MediaError> {
    if !asset.dir.symlink_metadata()?.file_type().is_dir() {
        return Err(MediaError::Storage);
    }
    regular(&asset.dir.join("payload"))
}
const ACQUISITION_REPORT_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
struct AcquisitionTimings {
    request_headers: Duration,
    body_read: Duration,
    payload_write_sync: Duration,
    checkpoint: Duration,
    cycle: Duration,
}

#[derive(Default)]
struct AcquisitionReport {
    bytes: u64,
    requests: u64,
    elapsed: Duration,
    totals: AcquisitionTimings,
    cycle_max: Duration,
}

struct AcquisitionTelemetry {
    started: std::time::Instant,
    interval: AcquisitionReport,
}

impl AcquisitionTelemetry {
    fn new(started: std::time::Instant) -> Self {
        Self {
            started,
            interval: AcquisitionReport::default(),
        }
    }

    // Only fully validated, durable chunks contribute. Failed/retried requests are
    // not samples. Keep constant-size interval totals, never per-chunk history.
    // Check on completion only: no timer, catch-up reports, or short tail flush.
    fn record(
        &mut self,
        now: std::time::Instant,
        bytes: u64,
        timings: AcquisitionTimings,
    ) -> Option<AcquisitionReport> {
        let interval = &mut self.interval;
        interval.bytes = interval.bytes.saturating_add(bytes);
        interval.requests = interval.requests.saturating_add(1);
        interval.totals.request_headers += timings.request_headers;
        interval.totals.body_read += timings.body_read;
        interval.totals.payload_write_sync += timings.payload_write_sync;
        interval.totals.checkpoint += timings.checkpoint;
        interval.totals.cycle += timings.cycle;
        interval.cycle_max = interval.cycle_max.max(timings.cycle);
        interval.elapsed = now.duration_since(self.started);
        if interval.elapsed < ACQUISITION_REPORT_INTERVAL {
            return None;
        }
        self.started = now;
        Some(std::mem::take(interval))
    }
}

impl AcquisitionReport {
    fn log(&self) {
        let mean_ms = |total: Duration| total.as_secs_f64() * 1000.0 / self.requests as f64;
        tracing::info!(
            target: "kinema_desktop::media_performance",
            interval_bytes = self.bytes,
            interval_completed_requests = self.requests,
            interval_seconds = self.elapsed.as_secs_f64(),
            interval_bytes_per_second = self.bytes as f64 / self.elapsed.as_secs_f64(),
            request_header_mean_ms = mean_ms(self.totals.request_headers),
            body_read_mean_ms = mean_ms(self.totals.body_read),
            payload_write_sync_mean_ms = mean_ms(self.totals.payload_write_sync),
            checkpoint_mean_ms = mean_ms(self.totals.checkpoint),
            total_cycle_mean_ms = mean_ms(self.totals.cycle),
            total_cycle_max_ms = self.cycle_max.as_secs_f64() * 1000.0,
            "Media acquisition completed-chunk interval"
        );
    }
}

pub(super) fn acquire(
    shared: &Shared,
    asset: &Asset,
    resolved: ResolvedMedia,
    renew: Renew,
) -> Result<(), MediaError> {
    let mut observation =
        diagnostics::Observation::start(asset.diagnostic_id, diagnostics::Phase::Open);
    let result = acquire_inner(shared, asset, resolved, renew, &mut observation);
    if let Some(observation) = &observation {
        observation.finish(&result);
    }
    result
}
fn acquire_inner(
    shared: &Shared,
    asset: &Asset,
    mut resolved: ResolvedMedia,
    renew: Renew,
    observation: &mut Option<diagnostics::Observation>,
) -> Result<(), MediaError> {
    // Snapshot opt-in once per acquisition; no environment reads on the hot path.
    let mut telemetry = (std::env::var_os("KINEMA_PLAYER_DIAGNOSTICS").as_deref()
        == Some(std::ffi::OsStr::new("1")))
    .then(|| AcquisitionTelemetry::new(std::time::Instant::now()));
    check_payload(asset)?;
    let mut payload = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(asset.dir.join("payload"))?;
    let mut renewals = 0;
    let mut link_age = std::time::Instant::now();
    let mut request_id = 0u64;
    loop {
        if shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
        }
        let cycle_started = telemetry.as_ref().map(|_| std::time::Instant::now());
        let range = {
            let mut state = asset.state.lock().map_err(|_| MediaError::Storage)?;
            let priority = state.priority.take();
            next_range(&state.ranges, asset.identity.size, priority)
        };
        let Some((start, end)) = range else {
            return Ok(());
        };
        if let Some(o) = observation.as_ref() {
            o.range(start, end, asset.identity.size);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| {
                tracing::warn!(
                    category = "clock_before_epoch",
                    "Media expiry clock unavailable"
                )
            })
            .ok()
            .map(|now| now.as_secs());
        if needs_renewal(resolved.expires_at, link_age.elapsed(), now) {
            if let Some(o) = observation.as_ref() {
                o.phase(diagnostics::Phase::Renewal, request_id);
            }
            renew_media(&mut resolved, &renew, &mut renewals, &asset.identity)?;
            link_age = std::time::Instant::now();
        }
        if shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
        }
        request_id = request_id.saturating_add(1);
        if let Some(o) = observation.as_ref() {
            o.phase(diagnostics::Phase::Headers, request_id);
        }
        let request_started = std::time::Instant::now();
        let mut response = shared
            .client
            .get(resolved.url.as_url().clone())
            .header(reqwest::header::RANGE, format!("bytes={start}-{}", end - 1))
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .map_err(|error| {
                tracing::warn!(
                    kind = crate::logging::http_error_kind(&error),
                    duration_ms = request_started.elapsed().as_millis() as u64,
                    "Media range transport failed"
                );
                request_error(error)
            })?;
        let request_headers = telemetry.as_ref().map(|_| request_started.elapsed());
        tracing::trace!(
            status = response.status().as_u16(),
            bytes = end - start,
            duration_ms = request_started.elapsed().as_millis() as u64,
            "Media upstream range headers"
        );
        if !response.status().is_success() {
            tracing::warn!(
                status = response.status().as_u16(),
                duration_ms = request_started.elapsed().as_millis() as u64,
                "Media upstream range rejected"
            );
        }
        if shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
        }
        if matches!(response.status().as_u16(), 401 | 403) {
            if let Some(o) = observation.as_ref() {
                o.phase(diagnostics::Phase::Renewal, request_id);
            }
            renew_media(&mut resolved, &renew, &mut renewals, &asset.identity)?;
            link_age = std::time::Instant::now();
            continue;
        }
        if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            tracing::warn!(
                status = response.status().as_u16(),
                category = "range_status",
                "Media upstream validation failed"
            );
            return Err(MediaError::UnsupportedRange);
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
            tracing::warn!(category = "identity_mismatch", "Media validation failed");
            return Err(MediaError::IdentityMismatch);
        }
        if let Some(o) = observation.as_ref() {
            o.phase(diagnostics::Phase::Body, request_id);
        }
        let body_started = telemetry.as_ref().map(|_| std::time::Instant::now());
        // Reserve the validation probe byte too, avoiding capacity doubling if
        // an invalid response yields one byte beyond the requested range.
        let mut bytes = Vec::with_capacity((end - start + 1) as usize);
        response
            .by_ref()
            .take(end - start + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| { tracing::warn!(io_kind = ?error.kind(), duration_ms = request_started.elapsed().as_millis() as u64, "Media upstream body read failed"); MediaError::BodyRead })?;
        let body_read = body_started.map(|started| started.elapsed());
        if bytes.len() as u64 != end - start {
            tracing::warn!(
                category = "body_length",
                expected_bytes = end - start,
                actual_bytes = bytes.len(),
                "Media upstream validation failed"
            );
            return Err(MediaError::BodyRead);
        }
        if asset.cancel.load(Ordering::Relaxed) || shared.canceled.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
        }
        if let Some(o) = observation.as_ref() {
            o.phase(diagnostics::Phase::PayloadSync, request_id);
        }
        let payload_started = telemetry.as_ref().map(|_| std::time::Instant::now());
        payload.seek(SeekFrom::Start(start))?;
        payload.write_all(&bytes)?;
        payload.sync_data()?;
        let payload_write_sync = payload_started.map(|started| started.elapsed());
        let mut ranges = asset
            .state
            .lock()
            .map_err(|_| MediaError::Storage)?
            .ranges
            .clone();
        ranges.insert(ByteRange::new(start, end).ok_or(MediaError::Storage)?);
        let durable: Vec<_> = ranges
            .ranges()
            .iter()
            .map(|r| (r.start(), r.end()))
            .collect();
        if let Some(o) = observation.as_ref() {
            o.phase(diagnostics::Phase::Checkpoint, request_id);
        }
        let checkpoint_started = telemetry.as_ref().map(|_| std::time::Instant::now());
        checkpoint(asset, &durable)?;
        let checkpoint_time = checkpoint_started.map(|started| started.elapsed());
        asset.state.lock().map_err(|_| MediaError::Storage)?.ranges = ranges;
        renewals = 0;
        asset.changed.notify_all();
        if let Some(o) = observation.as_mut() {
            o.first_durable(
                bytes.len() as u64,
                request_id,
                [
                    request_headers.unwrap_or_default(),
                    body_read.unwrap_or_default(),
                    payload_write_sync.unwrap_or_default(),
                    checkpoint_time.unwrap_or_default(),
                ],
            );
        }
        if let (Some(telemetry), Some(cycle_started)) = (&mut telemetry, cycle_started) {
            let now = std::time::Instant::now();
            if let Some(report) = telemetry.record(
                now,
                bytes.len() as u64,
                AcquisitionTimings {
                    request_headers: request_headers.unwrap_or_default(),
                    body_read: body_read.unwrap_or_default(),
                    payload_write_sync: payload_write_sync.unwrap_or_default(),
                    checkpoint: checkpoint_time.unwrap_or_default(),
                    cycle: now.duration_since(cycle_started),
                },
            ) {
                report.log();
            }
        }
    }
}
pub(super) fn needs_renewal(expires_at: Option<u64>, age: Duration, now: Option<u64>) -> bool {
    age >= Duration::from_secs(300)
        || expires_at.is_some_and(|expiry| now.is_none_or(|now| now.saturating_add(5) >= expiry))
}
fn renew_media(
    resolved: &mut ResolvedMedia,
    renew: &Renew,
    attempts: &mut u32,
    identity: &MediaIdentity,
) -> Result<(), MediaError> {
    let _span = tracing::info_span!("media_storage", operation = "renew_media").entered();
    let started = std::time::Instant::now();
    let result = (|| {
        if *attempts >= 2 {
            return Err(MediaError::RenewalExhausted);
        }
        *attempts += 1;
        let next = renew()?;
        if &next.identity != identity {
            tracing::warn!(category = "identity_mismatch", "Media validation failed");
            return Err(MediaError::IdentityMismatch);
        }
        *resolved = next;
        Ok(())
    })();
    match &result {
        Ok(_) => tracing::debug!(
            duration_ms = started.elapsed().as_millis() as u64,
            "media_storage operation completed"
        ),
        Err(category) => {
            if *category == MediaError::Canceled {
                tracing::debug!("media_storage operation canceled");
            } else {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "media_storage operation failed"
                );
            }
        }
    }
    result
}
pub(super) fn discard(root: &Path, asset: &Asset) -> Result<(), MediaError> {
    let _span = tracing::info_span!("media_storage", operation = "discard").entered();
    let started = std::time::Instant::now();
    let result = (|| {
        let tombstone = root.join(format!(".delete-{}", key(&asset.identity)));
        if asset.dir.exists() {
            std::fs::rename(&asset.dir, &tombstone)?;
            sync_directory(root)?;
        }
        remove_owned(&tombstone)?;
        tracing::info!(size = asset.identity.size, "Media asset removed");
        Ok(())
    })();
    match &result {
        Ok(_) => tracing::debug!(
            duration_ms = started.elapsed().as_millis() as u64,
            "media_storage operation completed"
        ),
        Err(category) => {
            if *category == MediaError::Canceled {
                tracing::debug!("media_storage operation canceled");
            } else {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "media_storage operation failed"
                );
            }
        }
    }
    result
}
pub(super) fn cleanup(shared: &Shared) -> Result<super::CleanupReport, MediaError> {
    let _span = tracing::info_span!("media_storage", operation = "cleanup").entered();
    let started = std::time::Instant::now();
    let result = (|| {
        let mut state = shared.state.lock().map_err(|_| MediaError::Storage)?;
        // Each HTTP reader and acquisition owner holds an Arc independently of session leases.
        let keys: Vec<_> = state
            .assets
            .iter()
            .filter(|(_, asset)| Arc::strong_count(asset) == 1)
            .map(|(k, _)| k.clone())
            .collect();
        let protected_files = state.assets.len() - keys.len();
        let mut freed = 0u64;
        for key in keys {
            let asset = &state.assets[&key];
            discard(&shared.root, asset)?;
            freed = freed
                .checked_add(asset.identity.size)
                .ok_or(MediaError::Storage)?;
            state.assets.remove(&key);
        }
        tracing::info!(
            freed_bytes = freed,
            protected_files,
            "Media cleanup completed"
        );
        Ok(super::CleanupReport {
            freed_bytes: freed,
            protected_files,
        })
    })();
    match &result {
        Ok(_) => tracing::debug!(
            duration_ms = started.elapsed().as_millis() as u64,
            "media_storage operation completed"
        ),
        Err(category) => {
            if *category == MediaError::Canceled {
                tracing::debug!("media_storage operation canceled");
            } else {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "media_storage operation failed"
                );
            }
        }
    }
    result
}

#[cfg(test)]
mod acquisition_telemetry_tests {
    use super::*;
    use std::time::Instant;

    fn timings(milliseconds: u64) -> AcquisitionTimings {
        AcquisitionTimings {
            request_headers: Duration::from_millis(milliseconds),
            body_read: Duration::from_millis(milliseconds * 2),
            payload_write_sync: Duration::from_millis(milliseconds * 3),
            checkpoint: Duration::from_millis(milliseconds * 4),
            cycle: Duration::from_millis(milliseconds * 10),
        }
    }

    #[test]
    fn completed_chunks_aggregate_at_boundary_and_reset() -> Result<(), &'static str> {
        let start = Instant::now();
        let mut telemetry = AcquisitionTelemetry::new(start);
        assert!(
            telemetry
                .record(start + Duration::from_millis(4999), 256, timings(10))
                .is_none()
        );
        let report = telemetry
            .record(start + Duration::from_secs(5), 512, timings(20))
            .ok_or("missing boundary report")?;
        assert_eq!(report.bytes, 768);
        assert_eq!(report.requests, 2);
        assert_eq!(report.elapsed, Duration::from_secs(5));
        assert_eq!(report.totals.request_headers, Duration::from_millis(30));
        assert_eq!(report.totals.body_read, Duration::from_millis(60));
        assert_eq!(report.totals.payload_write_sync, Duration::from_millis(90));
        assert_eq!(report.totals.checkpoint, Duration::from_millis(120));
        assert_eq!(report.totals.cycle, Duration::from_millis(300));
        assert_eq!(report.cycle_max, Duration::from_millis(200));
        assert!(
            telemetry
                .record(start + Duration::from_secs(6), 128, timings(1))
                .is_none()
        );
        let next = telemetry
            .record(start + Duration::from_secs(10), 128, timings(2))
            .ok_or("missing next interval report")?;
        assert_eq!(next.bytes, 256);
        assert_eq!(next.requests, 2);
        assert_eq!(next.elapsed, Duration::from_secs(5));
        assert_eq!(next.totals.cycle, Duration::from_millis(30));
        assert_eq!(next.cycle_max, Duration::from_millis(20));
        Ok(())
    }

    #[test]
    fn slow_chunk_reports_once_without_catch_up_intervals() -> Result<(), &'static str> {
        let start = Instant::now();
        let mut telemetry = AcquisitionTelemetry::new(start);
        let report = telemetry
            .record(start + Duration::from_secs(16), 262_144, timings(1600))
            .ok_or("missing slow chunk report")?;
        assert_eq!(report.requests, 1);
        assert_eq!(report.elapsed, Duration::from_secs(16));
        assert!(
            telemetry
                .record(start + Duration::from_secs(17), 262_144, timings(100))
                .is_none()
        );
        Ok(())
    }
}

#[cfg(test)]
mod missing_range_tests {
    use super::*;

    #[test]
    fn caps_requests_at_four_mib_and_clips_file_end() {
        assert_eq!(CHUNK, 4 * 1024 * 1024);
        let ranges = RangeSet::new();
        assert_eq!(missing(&ranges, CHUNK * 3, 0), Some((0, CHUNK)));
        assert_eq!(
            missing(&ranges, CHUNK + 123, CHUNK),
            Some((CHUNK, CHUNK + 123))
        );
        assert_eq!(missing(&ranges, CHUNK, CHUNK), None);
        assert_eq!(missing(&ranges, CHUNK, CHUNK + 1), None);
        assert_eq!(missing(&ranges, 0, 0), None);
    }

    #[test]
    fn overlapping_reader_window_prioritizes_missing_suffix_before_background_hole()
    -> Result<(), &'static str> {
        let mut ranges = RangeSet::new();
        ranges.insert(ByteRange::new(0, CHUNK).ok_or("head")?);
        ranges.insert(ByteRange::new(CHUNK * 16, CHUNK * 17).ok_or("seek")?);
        let start = CHUNK * 17 - 32 * 1024;
        let end = start + 64 * 1024;
        let size = CHUNK * 20;
        assert!(available(&ranges, start, start + 1));
        assert!(!available(&ranges, start, end));
        // Publishing Some(start) would be rejected as a satisfied stale hint.
        assert_eq!(
            next_range(&ranges, size, Some(start)),
            Some((CHUNK, CHUNK * 2))
        );
        let priority = reader_priority(&ranges, start, end);
        assert_eq!(priority, Some(CHUNK * 17));
        assert_eq!(
            next_range(&ranges, size, priority),
            Some((CHUNK * 17, CHUNK * 18))
        );
        ranges.insert(ByteRange::new(CHUNK * 17, CHUNK * 18).ok_or("suffix")?);
        // Once the entire reader window is durable, both freshly evaluated and
        // stale priorities correctly fall back to the earliest background gap.
        assert_eq!(reader_priority(&ranges, start, end), None);
        assert_eq!(
            next_range(&ranges, size, priority),
            Some((CHUNK, CHUNK * 2))
        );
        assert_eq!(reader_priority(&ranges, start, start), None);
        Ok(())
    }

    #[test]
    fn satisfied_reader_hint_does_not_skip_earlier_missing_bytes() -> Result<(), &'static str> {
        let mut ranges = RangeSet::new();
        ranges.insert(ByteRange::new(0, CHUNK).ok_or("head")?);
        // The reader repeated its hint while this seek fetch was in flight.
        let hint = CHUNK * 2 + 100;
        assert_eq!(
            next_range(&ranges, CHUNK * 5, Some(hint)),
            Some((hint, hint + CHUNK))
        );
        ranges.insert(ByteRange::new(hint, hint + CHUNK).ok_or("seek")?);
        // Before the fix this selected (hint + CHUNK, hint + CHUNK * 2),
        // delaying the earliest hole by another complete network/durability cycle.
        assert_eq!(
            next_range(&ranges, CHUNK * 5, Some(hint)),
            Some((CHUNK, CHUNK * 2))
        );
        assert_eq!(
            next_range(&ranges, CHUNK * 5, Some(CHUNK * 5)),
            Some((CHUNK, CHUNK * 2))
        );
        Ok(())
    }

    #[test]
    fn preferred_offset_selects_gap_without_crossing_durable_ranges() -> Result<(), &'static str> {
        let mut ranges = RangeSet::new();
        ranges.insert(ByteRange::new(0, CHUNK).ok_or("invalid first range")?);
        ranges.insert(ByteRange::new(CHUNK + 100, CHUNK * 2).ok_or("invalid second range")?);
        // Sequential work clips at the next durable range, even below the cap.
        assert_eq!(missing(&ranges, CHUNK * 4, 0), Some((CHUNK, CHUNK + 100)));
        assert_eq!(
            missing(&ranges, CHUNK * 4, CHUNK + 50),
            Some((CHUNK + 50, CHUNK + 100))
        );
        // The low-level forward scanner advances past durable bytes. The
        // scheduler must reject a satisfied priority before calling it.
        assert_eq!(
            missing(&ranges, CHUNK * 4, CHUNK + 100),
            Some((CHUNK * 2, CHUNK * 3))
        );
        assert_eq!(
            missing(&ranges, CHUNK * 4, CHUNK * 3 + 50),
            Some((CHUNK * 3 + 50, CHUNK * 4))
        );
        assert_eq!(missing(&ranges, CHUNK * 2, CHUNK + 100), None);
        Ok(())
    }
}
