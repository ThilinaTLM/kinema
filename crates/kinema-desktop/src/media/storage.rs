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
            let asset = Asset {
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
fn missing(ranges: &RangeSet, size: u64, preferred: u64) -> Option<(u64, u64)> {
    let range = ByteRange::new(preferred, size)?;
    ranges
        .missing(range)
        .first()
        .map(|r| (r.start(), r.end().min(r.start().saturating_add(CHUNK))))
}
pub(super) fn check_payload(asset: &Asset) -> Result<(), MediaError> {
    if !asset.dir.symlink_metadata()?.file_type().is_dir() {
        return Err(MediaError::Storage);
    }
    regular(&asset.dir.join("payload"))
}
pub(super) fn acquire(
    shared: &Shared,
    asset: &Asset,
    mut resolved: ResolvedMedia,
    renew: Renew,
) -> Result<(), MediaError> {
    check_payload(asset)?;
    let mut payload = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(asset.dir.join("payload"))?;
    let mut renewals = 0;
    let mut link_age = std::time::Instant::now();
    loop {
        if shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
        }
        let range = {
            let mut state = asset.state.lock().map_err(|_| MediaError::Storage)?;
            let preferred = state.priority.take().unwrap_or(0);
            missing(&state.ranges, asset.identity.size, preferred)
                .or_else(|| missing(&state.ranges, asset.identity.size, 0))
        };
        let Some((start, end)) = range else {
            return Ok(());
        };
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
            renew_media(&mut resolved, &renew, &mut renewals, &asset.identity)?;
            link_age = std::time::Instant::now();
        }
        if shared.canceled.load(Ordering::Relaxed) || asset.cancel.load(Ordering::Relaxed) {
            return Err(MediaError::Canceled);
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
        let mut bytes = Vec::with_capacity((end - start) as usize);
        response
            .by_ref()
            .take(end - start + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| { tracing::warn!(io_kind = ?error.kind(), duration_ms = request_started.elapsed().as_millis() as u64, "Media upstream body read failed"); MediaError::BodyRead })?;
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
        payload.seek(SeekFrom::Start(start))?;
        payload.write_all(&bytes)?;
        payload.sync_data()?;
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
        checkpoint(asset, &durable)?;
        asset.state.lock().map_err(|_| MediaError::Storage)?.ranges = ranges;
        renewals = 0;
        asset.changed.notify_all();
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
