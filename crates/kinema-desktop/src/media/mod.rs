//! Owned, loopback-only media leases and durable, quota-reserved acquisition.
//! All public methods performing I/O must be called from a service worker, not UI rendering.
#[cfg(test)]
mod remote_tests;
mod server;
mod sessions;
pub use sessions::{SessionOrigin, SessionProgress, SessionUpdate};
mod storage;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transport_tests;

use crate::{paths::AppPaths, providers::ResolvedMedia};
use kinema_core::{
    CacheBudget,
    media::{ByteRange, RangeSet},
    sources::MediaIdentity,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt,
    fs::File,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

// Amortize upstream request-header latency while retaining one bounded,
// validated and durable range per acquisition cycle.
const CHUNK: u64 = 4 * 1024 * 1024;
const MAX_ASSETS: usize = 8;
const MAX_SESSIONS: usize = 32;
type Renew = Arc<dyn Fn() -> Result<ResolvedMedia, MediaError> + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaError {
    FileExceedsLimit { size: u64, limit: u64 },
    CacheFull,
    UnknownSize,
    Storage,
    UnsupportedRange,
    Upstream,
    ConnectTimeout,
    RequestTimeout,
    BodyRead,
    RenewalExhausted,
    IdentityMismatch,
    Canceled,
    Busy,
    InvalidLocalFile,
}
impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FileExceedsLimit { .. } => "Media file exceeds the configured cache limit",
            Self::CacheFull => "Media cache or disk is full",
            Self::UnknownSize => "Media size is unknown",
            Self::Storage => "Media storage operation failed",
            Self::UnsupportedRange => "Upstream does not support validated byte ranges",
            Self::Upstream => "Media provider request failed",
            Self::ConnectTimeout => "Media connection timed out during DNS, TCP, or TLS setup",
            Self::RequestTimeout => "Media request timed out before response headers arrived",
            Self::BodyRead => "Media response body could not be read completely (interrupted, timed out, or truncated)",
            Self::RenewalExhausted => "Media link renewal limit reached without download progress",
            Self::IdentityMismatch => "Renewed media identity changed",
            Self::Canceled => "Media session ended",
            Self::Busy => "Media service capacity reached",
            Self::InvalidLocalFile => "Local media is not a regular file",
        })
    }
}
impl std::error::Error for MediaError {}
impl From<std::io::Error> for MediaError {
    fn from(error: std::io::Error) -> Self {
        tracing::warn!(io_kind = ?error.kind(), "Media I/O failed");
        Self::Storage
    }
}
// Keep certificate verification enabled and never follow signed-link redirects.
fn client_builder() -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder()
        // System DNS may consume a five-second retry before TCP/TLS can begin.
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
}
fn request_error(error: reqwest::Error) -> MediaError {
    tracing::warn!(
        kind = crate::logging::http_error_kind(&error),
        "Media transport failed"
    );
    // Never retain or format reqwest errors: they can contain signed URLs.
    if error.is_timeout() {
        if error.is_connect() {
            MediaError::ConnectTimeout
        } else {
            MediaError::RequestTimeout
        }
    } else {
        MediaError::Upstream
    }
}
#[derive(Clone, Debug)]
pub struct PlaybackSession {
    pub id: String,
    pub url: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupReport {
    pub freed_bytes: u64,
    pub protected_files: usize,
}
#[derive(Clone, Debug)]
pub struct UsageSnapshot {
    pub reserved_bytes: u64,
    pub limit_bytes: u64,
    pub downloaded_bytes: u64,
}

struct Asset {
    dir: PathBuf,
    identity: MediaIdentity,
    state: Mutex<AssetState>,
    changed: Condvar,
    cancel: AtomicBool,
    running: AtomicBool,
}
struct AssetState {
    ranges: RangeSet,
    priority: Option<u64>,
    error: Option<MediaError>,
}
#[derive(Clone)]
enum Source {
    Local(Arc<File>, u64, &'static str),
    Remote(Arc<Asset>),
}
#[derive(Clone)]
struct Lease {
    source: Source,
    active: Arc<AtomicBool>,
    path: Option<PathBuf>,
}
impl Lease {
    fn new(source: Source) -> Self {
        Self {
            source,
            active: Arc::new(AtomicBool::new(true)),
            path: None,
        }
    }
}
struct State {
    sessions: HashMap<String, Lease>,
    assets: HashMap<String, Arc<Asset>>,
    limit: u64,
}
struct Shared {
    root: PathBuf,
    state: Mutex<State>,
    canceled: AtomicBool,
    _lock: File,
    client: reqwest::blocking::Client,
}
pub struct MediaService {
    shared: Arc<Shared>,
    address: std::net::SocketAddr,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}
impl MediaService {
    pub fn start(paths: &AppPaths, limit: CacheBudget) -> Result<Self, MediaError> {
        let _span = tracing::info_span!("media", operation = "start").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            let root = storage::resolve_root(&paths.cache)?;
            let (lock, assets) = storage::open(&root)?;
            let reserved = assets
                .values()
                .try_fold(0u64, |sum, a| sum.checked_add(a.identity.size))
                .ok_or(MediaError::Storage)?;
            if reserved > limit.bytes() {
                return Err(MediaError::CacheFull);
            }
            let shared = Arc::new(Shared {
                root,
                state: Mutex::new(State {
                    sessions: HashMap::new(),
                    assets,
                    limit: limit.bytes(),
                }),
                canceled: AtomicBool::new(false),
                _lock: lock,
                client: client_builder().build().map_err(request_error)?,
            });
            let (address, stop) = server::start(shared.clone())?;
            tracing::info!(port = address.port(), "Media server started");
            Ok(Self {
                shared,
                address,
                stop: Some(stop),
            })
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "media operation completed"
            ),
            Err(category) => {
                if *category == MediaError::Canceled {
                    tracing::debug!("media operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "media operation failed"
                    );
                }
            }
        }
        result
    }
    pub fn register_local(&self, path: &Path) -> Result<PlaybackSession, MediaError> {
        let _span = tracing::info_span!("media", operation = "register_local").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            let canonical = path.canonicalize()?;
            if canonical.starts_with(&self.shared.root) {
                return Err(MediaError::InvalidLocalFile);
            }
            let file = File::open(&canonical)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() {
                return Err(MediaError::InvalidLocalFile);
            }
            let session = self.lease(Source::Local(
                Arc::new(file),
                metadata.len(),
                content_type(path),
            ))?;
            if let Some(lease) = self
                .shared
                .state
                .lock()
                .map_err(|_| MediaError::Storage)?
                .sessions
                .get_mut(&session.id)
            {
                lease.path = Some(canonical);
            }
            Ok(session)
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "media operation completed"
            ),
            Err(category) => {
                if *category == MediaError::Canceled {
                    tracing::debug!("media operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "media operation failed"
                    );
                }
            }
        }
        result
    }
    pub fn register_remote(
        &self,
        resolved: ResolvedMedia,
        renew: Renew,
    ) -> Result<PlaybackSession, MediaError> {
        let _span = tracing::info_span!("media", operation = "register_remote").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            if resolved.identity.size == 0 {
                return Err(MediaError::UnknownSize);
            }
            let key = storage::key(&resolved.identity);
            let mut state = self.shared.state.lock().map_err(|_| MediaError::Storage)?;
            if self.shared.canceled.load(Ordering::Relaxed) {
                return Err(MediaError::Canceled);
            }
            if state.sessions.len() >= MAX_SESSIONS {
                return Err(MediaError::Busy);
            }
            let created = !state.assets.contains_key(&key);
            if created
                && state
                    .assets
                    .values()
                    .filter(|a| a.running.load(Ordering::Acquire))
                    .count()
                    >= MAX_ASSETS
            {
                return Err(MediaError::Busy);
            }
            let asset = if let Some(asset) = state.assets.get(&key) {
                if asset.dir != self.shared.root.join(&key)
                    || !asset.dir.symlink_metadata()?.file_type().is_dir()
                {
                    return Err(MediaError::Storage);
                }
                let (reserved, downloaded) = totals(&state)?;
                admission(
                    0,
                    state.limit,
                    reserved,
                    downloaded,
                    fs4::available_space(&self.shared.root)?,
                )?;
                asset.clone()
            } else {
                let size = resolved.identity.size;
                if size > state.limit {
                    return Err(MediaError::FileExceedsLimit {
                        size,
                        limit: state.limit,
                    });
                }
                let (reserved, downloaded) = totals(&state)?;
                admission(
                    size,
                    state.limit,
                    reserved,
                    downloaded,
                    fs4::available_space(&self.shared.root)?,
                )?;
                let asset =
                    match storage::create(&self.shared.root, &key, resolved.identity.clone()) {
                        Ok(asset) => asset,
                        Err(error) => {
                            // Even failed creation/deletion remains reserved if any owned artifact survived.
                            if let Some(asset) = storage::failed_asset(
                                &self.shared.root,
                                &key,
                                resolved.identity.clone(),
                            ) {
                                state.assets.insert(key.clone(), asset);
                            }
                            return Err(error);
                        }
                    };
                state.assets.insert(key.clone(), asset.clone());
                asset
            };
            // A single acquisition owner per active asset; a released asset can restart from its checkpoint.
            let active = state
                .sessions
                .values()
                .any(|s| matches!(&s.source, Source::Remote(a) if Arc::ptr_eq(a, &asset)));
            if !active && asset.running.load(Ordering::Acquire) {
                return Err(MediaError::Busy);
            }
            if !active {
                if state
                    .assets
                    .values()
                    .filter(|a| a.running.load(Ordering::Acquire))
                    .count()
                    >= MAX_ASSETS
                {
                    return Err(MediaError::Busy);
                }
                asset.running.store(true, Ordering::Release);
                asset.cancel.store(false, Ordering::Relaxed);
                asset.state.lock().map_err(|_| MediaError::Storage)?.error = None;
                let shared = self.shared.clone();
                let acquired = asset.clone();
                let spawned = std::thread::Builder::new()
                    .name("media-acquire".into())
                    .spawn(move || {
                        tracing::info!(size = acquired.identity.size, "Media acquisition started");
                        let result = storage::acquire(&shared, &acquired, resolved, renew);
                        match &result {
                            Ok(()) => tracing::info!(
                                size = acquired.identity.size,
                                "Media asset complete"
                            ),
                            Err(MediaError::Canceled) => {
                                tracing::debug!("Media acquisition canceled")
                            }
                            Err(category) => tracing::warn!(?category, "Media acquisition failed"),
                        }
                        if let Err(error) = result
                            && let Ok(mut s) = acquired.state.lock()
                        {
                            s.error = Some(error);
                        }
                        acquired.running.store(false, Ordering::Release);
                        acquired.changed.notify_all();
                    });
                if let Err(error) = spawned {
                    tracing::warn!(io_kind = ?error.kind(), "Media acquisition worker spawn failed");
                    asset.running.store(false, Ordering::Release);
                    if created && storage::discard(&self.shared.root, &asset).is_ok() {
                        state.assets.remove(&key);
                    }
                    return Err(MediaError::Storage);
                }
            }
            tracing::info!("Media session created");
            let id = token();
            state
                .sessions
                .insert(id.clone(), Lease::new(Source::Remote(asset)));
            Ok(PlaybackSession {
                url: format!("http://{}/media/{id}", self.address),
                id,
            })
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "media operation completed"
            ),
            Err(category) => {
                if *category == MediaError::Canceled {
                    tracing::debug!("media operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "media operation failed"
                    );
                }
            }
        }
        result
    }
    fn lease(&self, source: Source) -> Result<PlaybackSession, MediaError> {
        let _span = tracing::info_span!("media", operation = "lease").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            let mut state = self.shared.state.lock().map_err(|_| MediaError::Storage)?;
            if self.shared.canceled.load(Ordering::Relaxed) {
                return Err(MediaError::Canceled);
            }
            if state.sessions.len() >= MAX_SESSIONS {
                return Err(MediaError::Busy);
            }
            tracing::info!("Media session created");
            let id = token();
            state.sessions.insert(id.clone(), Lease::new(source));
            Ok(PlaybackSession {
                url: format!("http://{}/media/{id}", self.address),
                id,
            })
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "media operation completed"
            ),
            Err(category) => {
                if *category == MediaError::Canceled {
                    tracing::debug!("media operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "media operation failed"
                    );
                }
            }
        }
        result
    }
    pub fn release(&self, id: &str) -> Result<(), MediaError> {
        let _span = tracing::info_span!("media", operation = "release").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            let mut state = self.shared.state.lock().map_err(|_| MediaError::Storage)?;
            if let Some(lease) = state.sessions.remove(id) {
                tracing::info!("Media session released");
                lease.active.store(false, Ordering::Release);
                if let Source::Remote(asset) = lease.source
                    && !state
                        .sessions
                        .values()
                        .any(|s| matches!(&s.source, Source::Remote(a) if Arc::ptr_eq(a, &asset)))
                {
                    asset.cancel.store(true, Ordering::Relaxed);
                    asset.changed.notify_all();
                }
            }
            Ok(())
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "media operation completed"
            ),
            Err(category) => {
                if *category == MediaError::Canceled {
                    tracing::debug!("media operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "media operation failed"
                    );
                }
            }
        }
        result
    }
    pub fn set_limit(&self, limit: CacheBudget) -> Result<(), MediaError> {
        let _span = tracing::info_span!("media", operation = "set_limit").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            let mut state = self.shared.state.lock().map_err(|_| MediaError::Storage)?;
            if totals(&state)?.0 > limit.bytes() {
                return Err(MediaError::CacheFull);
            }
            state.limit = limit.bytes();
            Ok(())
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "media operation completed"
            ),
            Err(category) => {
                if *category == MediaError::Canceled {
                    tracing::debug!("media operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "media operation failed"
                    );
                }
            }
        }
        result
    }
    pub fn usage(&self) -> UsageSnapshot {
        let state = self.shared.state.lock().unwrap_or_else(|error| {
            tracing::error!(
                category = "poisoned_lock",
                "Media snapshot recovered poisoned state"
            );
            error.into_inner()
        });
        let (reserved_bytes, downloaded_bytes) = totals(&state).unwrap_or_else(|category| {
            tracing::warn!(?category, "Media usage unavailable");
            (u64::MAX, 0)
        });
        UsageSnapshot {
            reserved_bytes,
            limit_bytes: state.limit,
            downloaded_bytes,
        }
    }
    /// Safe, URL-free asynchronous acquisition errors for currently leased sessions.
    pub fn session_errors(&self) -> Vec<(String, MediaError)> {
        let state = self.shared.state.lock().unwrap_or_else(|error| {
            tracing::error!(
                category = "poisoned_lock",
                "Media snapshot recovered poisoned state"
            );
            error.into_inner()
        });
        state
            .sessions
            .iter()
            .filter_map(|(id, lease)| match &lease.source {
                Source::Remote(asset) => asset
                    .state
                    .lock()
                    .unwrap_or_else(|error| {
                        tracing::error!(
                            category = "poisoned_lock",
                            "Media snapshot recovered poisoned state"
                        );
                        error.into_inner()
                    })
                    .error
                    .clone()
                    .map(|error| (id.clone(), error)),
                Source::Local(..) => None,
            })
            .collect()
    }
    pub fn cleanup_unused(&self) -> Result<CleanupReport, MediaError> {
        let _span = tracing::info_span!("media", operation = "cleanup_unused").entered();
        let started = std::time::Instant::now();
        let result = storage::cleanup(&self.shared);
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "media operation completed"
            ),
            Err(category) => {
                if *category == MediaError::Canceled {
                    tracing::debug!("media operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "media operation failed"
                    );
                }
            }
        }
        result
    }
}
impl Drop for MediaService {
    fn drop(&mut self) {
        tracing::info!(port = self.address.port(), "Media server stop requested");
        self.shared.canceled.store(true, Ordering::Relaxed);
        if let Some(stop) = self.stop.take()
            && stop.send(()).is_err()
        {
            tracing::debug!("Media server shutdown receiver already closed");
        }
    }
}
fn token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn totals(state: &State) -> Result<(u64, u64), MediaError> {
    state
        .assets
        .values()
        .try_fold((0u64, 0u64), |(reserved, downloaded), asset| {
            let bytes = asset
                .state
                .lock()
                .map_err(|_| MediaError::Storage)?
                .ranges
                .total_bytes();
            Ok((
                reserved
                    .checked_add(asset.identity.size)
                    .ok_or(MediaError::Storage)?,
                downloaded.checked_add(bytes).ok_or(MediaError::Storage)?,
            ))
        })
}
fn admission(
    size: u64,
    limit: u64,
    reserved: u64,
    downloaded: u64,
    free: u64,
) -> Result<(), MediaError> {
    if size > limit {
        return Err(MediaError::FileExceedsLimit { size, limit });
    }
    let next = reserved.checked_add(size).ok_or(MediaError::CacheFull)?;
    let pending = reserved
        .checked_sub(downloaded)
        .ok_or(MediaError::Storage)?;
    if next > limit || pending.checked_add(size).ok_or(MediaError::CacheFull)? > free {
        return Err(MediaError::CacheFull);
    }
    Ok(())
}

// Select only static header values; filenames never become response headers or routes.
fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mpeg" | "mpg" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}
