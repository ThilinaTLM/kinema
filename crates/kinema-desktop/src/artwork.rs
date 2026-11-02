//! App-owned bounded artwork. Call `request` from event handlers (or rendering: it
//! only queues work); all transport and decoding runs on an owned, bounded thread pool.
//! UI elements should use returned RenderImages directly, not a RetainAll cache.
use gpui::{App, BorrowAppContext, Global, RenderImage, Task};
use image::{ImageFormat, ImageReader};
use reqwest::{Url, blocking::Client, redirect::Policy};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

const ACTIVE: usize = 8;
const QUEUED: usize = 128;
const INTERACTIVE_RESERVED: usize = 2;
const ENCODED: u64 = 4 * 1024 * 1024;
const PIXELS: u64 = 8 * 1024 * 1024;
const EDGE: u32 = 8192;
// Policy budgets, not total CPU/GPU RAM guarantees.
const ENTRIES: usize = 256;
const POSTER_BYTES: usize = 192 * 1024 * 1024;
const BACKDROP_BYTES: usize = 16 * 1024 * 1024;
const DISK_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArtworkKind {
    Poster,
    Backdrop,
    Still,
}
impl ArtworkKind {
    fn thumbnail_bounds(self) -> (u32, u32) {
        match self {
            Self::Poster => (384, 576),
            Self::Backdrop => (1024, 576),
            Self::Still => (480, 270),
        }
    }
    fn quota(self) -> usize {
        match self {
            Self::Poster => POSTER_BYTES,
            Self::Backdrop => 16 * 1024 * 1024,
            Self::Still => 64 * 1024 * 1024,
        }
    }
}
/// Stable identity includes the exact URL: changed artwork never reuses stale pixels.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtworkKey {
    pub kind: ArtworkKind,
    url: String,
    blurred: bool,
}
impl ArtworkKey {
    pub fn new(kind: ArtworkKind, url: &str) -> Option<Self> {
        let parsed = Url::parse(url).ok()?;
        (url.len() <= 2048
            && matches!(parsed.scheme(), "https" | "http")
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.fragment().is_none()
            && parsed.query().is_none())
        .then(|| Self {
            kind,
            url: parsed.to_string(),
            blurred: false,
        })
    }
}
impl ArtworkKey {
    fn source(&self) -> Self {
        Self {
            blurred: false,
            ..self.clone()
        }
    }
    fn blurred(&self) -> Self {
        Self {
            blurred: true,
            ..self.clone()
        }
    }
}

/// Cinematic variant of either a poster or fanart key. Uses the same owner epoch,
/// negative-cache/retry policy and worker slots as ordinary artwork. Cached source
/// pixels are reused; downscaling, blur and grading run only on the worker.
/// Begin the owner's generation outside render, as for `lookup_or_request`.
pub fn backdrop_or_request(
    owner: &str,
    generation: u64,
    key: &ArtworkKey,
    cx: &mut App,
) -> ArtworkState {
    if generation == 0 {
        return ArtworkState::Failed;
    }
    let key = key.blurred();
    if let Some(image) = get(&key, cx) {
        return ArtworkState::Ready(image);
    }
    if matches!(status(&key, cx), Some(Err(_))) {
        return ArtworkState::Failed;
    }
    request(owner, generation, key, cx);
    ArtworkState::Loading
}
/// Read a cached cinematic variant without queuing work or touching the LRU.
/// Use `backdrop_or_request` to distinguish failure from missing/pending work.
pub fn backdrop(key: &ArtworkKey, cx: &App) -> Option<Arc<RenderImage>> {
    cx.try_global::<ArtworkStore>()?
        .cache
        .iter()
        .find(|entry| entry.key == key.blurred())?
        .result
        .as_ref()
        .ok()
        .map(|decoded| decoded.image.clone())
}

/// Muted sRGB colors, computed once on the decode worker (never during render).
/// Byte channels keep every public palette finite, including caller-built values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtworkPalette {
    pub primary: [u8; 3],
    pub secondary: [u8; 3],
}
impl ArtworkPalette {
    pub const NEUTRAL: Self = Self {
        primary: [96, 100, 108],
        secondary: [84, 88, 96],
    };
}

// A fixed 24×24 midpoint sample and 3-bit/channel histogram bound decode work
// independently of source size. Iteration order resolves frequency ties stably.
fn extract_palette(image: &image::RgbaImage) -> ArtworkPalette {
    if image.width() == 0 || image.height() == 0 {
        return ArtworkPalette::NEUTRAL;
    }
    let mut bins = [(0u32, [0u32; 3]); 512];
    for y in 0..24u64 {
        for x in 0..24u64 {
            let pixel = image
                .get_pixel(
                    (((2 * x + 1) * u64::from(image.width())) / 48) as u32,
                    (((2 * y + 1) * u64::from(image.height())) / 48) as u32,
                )
                .0;
            let [r, g, b, a] = pixel;
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            if a < 192 || max < 24 || min > 232 || max - min < 16 {
                continue;
            }
            let index = ((r as usize >> 5) << 6) | ((g as usize >> 5) << 3) | (b as usize >> 5);
            let (count, sums) = &mut bins[index];
            *count += 1;
            for (sum, channel) in sums.iter_mut().zip([r, g, b]) {
                *sum += u32::from(channel);
            }
        }
    }
    let mut ranked: Vec<_> = bins
        .iter()
        .enumerate()
        .filter(|(_, bin)| bin.0 > 0)
        .collect();
    ranked.sort_by_key(|(index, bin)| (std::cmp::Reverse(bin.0), *index));
    let Some((_, first)) = ranked.first() else {
        return ArtworkPalette::NEUTRAL;
    };
    let average = |bin: &(u32, [u32; 3])| bin.1.map(|sum| sum as f32 / bin.0 as f32 / 255.0);
    let primary = average(first);
    let secondary = ranked
        .iter()
        .skip(1)
        .map(|(_, bin)| average(bin))
        .find(|color| {
            color
                .iter()
                .zip(primary)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                > 0.35
        })
        .unwrap_or(primary);
    ArtworkPalette {
        primary: mute_color(primary),
        secondary: mute_color(secondary),
    }
}
fn mute_color(color: [f32; 3]) -> [u8; 3] {
    let max = color.into_iter().fold(0.0f32, f32::max);
    let min = color.into_iter().fold(1.0f32, f32::min);
    // HSV saturation <= 0.32, value in 0.32..=0.58. No bright/neon wash.
    let mix = (0.32 * max / (max - min).max(0.001)).min(1.0);
    let value = max.clamp(0.32, 0.58);
    color.map(|channel| (((max + (channel - max) * mix) / max) * value * 255.0).round() as u8)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkError {
    Transport,
    Unavailable,
    CachedMiss(SystemTime),
    TooLarge,
    InvalidImage,
}
#[derive(Clone)]
pub enum ArtworkState {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}
/// Mounted cards may call this during render. Begin/invalidate the owner's epoch
/// on route changes, never in render. Embedded fixture posters bypass this API.
pub fn lookup_or_request(owner: &str, generation: u64, url: &str, cx: &mut App) -> ArtworkState {
    if generation == 0 {
        return ArtworkState::Failed;
    }
    let Some(key) = ArtworkKey::new(ArtworkKind::Poster, url) else {
        return ArtworkState::Failed;
    };
    if let Some(image) = get(&key, cx) {
        return ArtworkState::Ready(image);
    }
    if matches!(status(&key, cx), Some(Err(_))) {
        return ArtworkState::Failed;
    }
    request(owner, generation, key, cx);
    ArtworkState::Loading
}
#[derive(Clone)]
struct Job {
    id: u64,
    owner: String,
    generation: u64,
    key: ArtworkKey,
    interactive: bool,
    queued_at: Instant,
}
#[derive(Clone)]
struct DecodedArtwork {
    image: Arc<RenderImage>,
    palette: ArtworkPalette,
}
fn expired(result: &ArtworkResult) -> bool {
    matches!(result, Err(ArtworkError::CachedMiss(until)) if SystemTime::now() >= *until)
}
struct Cached {
    key: ArtworkKey,
    result: Result<DecodedArtwork, ArtworkError>,
    bytes: usize,
}
#[derive(Default)]
struct ArtworkStore {
    generations: HashMap<String, u64>,
    queue: VecDeque<Job>,
    active: HashMap<u64, Job>,
    tasks: HashMap<u64, Task<()>>,
    timers: HashMap<u64, Task<()>>,
    cache: VecDeque<Cached>,
    next: u64,
    newest_owner: Option<String>,
    requested: HashMap<String, Vec<String>>,
    title_scopes: HashMap<String, String>,
    retry: std::collections::HashSet<String>,
    workers: Option<ArtworkWorkers>,
}
impl Global for ArtworkStore {}
pub fn init(cx: &mut App) {
    if !cx.has_global::<ArtworkStore>() {
        cx.set_global(ArtworkStore::default());
    }
}
/// Invalidate queued work for one view. Running work retains its global slot until
/// completion; its obsolete result is rejected, not detached/cancelled and replaced.
/// Explicit route/refresh boundaries also clear negative cache entries globally,
/// allowing transient failures to retry. Ready artwork remains cached; no timers
/// or automatic retries are introduced.
/// Use stable view identifiers, not per-title/query strings. At most 64 owners and
/// 128-byte owner names are accepted; zero denotes a rejected registration.
/// Explicit refresh clears this title's persisted misses, including prior mounts.
/// Requested URLs also retry on workers, including older unscoped markers.
/// Route changes alone never erase persisted misses.
pub fn refresh_misses(owner: &str, cx: &mut App) {
    init(cx);
    let receiver = cx.update_global::<ArtworkStore, _>(|store, _| {
        let urls = store.requested.get(owner).cloned().unwrap_or_default();
        store.retry.clear();
        store.retry.extend(urls.iter().cloned());
        let (reply, receiver) = tokio::sync::oneshot::channel();
        let workers = store.workers.get_or_insert_with(ArtworkWorkers::new);
        if workers
            .sender
            .try_send(WorkerCommand::Clear(
                urls,
                store.title_scopes.get(owner).cloned(),
                reply,
            ))
            .is_err()
        {
            tracing::warn!("Artwork miss clearing queue unavailable");
        }
        store.next = store.next.wrapping_add(1);
        (store.next, receiver)
    });
    let (id, receiver) = receiver;
    let task = cx.spawn(async move |cx| {
        let _ = receiver.await;
        cx.update(|cx| {
            cx.update_global::<ArtworkStore, _>(|store, _| {
                store.tasks.remove(&id);
            });
        });
    });
    cx.update_global::<ArtworkStore, _>(|store, _| {
        store.tasks.insert(id, task);
    });
}
fn permanent(error: ArtworkError) -> bool {
    matches!(
        error,
        ArtworkError::Unavailable | ArtworkError::InvalidImage | ArtworkError::CachedMiss(_)
    )
}
pub fn begin_generation(owner: &str, cx: &mut App) -> u64 {
    init(cx);
    cx.update_global::<ArtworkStore, _>(|store, _| store.advance_generation(owner))
}
/// A mounted title tracks its requested URLs so explicit Refresh can clear all
/// of its misses, including earlier seasons. Browse windows need no URL history.
pub fn begin_title_generation(owner: &str, title: &str, cx: &mut App) -> u64 {
    let generation = begin_generation(owner, cx);
    if generation != 0 {
        cx.update_global::<ArtworkStore, _>(|store, _| {
            store.requested.insert(owner.to_owned(), Vec::new());
            store
                .title_scopes
                .insert(owner.to_owned(), title.to_owned());
        });
    }
    generation
}
/// Cache lookup touches the decoded LRU. None includes pending and failed images;
/// `status` distinguishes a negative-cache hit from missing work.
pub fn get(key: &ArtworkKey, cx: &mut App) -> Option<Arc<RenderImage>> {
    init(cx);
    cx.update_global::<ArtworkStore, _>(|store, _| store.lookup(key))
}
/// Palette for the exact cached decoded image. No sampling, I/O or LRU mutation.
/// Missing, failed and evicted artwork has no palette.
pub fn ambient(key: &ArtworkKey, cx: &App) -> Option<ArtworkPalette> {
    cx.try_global::<ArtworkStore>()?.ambient(key)
}
pub fn status(key: &ArtworkKey, cx: &App) -> Option<Result<(), ArtworkError>> {
    cx.try_global::<ArtworkStore>()?
        .cache
        .iter()
        .find(|e| &e.key == key && !expired(&e.result))
        .map(|e| e.result.as_ref().map(|_| ()).map_err(|e| *e))
}
/// Disk hits stay a quiet placeholder for 150 ms instead of flashing a shimmer.
pub fn skeleton_due(key: &ArtworkKey, cx: &App) -> bool {
    cx.try_global::<ArtworkStore>().is_some_and(|store| {
        store
            .queue
            .iter()
            .chain(store.active.values())
            .find(|job| &job.key == key)
            .is_some_and(|job| {
                job.queued_at.elapsed() >= crate::ui::theme::motion::timing::SKELETON_DELAY
            })
    })
}
pub fn request(owner: &str, generation: u64, key: ArtworkKey, cx: &mut App) {
    init(cx);
    cx.update_global::<ArtworkStore, _>(|store, _| store.enqueue(owner, generation, key));
    pump(cx);
}
/// Replace this owner's queued prefetch window, preserving explicit requests.
/// Active work retains its slot. Explicit/visible requests outrank all prefetch;
/// within each class the most recently activated owner is served first.
pub fn request_window(owner: &str, generation: u64, keys: Vec<ArtworkKey>, cx: &mut App) {
    init(cx);
    cx.update_global::<ArtworkStore, _>(|store, _| {
        store.replace_window(owner, generation, keys);
    });
    pump(cx);
}
fn pump(cx: &mut App) {
    loop {
        let job = cx.update_global::<ArtworkStore, _>(|store, _| store.start_next());
        let Some(job) = job else {
            break;
        };
        let id = job.id;
        let timer = cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(crate::ui::theme::motion::timing::SKELETON_DELAY)
                .await;
            cx.update(|cx| cx.refresh_windows());
        });
        cx.update_global::<ArtworkStore, _>(|store, _| {
            store.timers.insert(id, timer);
        });
        // Clone only an Arc on the UI thread, never image bytes. A negative source
        // cache hit also propagates without a second download.
        let source = cx.update_global::<ArtworkStore, _>(|store, _| {
            job.key.blurred.then(|| store.source(&job.key)).flatten()
        });
        let receiver = cx.update_global::<ArtworkStore, _>(|store, _| {
            let workers = store.workers.get_or_insert_with(ArtworkWorkers::new);
            let clear_miss = store.retry.remove(&job.key.url);
            workers.submit(
                job.key.clone(),
                source,
                clear_miss,
                store.title_scopes.get(&job.owner).cloned(),
            )
        });
        let task = cx.spawn(async move |cx| {
            let result = receiver.await.unwrap_or(Err(ArtworkError::Transport));
            cx.update(|cx| {
                cx.update_global::<ArtworkStore, _>(|store, cx| {
                    if store.complete(&job, result) {
                        for image in store.trim_images() {
                            cx.drop_image(image, None);
                        }
                    }
                });
                cx.refresh_windows();
                pump(cx);
            });
        });
        cx.update_global::<ArtworkStore, _>(|store, _| {
            store.tasks.insert(id, task);
        });
    }
}
// Only ACTIVE jobs can be submitted by pump. Dropping the store closes the
// channel: idle threads exit, in-flight blocking calls finish under HTTP timeouts.
// Never join these threads on the UI thread. No blocking work uses GPUI's executor.
type ArtworkResult = Result<DecodedArtwork, ArtworkError>;
struct ArtworkWork {
    key: ArtworkKey,
    source: Option<ArtworkResult>,
    clear_miss: bool,
    title_scope: Option<String>,
    reply: tokio::sync::oneshot::Sender<ArtworkResult>,
}
enum WorkerCommand {
    Image(ArtworkWork),
    Clear(
        Vec<String>,
        Option<String>,
        tokio::sync::oneshot::Sender<()>,
    ),
}
struct ArtworkWorkers {
    sender: mpsc::SyncSender<WorkerCommand>,
    threads: Vec<JoinHandle<()>>,
}
impl ArtworkWorkers {
    fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WorkerCommand>(ACTIVE + 1);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut threads = Vec::new();
        for index in 0..ACTIVE {
            let receiver = receiver.clone();
            let spawned = thread::Builder::new()
                .name(format!("kinema-art-{index}"))
                .spawn(move || {
                    loop {
                        let work = {
                            let Ok(receiver) = receiver.lock() else {
                                tracing::error!(
                                    category = "poisoned_lock",
                                    "Artwork worker receiver failed"
                                );
                                break;
                            };
                            receiver.recv()
                        };
                        let Ok(command) = work else { break };
                        let work = match command {
                            WorkerCommand::Image(work) => work,
                            WorkerCommand::Clear(urls, scope, reply) => {
                                if let Some(disk) = disk_cache() {
                                    if let Some(scope) = scope {
                                        disk.clear_scope(&scope);
                                    }
                                    for url in urls {
                                        disk.clear_miss(&url);
                                    }
                                }
                                let _ = reply.send(());
                                continue;
                            }
                        };
                        if work.source.is_some() {
                            tracing::debug!(cache = "memory", "Artwork source reused");
                        }
                        let result = work
                            .source
                            .unwrap_or_else(|| {
                                fetch(
                                    &work.key.url,
                                    work.key.kind,
                                    work.clear_miss,
                                    work.title_scope.as_deref(),
                                )
                            })
                            .and_then(|decoded| {
                                if work.key.blurred {
                                    derive_backdrop(&decoded)
                                } else {
                                    Ok(decoded)
                                }
                            });
                        if let Err(category) = &result {
                            tracing::warn!(?category, "Artwork work failed");
                        }
                        if work.reply.send(result).is_err() {
                            tracing::debug!("Artwork result receiver closed");
                        }
                    }
                });
            match spawned {
                Ok(handle) => threads.push(handle),
                Err(error) => {
                    tracing::warn!(io_kind = ?error.kind(), "Artwork worker could not start")
                }
            }
        }
        Self { sender, threads }
    }
    fn submit(
        &self,
        key: ArtworkKey,
        source: Option<ArtworkResult>,
        clear_miss: bool,
        title_scope: Option<String>,
    ) -> tokio::sync::oneshot::Receiver<ArtworkResult> {
        let (reply, receiver) = tokio::sync::oneshot::channel();
        // A failed spawn or disconnected/full queue becomes a retryable failure,
        // never a blocking send on the UI thread.
        if !self.threads.is_empty()
            && self
                .sender
                .try_send(WorkerCommand::Image(ArtworkWork {
                    key,
                    source,
                    clear_miss,
                    title_scope,
                    reply,
                }))
                .is_err()
        {
            tracing::warn!(category = "queue_unavailable", "Artwork work not admitted");
        }
        receiver
    }
}
impl ArtworkStore {
    fn source(&mut self, key: &ArtworkKey) -> Option<Result<DecodedArtwork, ArtworkError>> {
        let source = key.source();
        self.cache.retain(|entry| !expired(&entry.result));
        let index = self.cache.iter().position(|entry| entry.key == source)?;
        let entry = self.cache.remove(index)?;
        let result = entry.result.clone();
        self.cache.push_back(entry);
        Some(result)
    }
    fn ambient(&self, key: &ArtworkKey) -> Option<ArtworkPalette> {
        self.cache
            .iter()
            .find(|entry| &entry.key == key)?
            .result
            .as_ref()
            .ok()
            .map(|decoded| decoded.palette)
    }
    fn advance_generation(&mut self, owner: &str) -> u64 {
        if owner.len() > 128
            || (!self.generations.contains_key(owner) && self.generations.len() >= 64)
        {
            return 0;
        }
        let generation = self.generations.entry(owner.into()).or_default();
        *generation = generation.wrapping_add(1);
        self.queue.retain(|job| job.owner != owner);
        self.requested.remove(owner);
        self.title_scopes.remove(owner);
        self.cache.retain(|entry| entry.result.is_ok());
        self.newest_owner = Some(owner.into());
        *generation
    }

    fn enqueue(&mut self, owner: &str, generation: u64, key: ArtworkKey) {
        self.enqueue_with_priority(owner, generation, key, true);
    }
    fn enqueue_with_priority(
        &mut self,
        owner: &str,
        generation: u64,
        key: ArtworkKey,
        interactive: bool,
    ) {
        self.cache.retain(|entry| !expired(&entry.result));
        if self.generations.get(owner).copied() != Some(generation)
            || self.cache.iter().any(|e| e.key == key)
            || self
                .active
                .values()
                .any(|j| j.key == key && j.owner == owner && j.generation == generation)
        {
            return;
        }
        if let Some(job) = self
            .queue
            .iter_mut()
            .find(|j| j.key == key && j.owner == owner && j.generation == generation)
        {
            job.interactive |= interactive;
            return;
        }
        if self.queue.len() >= QUEUED {
            let victim = self.queue.iter().position(|job| {
                !job.interactive
                    && (interactive
                        || self.newest_owner.as_deref() == Some(owner) && job.owner != owner)
            });
            if let Some(index) = victim {
                self.queue.remove(index);
            } else if interactive {
                // Even an all-interactive backlog must admit new visible work.
                self.queue.pop_front();
            } else {
                return;
            }
        }
        if let Some(urls) = self.requested.get_mut(owner)
            && !urls.contains(&key.url)
        {
            urls.push(key.url.clone());
        }
        self.next = self.next.wrapping_add(1);
        self.queue.push_back(Job {
            id: self.next,
            owner: owner.into(),
            generation,
            key,
            interactive,
            queued_at: Instant::now(),
        });
    }
    fn replace_window(&mut self, owner: &str, generation: u64, keys: Vec<ArtworkKey>) {
        if self.generations.get(owner).copied() != Some(generation) {
            return;
        }
        self.queue
            .retain(|job| job.owner != owner || job.interactive);
        for key in keys.into_iter().take(QUEUED) {
            self.enqueue_with_priority(owner, generation, key, false);
        }
    }
    fn start_next(&mut self) -> Option<Job> {
        if self.active.len() >= ACTIVE {
            return None;
        }
        self.queue.retain(|job| {
            self.generations.get(&job.owner).copied() == Some(job.generation)
                && !self.cache.iter().any(|e| e.key == job.key)
        });
        // Keep two physical workers available for newly visible work even when
        // all six prefetch calls are blocked on transport. In-flight work is not
        // preempted; its slot remains accounted for across generation changes.
        let prefetch_full = self.active.values().filter(|job| !job.interactive).count()
            >= ACTIVE - INTERACTIVE_RESERVED;
        let index = self
            .queue
            .iter()
            .enumerate()
            .filter(|(_, job)| {
                (job.interactive || !prefetch_full)
                    && !self
                        .active
                        .values()
                        .any(|active| active.key.source() == job.key.source())
            })
            .max_by_key(|(index, job)| {
                (
                    job.interactive,
                    self.newest_owner.as_deref() == Some(job.owner.as_str()),
                    job.key.blurred || job.key.kind == ArtworkKind::Backdrop,
                    std::cmp::Reverse(*index),
                )
            })
            .map(|(index, _)| index)?;
        let job = self.queue.remove(index)?;
        self.active.insert(job.id, job.clone());
        Some(job)
    }

    fn complete(&mut self, job: &Job, result: Result<DecodedArtwork, ArtworkError>) -> bool {
        self.active.remove(&job.id);
        self.tasks.remove(&job.id);
        self.timers.remove(&job.id);
        if self.generations.get(&job.owner).copied() != Some(job.generation) {
            return false;
        }
        let bytes = result
            .as_ref()
            .map_or(0, |image| image.image.as_bytes(0).map_or(0, <[u8]>::len));
        self.cache.push_back(Cached {
            key: job.key.clone(),
            result,
            bytes,
        });
        true
    }
    fn lookup(&mut self, key: &ArtworkKey) -> Option<Arc<RenderImage>> {
        let index = self.cache.iter().position(|entry| &entry.key == key)?;
        let entry = self.cache.remove(index)?;
        let result = entry
            .result
            .as_ref()
            .ok()
            .map(|decoded| decoded.image.clone());
        self.cache.push_back(entry);
        result
    }
    fn trim_images(&mut self) -> Vec<Arc<RenderImage>> {
        let mut evicted = Vec::new();
        for (kind, blurred) in [
            (ArtworkKind::Poster, false),
            (ArtworkKind::Backdrop, false),
            (ArtworkKind::Still, false),
            (ArtworkKind::Poster, true),
        ] {
            let matches =
                |key: &ArtworkKey| key.blurred == blurred && (blurred || key.kind == kind);
            let quota = if blurred {
                BACKDROP_BYTES
            } else {
                kind.quota()
            };
            while self
                .cache
                .iter()
                .filter(|e| matches(&e.key))
                .map(|e| e.bytes)
                .sum::<usize>()
                > quota
            {
                if let Some(index) = self.cache.iter().position(|e| matches(&e.key))
                    && let Some(Cached {
                        result: Ok(image), ..
                    }) = self.cache.remove(index)
                {
                    evicted.push(image.image);
                }
            }
        }
        // Browse posters cannot consume the episode-still entry allowance.
        for (still, limit) in [(false, ENTRIES), (true, 128)] {
            while self
                .cache
                .iter()
                .filter(|entry| {
                    (entry.key.kind == ArtworkKind::Still && !entry.key.blurred) == still
                })
                .count()
                > limit
            {
                let Some(index) = self.cache.iter().position(|entry| {
                    (entry.key.kind == ArtworkKind::Still && !entry.key.blurred) == still
                }) else {
                    break;
                };
                if let Some(Cached {
                    result: Ok(image), ..
                }) = self.cache.remove(index)
                {
                    evicted.push(image.image);
                }
            }
        }
        evicted
    }
}

fn client_builder() -> reqwest::blocking::ClientBuilder {
    // Some current Metahub posters redirect to live.metahub.space. Every hop
    // must pass key validation and stay HTTPS; never forward a Referer, proxy,
    // cookie or provider credential. HTTP/1 pooling needs neither http2 nor gzip.
    Client::builder()
        .no_proxy()
        .referer(false)
        .redirect(Policy::custom(|attempt| {
            if valid_redirect(attempt.url(), attempt.previous()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(ACTIVE)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
}
fn valid_redirect(url: &Url, previous: &[Url]) -> bool {
    previous.len() <= 3
        && previous.iter().all(|url| url.scheme() == "https")
        && url.scheme() == "https"
        && ArtworkKey::new(ArtworkKind::Poster, url.as_str()).is_some()
}
fn build_client() -> Result<Client, ArtworkError> {
    client_builder().build().map_err(|error| {
        tracing::warn!(
            kind = crate::logging::http_error_kind(&error),
            operation = "build_client",
            "Artwork client initialization failed"
        );
        ArtworkError::Transport
    })
}
fn shared_client() -> Result<&'static Client, ArtworkError> {
    static CLIENT: OnceLock<Result<Client, ArtworkError>> = OnceLock::new();
    CLIENT.get_or_init(build_client).as_ref().map_err(|e| *e)
}
fn download(client: &Client, url: &str) -> Result<Vec<u8>, ArtworkError> {
    let started = Instant::now();
    let response = client.get(url).send().map_err(|error| {
        tracing::warn!(
            kind = crate::logging::http_error_kind(&error),
            duration_ms = started.elapsed().as_millis() as u64,
            "Artwork transport failed"
        );
        ArtworkError::Transport
    })?;
    if !response.status().is_success() {
        tracing::warn!(
            status = response.status().as_u16(),
            duration_ms = started.elapsed().as_millis() as u64,
            "Artwork HTTP request failed"
        );
        return Err(if matches!(response.status().as_u16(), 404 | 410) {
            ArtworkError::Unavailable
        } else {
            ArtworkError::Transport
        });
    }
    if response.content_length().is_some_and(|n| n > ENCODED) {
        tracing::warn!(category = "encoded_size", "Artwork exceeds size limit");
        return Err(ArtworkError::TooLarge);
    }
    let mut bytes = Vec::new();
    response
        .take(ENCODED + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| { tracing::warn!(io_kind = ?error.kind(), duration_ms = started.elapsed().as_millis() as u64, "Artwork body read failed"); ArtworkError::Transport })?;
    if bytes.len() as u64 > ENCODED {
        tracing::warn!(category = "encoded_size", "Artwork exceeds size limit");
        return Err(ArtworkError::TooLarge);
    }
    tracing::debug!(
        bytes = bytes.len(),
        duration_ms = started.elapsed().as_millis() as u64,
        "Artwork downloaded"
    );
    Ok(bytes)
}
// Serialize this process's cache maintenance, not network/decode work. All callers
// run on background workers. Cache failures are best-effort misses, never UI errors.
static DISK_LOCK: Mutex<()> = Mutex::new(());
fn cache_result<T>(result: std::io::Result<T>, operation: &'static str) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!(operation, io_kind = ?error.kind(), "Artwork disk cache operation failed");
            }
            None
        }
    }
}
struct DiskCache {
    root: PathBuf,
    budget: u64,
    maintenance: Mutex<TrimState>,
    miss_leases: Mutex<HashMap<PathBuf, Weak<AtomicBool>>>,
}
#[derive(Default)]
struct TrimState {
    bytes: Option<u64>,
    writes: usize,
    last_trim: Option<Instant>,
}
impl TrimState {
    fn record_write(&mut self, bytes: u64, budget: u64, now: Instant) -> bool {
        self.writes += 1;
        self.bytes = self.bytes.map(|total| total.saturating_add(bytes));
        self.bytes.is_none_or(|total| total > budget)
            || self.writes >= 64
            || self
                .last_trim
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(60))
    }
    fn trimmed(&mut self, bytes: u64, now: Instant) {
        self.bytes = Some(bytes);
        self.writes = 0;
        self.last_trim = Some(now);
    }
}
impl DiskCache {
    fn new(root: PathBuf, budget: u64) -> Self {
        Self {
            root,
            budget,
            maintenance: Mutex::new(TrimState::default()),
            miss_leases: Mutex::new(HashMap::new()),
        }
    }
    fn path(&self, url: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        self.root
            .join(format!("{:x}.img", Sha256::digest(url.as_bytes())))
    }
    fn scope_hash(scope: &str) -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(scope.as_bytes()))
    }
    fn associate_scope(&self, url: &str, scope: &str) {
        let Ok(_guard) = DISK_LOCK.lock() else { return };
        if !fs::symlink_metadata(&self.root).is_ok_and(|meta| meta.is_dir()) {
            return;
        }
        let path = self.path(url).with_extension("miss");
        if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() < 4096) {
            return;
        }
        let Ok(mut contents) = fs::read_to_string(&path) else {
            return;
        };
        let hash = Self::scope_hash(scope);
        if contents.lines().skip(1).any(|line| line == hash) {
            return;
        }
        contents.push('\n');
        contents.push_str(&hash);
        cache_result(
            (|| -> std::io::Result<()> {
                let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
                file.write_all(contents.as_bytes())?;
                file.persist(path).map_err(|e| e.error)?;
                Ok(())
            })(),
            "associate_title_miss",
        );
    }
    fn clear_scope(&self, scope: &str) {
        let Ok(_guard) = DISK_LOCK.lock() else { return };
        if !fs::symlink_metadata(&self.root).is_ok_and(|meta| meta.is_dir()) {
            return;
        }
        let hash = Self::scope_hash(scope);
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !owned_cache_file(&path)
                || path.extension().is_none_or(|ext| ext != "miss")
                || !entry.file_type().is_ok_and(|kind| kind.is_file())
                || !entry.metadata().is_ok_and(|meta| meta.len() <= 4096)
            {
                continue;
            }
            if fs::read_to_string(&path)
                .is_ok_and(|text| text.lines().skip(1).any(|line| line == hash))
            {
                cache_result(fs::remove_file(path), "clear_title_miss");
            }
        }
    }
    // A refresh invalidates in-flight miss writes for this URL, not other titles.
    fn miss_lease(&self, url: &str) -> Arc<AtomicBool> {
        let Ok(mut leases) = self.miss_leases.lock() else {
            return Arc::new(AtomicBool::new(true));
        };
        leases.retain(|_, lease| lease.strong_count() > 0);
        let lease = leases.entry(self.path(url)).or_default();
        if let Some(existing) = lease.upgrade() {
            return existing;
        }
        let current = Arc::new(AtomicBool::new(false));
        *lease = Arc::downgrade(&current);
        current
    }
    fn clear_miss(&self, url: &str) {
        let Ok(_guard) = DISK_LOCK.lock() else { return };
        if let Ok(mut leases) = self.miss_leases.lock()
            && let Some(lease) = leases
                .remove(&self.path(url))
                .and_then(|lease| lease.upgrade())
        {
            lease.store(true, Ordering::Release);
        }
        if !fs::symlink_metadata(&self.root).is_ok_and(|meta| meta.is_dir()) {
            return;
        }
        cache_result(
            fs::remove_file(self.path(url).with_extension("miss")),
            "clear_miss",
        );
    }
    #[cfg(test)]
    fn missed(&self, url: &str, now: SystemTime) -> bool {
        self.miss_expiry(url, now).is_some()
    }
    fn miss_expiry(&self, url: &str, now: SystemTime) -> Option<SystemTime> {
        let Ok(_guard) = DISK_LOCK.lock() else {
            return None;
        };
        if !fs::symlink_metadata(&self.root).ok()?.is_dir() {
            return None;
        }
        let path = self.path(url).with_extension("miss");
        if !fs::symlink_metadata(&path).ok()?.is_file() {
            return None;
        }
        let expiry = (|| {
            let mut text = String::new();
            fs::File::open(&path)
                .ok()?
                .take(32)
                .read_to_string(&mut text)
                .ok()?;
            let stamp = text.lines().next()?.parse::<u64>().ok()?;
            let created = SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(stamp))?;
            let age = now.duration_since(created).ok()?;
            (age < Duration::from_secs(86_400)).then_some(())?;
            created.checked_add(Duration::from_secs(86_400))
        })();
        if expiry.is_none() {
            cache_result(fs::remove_file(path), "expire_miss");
        }
        expiry
    }

    fn write_miss(&self, url: &str, now: SystemTime, lease: Option<&AtomicBool>) {
        let Ok(_guard) = DISK_LOCK.lock() else { return };
        if lease.is_some_and(|lease| lease.load(Ordering::Acquire)) {
            return;
        }
        let Some(stamp) = now.duration_since(SystemTime::UNIX_EPOCH).ok() else {
            return;
        };
        cache_result(
            (|| -> std::io::Result<()> {
                fs::create_dir_all(&self.root)?;
                if !fs::symlink_metadata(&self.root)?.is_dir() {
                    return Ok(());
                }
                let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
                write!(file, "{}", stamp.as_secs())?;
                file.persist(self.path(url).with_extension("miss"))
                    .map_err(|e| e.error)?;
                if let Ok(mut state) = self.maintenance.lock() {
                    let now = Instant::now();
                    if state.record_write(20, self.budget, now) {
                        state.trimmed(self.trim()?, now);
                    }
                }
                Ok(())
            })(),
            "write_miss",
        );
    }
    fn read(&self, url: &str, kind: ArtworkKind) -> Option<DecodedArtwork> {
        let _guard = DISK_LOCK
            .lock()
            .map_err(|_| {
                tracing::warn!(category = "poisoned_lock", "Artwork disk cache unavailable")
            })
            .ok()?;
        if !cache_result(fs::symlink_metadata(&self.root), "root_metadata")?.is_dir() {
            return None;
        }
        let path = self.path(url);
        let result = (|| {
            if !cache_result(fs::symlink_metadata(&path), "entry_metadata")?.is_file() {
                return None;
            }
            let file = cache_result(fs::File::open(&path), "open")?;
            if !cache_result(file.metadata(), "file_metadata")?.is_file() {
                return None;
            }
            let mut bytes = Vec::new();
            cache_result(file.take(ENCODED + 1).read_to_end(&mut bytes), "read")?;
            Some(bytes)
        })();
        drop(_guard);
        let result = result.and_then(|bytes| decode(&bytes, kind).ok());
        let _guard = DISK_LOCK
            .lock()
            .map_err(|_| {
                tracing::warn!(category = "poisoned_lock", "Artwork disk cache unavailable")
            })
            .ok()?;
        if result.is_some() {
            // Best-effort last-access approximation; no automatic TTL.
            if let Some(file) =
                cache_result(fs::OpenOptions::new().write(true).open(&path), "touch_open")
            {
                cache_result(file.set_modified(SystemTime::now()), "touch");
            }
        } else {
            cache_result(fs::remove_file(path), "remove_invalid");
        }
        result
    }
    fn write(&self, url: &str, bytes: &[u8]) {
        let Ok(_guard) = DISK_LOCK.lock() else {
            return;
        };
        if bytes.len() as u64 > ENCODED || bytes.len() as u64 > self.budget {
            return;
        }
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(&self.root)?;
            if !fs::symlink_metadata(&self.root)?.is_dir() {
                tracing::warn!(
                    category = "cache_root_type",
                    "Artwork cache root is not a directory"
                );
                return Ok(());
            }
            let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
            file.write_all(bytes)?;
            file.persist(self.path(url)).map_err(|e| e.error)?;
            if let Ok(mut state) = self.maintenance.lock() {
                let now = Instant::now();
                // Conservative accounting (overwrites count twice) never hides
                // budget growth. Scan once initially, then only on budget/count/
                // time thresholds. Failures retain the need to retry maintenance.
                if state.record_write(bytes.len() as u64, self.budget, now) {
                    state.trimmed(self.trim()?, now);
                }
            }
            Ok(())
        })();
        cache_result(result, "write_or_trim");
    }
    fn trim(&self) -> std::io::Result<u64> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let Some(entry) = cache_result(entry, "scan_entry") else {
                continue;
            };
            let path = entry.path();
            if !owned_cache_file(&path) || !entry.file_type()?.is_file() {
                continue;
            }
            let metadata = entry.metadata()?;
            files.push((
                cache_result(metadata.modified(), "modified_time")
                    .unwrap_or(SystemTime::UNIX_EPOCH),
                path,
                metadata.len(),
            ));
        }
        files.sort_by_key(|entry| entry.0);
        let mut total: u64 = files.iter().map(|entry| entry.2).sum();
        let mut misses = files
            .iter()
            .filter(|(_, path, _)| path.extension().is_some_and(|ext| ext == "miss"))
            .count();
        for (_, path, size) in files {
            if total <= self.budget && misses <= 4096 {
                break;
            }
            let is_miss = path.extension().is_some_and(|ext| ext == "miss");
            if total <= self.budget && !is_miss {
                continue;
            }
            if cache_result(fs::remove_file(path), "evict").is_some() {
                total -= size;
                misses -= usize::from(is_miss);
            }
        }
        Ok(total)
    }
}
fn owned_cache_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            ((name.len() == 68 && name.ends_with(".img"))
                || (name.len() == 69 && name.ends_with(".miss")))
                && name.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit)
        })
}
fn disk_cache() -> Option<&'static DiskCache> {
    static DISK: OnceLock<Option<DiskCache>> = OnceLock::new();
    DISK.get_or_init(|| {
        if cfg!(test) {
            None
        } else {
            crate::paths::AppPaths::resolve()
                .map_err(|_| tracing::warn!("Artwork cache paths unavailable"))
                .ok()
                .map(|paths| DiskCache::new(paths.cache.join("artwork"), DISK_BYTES))
        }
    })
    .as_ref()
}
fn fetch(
    url: &str,
    kind: ArtworkKind,
    clear_miss: bool,
    title_scope: Option<&str>,
) -> Result<DecodedArtwork, ArtworkError> {
    let disk = disk_cache();
    let result = fetch_cached(url, kind, clear_miss, disk);
    if let Err(error) = &result
        && permanent(*error)
        && let (Some(disk), Some(scope)) = (disk, title_scope)
    {
        disk.associate_scope(url, scope);
    }
    result
}
fn fetch_cached(
    url: &str,
    kind: ArtworkKind,
    clear_miss: bool,
    disk: Option<&DiskCache>,
) -> Result<DecodedArtwork, ArtworkError> {
    let start = Instant::now();
    if clear_miss && let Some(disk) = disk {
        disk.clear_miss(url);
    }
    let miss_lease = disk.map(|cache| cache.miss_lease(url));
    if let Some(disk) = disk
        && let Some(expiry) = disk.miss_expiry(url, SystemTime::now())
    {
        return Err(ArtworkError::CachedMiss(expiry));
    }
    if let Some(image) = disk.and_then(|cache| cache.read(url, kind)) {
        tracing::debug!(
            cache = "disk",
            decode_ms = start.elapsed().as_millis(),
            "artwork ready"
        );
        return Ok(image);
    }
    let failure = |error| {
        if permanent(error)
            && let Some(disk) = disk
        {
            let now = SystemTime::now();
            disk.write_miss(url, now, miss_lease.as_deref());
            ArtworkError::CachedMiss(now + Duration::from_secs(86_400))
        } else {
            error
        }
    };
    let bytes = download(shared_client()?, url).map_err(failure)?;
    let fetched = start.elapsed();
    let decode_start = Instant::now();
    let image = decode(&bytes, kind).map_err(failure)?;
    tracing::debug!(
        bytes = bytes.len(),
        cache = "network",
        fetch_ms = fetched.as_millis(),
        decode_ms = decode_start.elapsed().as_millis(),
        "artwork ready"
    );
    // Only successful, validated static image responses are persisted.
    if let Some(disk) = disk {
        disk.write(url, &bytes);
    }
    Ok(image)
}
fn decode(bytes: &[u8], kind: ArtworkKind) -> Result<DecodedArtwork, ArtworkError> {
    let started = Instant::now();
    let result = (|| {
        if bytes.len() as u64 > ENCODED {
            tracing::warn!(category = "encoded_size", "Artwork exceeds size limit");
            return Err(ArtworkError::TooLarge);
        }
        let format = image::guess_format(bytes).map_err(|_| ArtworkError::InvalidImage)?;
        // Static JPEG, PNG and WebP only. Reject animated PNG/WebP before decoder
        // construction; GIF and other multi-frame formats are never enabled.
        if !matches!(
            format,
            ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
        ) || animated(bytes, format)
        {
            return Err(ArtworkError::InvalidImage);
        }
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(EDGE);
        limits.max_image_height = Some(EDGE);
        limits.max_alloc = Some(PIXELS * 8);
        reader.limits(limits.clone());
        let (width, height) = reader
            .into_dimensions()
            .map_err(|_| ArtworkError::InvalidImage)?;
        if width == 0
            || height == 0
            || width > EDGE
            || height > EDGE
            || u64::from(width) * u64::from(height) > PIXELS
        {
            return Err(ArtworkError::TooLarge);
        }
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|_| ArtworkError::InvalidImage)?;
        let (max_width, max_height) = kind.thumbnail_bounds();
        // Source limits above apply before full decoding. Only the bounded display
        // bucket enters the LRU/GPU; never upscale small source artwork.
        let thumbnail = if width > max_width || height > max_height {
            decoded.thumbnail(max_width, max_height)
        } else {
            decoded
        };
        let mut rgba = thumbnail.into_rgba8();
        let palette = extract_palette(&rgba);
        // GPUI's renderer expects BGRA (same convention as its built-in image loader).
        for pixel in rgba.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        Ok(DecodedArtwork {
            image: Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])),
            palette,
        })
    })();
    if let Err(category) = &result {
        tracing::warn!(
            ?category,
            decode_ms = started.elapsed().as_millis() as u64,
            "Artwork decode failed"
        );
    }
    result
}
// Input/output are BGRA, matching RenderImage. Blur and the symmetric RGB
// grade commute with that channel order; no extra full-image channel swap needed.
fn derive_backdrop(source: &DecodedArtwork) -> Result<DecodedArtwork, ArtworkError> {
    let size = source.image.size(0);
    let pixels = source.image.as_bytes(0).ok_or(ArtworkError::InvalidImage)?;
    let image = image::RgbaImage::from_raw(size.width.into(), size.height.into(), pixels.to_vec())
        .ok_or(ArtworkError::InvalidImage)?;
    let blurred = backdrop_pixels(image)?;
    Ok(DecodedArtwork {
        image: Arc::new(RenderImage::new(vec![image::Frame::new(blurred)])),
        palette: source.palette,
    })
}

fn backdrop_pixels(image: image::RgbaImage) -> Result<image::RgbaImage, ArtworkError> {
    if image.width() == 0 || image.height() == 0 {
        return Err(ArtworkError::InvalidImage);
    }
    let small = if image.width() > 480 || image.height() > 480 {
        image::DynamicImage::ImageRgba8(image)
            .thumbnail(480, 480)
            .into_rgba8()
    } else {
        image
    };
    let mut blurred = image::imageops::fast_blur(&small, 24.0);
    for pixel in blurred.pixels_mut() {
        let [b, g, r, a] = pixel.0;
        let gray = (u32::from(b) + u32::from(g) + u32::from(r)) / 3;
        // Retain 80% chroma, darken to 55%, and composite transparency onto
        // black so transparent artwork cannot expose bright ungraded RGB.
        for channel in &mut pixel.0[..3] {
            *channel =
                (((u32::from(*channel) * 4 + gray) * 55 * u32::from(a)) / (5 * 100 * 255)) as u8;
        }
        pixel.0[3] = 255;
    }
    Ok(blurred)
}

fn animated(bytes: &[u8], format: ImageFormat) -> bool {
    let (mut offset, png) = match format {
        ImageFormat::Png => (8usize, true),
        ImageFormat::WebP => (12usize, false),
        _ => return false,
    };
    while offset.checked_add(8).is_some_and(|n| n <= bytes.len()) {
        let mut length = [0; 4];
        length.copy_from_slice(&bytes[offset..offset + 4]);
        let (size, tag, overhead) = if png {
            (
                u32::from_be_bytes(length) as usize,
                &bytes[offset + 4..offset + 8],
                12,
            )
        } else {
            (
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap_or([0; 4]))
                    as usize,
                &bytes[offset..offset + 4],
                8,
            )
        };
        if (png && tag == b"acTL")
            || (!png
                && (tag == b"ANIM"
                    || tag == b"ANMF"
                    || (tag == b"VP8X"
                        && bytes.get(offset + 8).is_some_and(|flags| flags & 2 != 0))))
        {
            return true;
        }
        let padded = if png {
            size
        } else {
            size.saturating_add(size % 2)
        };
        let Some(next) = offset
            .checked_add(overhead)
            .and_then(|n| n.checked_add(padded))
        else {
            return true;
        };
        if next > bytes.len() {
            return true;
        }
        offset = next;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;

    fn test_key(index: usize) -> ArtworkKey {
        ArtworkKey::new(
            ArtworkKind::Poster,
            &format!("https://example.com/{index}.png"),
        )
        .unwrap_or_else(|| panic!("fixture URL"))
    }

    #[test]
    fn newest_owner_interactive_hero_precedes_full_background_prefetch() {
        let mut store = ArtworkStore::default();
        let browse = store.advance_generation("browse");
        store.replace_window("browse", browse, (0..QUEUED).map(test_key).collect());
        assert_eq!(store.queue.len(), QUEUED);
        let detail = store.advance_generation("detail");
        store.enqueue("detail", detail, test_key(1000));
        let hero = test_key(1001).blurred();
        store.enqueue("detail", detail, hero.clone());
        assert_eq!(store.queue.len(), QUEUED);
        // Background viewport churn must neither evict nor outrank the hero.
        store.replace_window(
            "browse",
            browse,
            (2000..2000 + QUEUED).map(test_key).collect(),
        );
        assert_eq!(store.start_next().map(|job| job.key), Some(hero));
        assert_eq!(store.start_next().map(|job| job.key), Some(test_key(1000)));
    }

    #[test]
    fn newest_owner_wins_within_each_priority_and_prefetch_promotes_in_place() {
        let mut store = ArtworkStore::default();
        let browse = store.advance_generation("browse");
        store.enqueue("browse", browse, test_key(0));
        let detail = store.advance_generation("detail");
        store.replace_window("detail", detail, vec![test_key(1), test_key(2)]);
        store.enqueue("detail", detail, test_key(1));
        assert_eq!(store.queue.len(), 3);
        assert_eq!(store.start_next().map(|job| job.key), Some(test_key(1)));
        assert_eq!(store.start_next().map(|job| job.key), Some(test_key(0)));
        store.replace_window("browse", browse, vec![test_key(3)]);
        assert_eq!(store.start_next().map(|job| job.key), Some(test_key(2)));
    }

    #[test]
    fn full_interactive_queue_still_admits_new_visible_request() {
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("detail");
        for index in 0..QUEUED {
            store.enqueue("detail", generation, test_key(index));
        }
        store.enqueue("detail", generation, test_key(QUEUED));
        assert_eq!(store.queue.len(), QUEUED);
        assert!(!store.queue.iter().any(|job| job.key == test_key(0)));
        assert!(store.queue.iter().any(|job| job.key == test_key(QUEUED)));
    }

    #[test]
    fn prefetch_reserves_two_slots_even_across_obsolete_generations() {
        let mut store = ArtworkStore::default();
        let browse = store.advance_generation("browse");
        store.replace_window("browse", browse, (0..QUEUED).map(test_key).collect());
        for _ in 0..ACTIVE - INTERACTIVE_RESERVED {
            assert!(store.start_next().is_some());
        }
        assert!(store.start_next().is_none());
        store.advance_generation("browse");
        let detail = store.advance_generation("detail");
        store.enqueue("detail", detail, test_key(1000).blurred());
        store.enqueue("detail", detail, test_key(1001));
        for _ in 0..INTERACTIVE_RESERVED {
            assert!(store.start_next().is_some_and(|job| job.interactive));
        }
        assert_eq!(store.active.len(), ACTIVE);
        store.enqueue("detail", detail, test_key(1002));
        assert!(store.start_next().is_none());
        let finished = store
            .active
            .values()
            .find(|job| !job.interactive)
            .cloned()
            .unwrap_or_else(|| panic!("active prefetch"));
        assert!(!store.complete(&finished, Err(ArtworkError::Transport)));
        assert_eq!(store.start_next().map(|job| job.key), Some(test_key(1002)));
    }

    #[test]
    fn disk_trim_is_throttled_but_budget_count_and_interval_trigger_scans() {
        let now = Instant::now();
        let mut state = TrimState::default();
        assert!(state.record_write(10, DISK_BYTES, now));
        state.trimmed(10, now);
        for _ in 0..63 {
            assert!(!state.record_write(10, DISK_BYTES, now));
        }
        assert!(state.record_write(10, DISK_BYTES, now));
        state.trimmed(DISK_BYTES - 10, now);
        assert!(!state.record_write(10, DISK_BYTES, now));
        assert!(state.record_write(1, DISK_BYTES, now));
        state.trimmed(0, now);
        assert!(state.record_write(1, DISK_BYTES, now + Duration::from_secs(60)));

        let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let cache = DiskCache::new(dir.path().join("artwork"), DISK_BYTES);
        cache.write("first", b"one");
        cache.write("second", b"two");
        let state = cache
            .maintenance
            .lock()
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(state.bytes, Some(6));
        assert_eq!(state.writes, 1, "second write did not scan");
    }

    #[test]
    fn shared_transport_client_is_reused() {
        let first = shared_client().unwrap_or_else(|error| panic!("{error:?}"));
        let second = shared_client().unwrap_or_else(|error| panic!("{error:?}"));
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn owned_workers_derive_and_return_failures_without_gpui_executor() {
        let workers = ArtworkWorkers::new();
        assert_eq!(workers.threads.len(), ACTIVE);
        assert!(workers.threads.iter().all(|worker| {
            worker
                .thread()
                .name()
                .is_some_and(|name| name.starts_with("kinema-art-"))
        }));
        let decoded = DecodedArtwork {
            image: Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4])),
            )])),
            palette: ArtworkPalette::NEUTRAL,
        };
        let result = workers
            .submit(test_key(0).blurred(), Some(Ok(decoded)), false, None)
            .blocking_recv()
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            result.image.as_bytes(0).map(|bytes| &bytes[..4]),
            Some([140, 140, 140, 255].as_slice())
        );
        assert!(matches!(
            workers
                .submit(test_key(1), Some(Err(ArtworkError::Transport)), false, None)
                .blocking_recv(),
            Ok(Err(ArtworkError::Transport))
        ));
    }

    #[test]
    fn backdrop_dimensions_blur_and_grade_are_bounded() {
        for (width, height, expected) in [
            (1024, 576, (480, 270)),
            (384, 576, (320, 480)),
            (3, 2, (3, 2)),
            (1, 8192, (1, 480)),
        ] {
            let input = image::RgbaImage::from_pixel(width, height, image::Rgba([255; 4]));
            let output = backdrop_pixels(input).unwrap_or_else(|e| panic!("backdrop: {e:?}"));
            assert_eq!(output.dimensions(), expected);
            assert!(output.pixels().all(|pixel| pixel.0 == [140, 140, 140, 255]));
        }
        let output = backdrop_pixels(image::RgbaImage::from_fn(120, 60, |x, _| {
            image::Rgba(if x < 60 {
                [0, 0, 255, 255]
            } else {
                [0, 0, 0, 255]
            })
        }))
        .unwrap_or_else(|e| panic!("backdrop: {e:?}"));
        let edge = output.get_pixel(60, 30).0;
        assert!(edge[2] > 0 && edge[2] < 120, "blur spreads the hard edge");
        assert!(edge[0] > 0 && edge[0] < edge[2], "slightly desaturated");
        assert!(
            output
                .pixels()
                .all(|pixel| pixel.0[..3].iter().all(|c| *c <= 140))
        );
        let transparent = backdrop_pixels(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([255, 0, 80, 0]),
        ))
        .unwrap_or_else(|e| panic!("backdrop: {e:?}"));
        assert_eq!(transparent.get_pixel(0, 0).0, [0, 0, 0, 255]);
    }

    #[test]
    fn backdrop_reuses_source_and_rejects_obsolete_generations() {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(12, 8, image::Rgba([240, 20, 10, 255]))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap_or_else(|e| panic!("fixture: {e}"));
        let decoded =
            decode(&bytes, ArtworkKind::Poster).unwrap_or_else(|e| panic!("decode: {e:?}"));
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("browse");
        let key = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/backdrop.png")
            .unwrap_or_else(|| panic!("key"));
        store.enqueue("browse", generation, key.clone());
        let source_job = store.start_next().unwrap_or_else(|| panic!("source job"));
        store.enqueue("browse", generation, key.blurred());
        assert!(store.start_next().is_none(), "wait for in-flight source");
        assert!(store.complete(&source_job, Ok(decoded.clone())));
        let source = store
            .source(&key.blurred())
            .unwrap_or_else(|| panic!("source"))
            .unwrap_or_else(|e| panic!("source: {e:?}"));
        assert!(Arc::ptr_eq(&source.image, &decoded.image));
        let derived = derive_backdrop(&source).unwrap_or_else(|e| panic!("derive: {e:?}"));
        assert_eq!(derived.palette, decoded.palette);
        // Renderer byte order stays BGRA.
        let pixels = derived
            .image
            .as_bytes(0)
            .unwrap_or_else(|| panic!("pixels"));
        assert!(pixels[2] > pixels[0]);
        let job = store.start_next().unwrap_or_else(|| panic!("derived job"));
        store.advance_generation("browse");
        assert!(!store.complete(&job, Ok(derived)));
        assert!(store.lookup(&key.blurred()).is_none());
        assert!(store.lookup(&key).is_some());
    }

    #[test]
    fn backdrop_failure_is_negative_cached_and_retryable() {
        assert!(matches!(
            backdrop_pixels(image::RgbaImage::new(0, 0)),
            Err(ArtworkError::InvalidImage)
        ));
        let empty = DecodedArtwork {
            image: Arc::new(RenderImage::new(Vec::<image::Frame>::new())),
            palette: ArtworkPalette::NEUTRAL,
        };
        assert!(matches!(
            derive_backdrop(&empty),
            Err(ArtworkError::InvalidImage)
        ));
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("detail");
        let key = ArtworkKey::new(ArtworkKind::Backdrop, "https://example.com/missing.png")
            .unwrap_or_else(|| panic!("key"));
        store.enqueue("detail", generation, key.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("source job"));
        assert!(store.complete(&job, Err(ArtworkError::Transport)));
        store.enqueue("detail", generation, key.blurred());
        let job = store.start_next().unwrap_or_else(|| panic!("derived job"));
        let failure = store
            .source(&key)
            .unwrap_or_else(|| panic!("negative cache"));
        assert!(matches!(failure, Err(ArtworkError::Transport)));
        assert!(store.complete(&job, failure));
        store.enqueue("detail", generation, key.blurred());
        assert!(
            store.start_next().is_none(),
            "failure must not spin/retry during render"
        );
        let generation = store.advance_generation("detail");
        store.enqueue("detail", generation, key.blurred());
        assert!(store.start_next().is_some());
    }

    #[test]
    fn backdrop_cache_has_independent_byte_and_shared_count_bounds() {
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("browse");
        let key = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/source.png")
            .unwrap_or_else(|| panic!("key"));
        let image = DecodedArtwork {
            image: Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::new(480, 480),
            )])),
            palette: ArtworkPalette::NEUTRAL,
        };
        store.enqueue("browse", generation, key.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("source job"));
        assert!(store.complete(&job, Ok(image.clone())));
        for index in 0..ENTRIES + 10 {
            let key = ArtworkKey::new(
                if index % 2 == 0 {
                    ArtworkKind::Poster
                } else {
                    ArtworkKind::Backdrop
                },
                &format!("https://example.com/{index}.png"),
            )
            .unwrap_or_else(|| panic!("key"))
            .blurred();
            store.enqueue("browse", generation, key);
            let job = store.start_next().unwrap_or_else(|| panic!("derived job"));
            assert!(store.complete(&job, Ok(image.clone())));
            store.trim_images();
            assert!(
                store
                    .cache
                    .iter()
                    .filter(|e| e.key.blurred)
                    .map(|e| e.bytes)
                    .sum::<usize>()
                    <= BACKDROP_BYTES
            );
            assert!(store.cache.len() <= ENTRIES);
        }
        assert!(
            store.lookup(&key).is_some(),
            "derived variants must not consume source quota"
        );
        for index in 0..ENTRIES + 10 {
            store.cache.push_back(Cached {
                key: ArtworkKey::new(
                    ArtworkKind::Poster,
                    &format!("https://example.com/fail{index}.png"),
                )
                .unwrap_or_else(|| panic!("key"))
                .blurred(),
                result: Err(ArtworkError::InvalidImage),
                bytes: 0,
            });
        }
        store.trim_images();
        assert_eq!(store.cache.len(), ENTRIES);
    }

    #[test]
    fn palette_samples_are_deterministic_muted_and_ignore_transparency_and_extremes() {
        let colorful = image::RgbaImage::from_fn(48, 48, |x, _| {
            image::Rgba(if x < 32 {
                [240, 30, 20, 255]
            } else {
                [20, 60, 220, 255]
            })
        });
        let palette = extract_palette(&colorful);
        assert_eq!(palette, extract_palette(&colorful));
        assert_ne!(palette, ArtworkPalette::NEUTRAL);
        assert!(palette.primary[0] > palette.primary[2]);
        assert!(palette.secondary[2] > palette.secondary[0]);
        for color in [palette.primary, palette.secondary] {
            let max = *color.iter().max().unwrap_or(&0) as f32;
            let min = *color.iter().min().unwrap_or(&0) as f32;
            assert!((81.0..=148.0).contains(&max));
            assert!((max - min) / max <= 0.33);
        }
        for pixel in [
            [100, 100, 100, 255],
            [240, 20, 10, 0],
            [2, 3, 4, 255],
            [250, 248, 245, 255],
        ] {
            assert_eq!(
                extract_palette(&image::RgbaImage::from_pixel(8, 8, image::Rgba(pixel))),
                ArtworkPalette::NEUTRAL
            );
        }
    }
    #[test]
    fn decoded_palette_matches_rgb_before_bgra_swap_and_cache_lifetime() {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([220, 40, 20, 255]))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap_or_else(|e| panic!("fixture: {e}"));
        let decoded =
            decode(&bytes, ArtworkKind::Poster).unwrap_or_else(|e| panic!("decode: {e:?}"));
        assert!(decoded.palette.primary[0] > decoded.palette.primary[2]);
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let disk = DiskCache::new(dir.path().join("artwork"), ENCODED);
        disk.write("palette", &bytes);
        assert_eq!(fs::read(disk.path("palette")).ok(), Some(bytes));
        let disk_decoded = disk
            .read("palette", ArtworkKind::Poster)
            .unwrap_or_else(|| panic!("disk decode"));
        assert_eq!(disk_decoded.palette, decoded.palette);
        assert_eq!(
            decoded.image.as_bytes(0).map(|bytes| &bytes[..4]),
            Some([20, 40, 220, 255].as_slice())
        );
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("detail");
        let key = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/palette.png")
            .unwrap_or_else(|| panic!("key"));
        store.enqueue("detail", generation, key.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("job"));
        let palette = decoded.palette;
        assert!(store.complete(&job, Ok(decoded)));
        assert_eq!(store.ambient(&key), Some(palette));
        let other_kind =
            ArtworkKey::new(ArtworkKind::Backdrop, &key.url).unwrap_or_else(|| panic!("key"));
        let other_url = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/changed.png")
            .unwrap_or_else(|| panic!("key"));
        assert_eq!(store.ambient(&other_kind), None);
        assert_eq!(store.ambient(&other_url), None);
        store.advance_generation("detail");
        assert_eq!(store.ambient(&key), Some(palette));
        // Simulate memory pressure: palette leaves with its decoded image.
        store
            .cache
            .front_mut()
            .unwrap_or_else(|| panic!("cache"))
            .bytes = POSTER_BYTES + 1;
        assert_eq!(store.trim_images().len(), 1);
        assert_eq!(store.ambient(&key), None);
        store.enqueue("detail", generation + 1, key.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("job"));
        assert!(store.complete(&job, Err(ArtworkError::InvalidImage)));
        assert_eq!(store.ambient(&key), None);
    }
    #[test]
    fn redirects_require_valid_https_urls_and_have_a_hop_limit() {
        let parse = |url| Url::parse(url).unwrap_or_else(|e| panic!("{e}"));
        let origin = parse("https://images.metahub.space/poster/a/img");
        let target = parse("https://live.metahub.space/poster/a/img");
        assert!(valid_redirect(&target, std::slice::from_ref(&origin)));
        for url in [
            "http://example.com/a",
            "file:///tmp/a",
            "https://user:pass@example.com/a",
            "https://example.com/a?token=secret",
            "https://example.com/a#fragment",
        ] {
            assert!(!valid_redirect(&parse(url), std::slice::from_ref(&origin)));
        }
        assert!(!valid_redirect(&target, &[parse("http://example.com/a")]));
        assert!(!valid_redirect(
            &target,
            &[origin.clone(), origin.clone(), origin.clone(), origin]
        ));
    }
    #[test]
    fn window_priority_reorders_and_prunes_without_replacing_active_work() {
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("browse");
        let key = |i| {
            ArtworkKey::new(ArtworkKind::Poster, &format!("https://example.com/{i}.jpg"))
                .unwrap_or_else(|| panic!("key"))
        };
        store.replace_window("browse", generation, vec![key(1), key(2), key(3)]);
        let active = store.start_next().unwrap_or_else(|| panic!("job"));
        store.replace_window("browse", generation, vec![key(3), key(4), key(1)]);
        assert_eq!(
            store
                .queue
                .iter()
                .map(|j| j.key.clone())
                .collect::<Vec<_>>(),
            vec![key(3), key(4)]
        );
        assert_eq!(store.start_next().map(|j| j.key), Some(key(3)));
        assert!(store.active.contains_key(&active.id));
        store.replace_window("browse", generation + 1, vec![]);
        assert_eq!(store.queue.len(), 1);
        store.replace_window("browse", generation, vec![]);
        assert!(store.queue.is_empty());
    }
    #[test]
    fn disk_cache_hit_miss_eviction_and_corruption() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(2, 3)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap_or_else(|e| panic!("{e}"));
        let cache = DiskCache::new(dir.path().join("artwork"), bytes.len() as u64);
        assert!(cache.read("a", ArtworkKind::Poster).is_none());
        cache.write("a", &bytes);
        assert!(cache.read("a", ArtworkKind::Poster).is_some());
        fs::File::options()
            .write(true)
            .open(cache.path("a"))
            .and_then(|file| file.set_modified(SystemTime::UNIX_EPOCH))
            .unwrap_or_else(|e| panic!("{e}"));
        let unrelated = cache.root.join("unrelated.txt");
        fs::write(&unrelated, b"preserve").unwrap_or_else(|e| panic!("{e}"));
        cache.write("b", &bytes);
        assert!(unrelated.exists());
        assert!(cache.read("a", ArtworkKind::Poster).is_none());
        assert!(cache.read("b", ArtworkKind::Poster).is_some());
        fs::write(cache.path("b"), b"corrupt").unwrap_or_else(|e| panic!("{e}"));
        assert!(cache.read("b", ArtworkKind::Poster).is_none());
        assert!(!cache.path("b").exists());
        fs::write(cache.path("b"), vec![0; ENCODED as usize + 1]).unwrap_or_else(|e| panic!("{e}"));
        assert!(cache.read("b", ArtworkKind::Poster).is_none());
        assert!(!cache.path("b").exists());
        cache.write("large", &vec![0; ENCODED as usize + 1]);
        assert!(!cache.path("large").exists());
    }
    #[cfg(unix)]
    #[test]
    fn disk_cache_does_not_follow_symlinked_root() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap_or_else(|e| panic!("{e}"));
        let root = dir.path().join("artwork");
        std::os::unix::fs::symlink(&target, &root).unwrap_or_else(|e| panic!("{e}"));
        let cache = DiskCache::new(root, DISK_BYTES);
        fs::write(cache.path("a"), b"preserve").unwrap_or_else(|e| panic!("{e}"));
        assert!(cache.read("a", ArtworkKind::Poster).is_none());
        cache.write("a", b"replacement");
        assert_eq!(
            fs::read(cache.path("a")).unwrap_or_else(|e| panic!("{e}")),
            b"preserve"
        );
    }
    /// Run explicitly with --ignored --nocapture; uses public posters only,
    /// bypasses disk cache, and does not log URLs or touch application data.
    #[test]
    #[ignore = "live Cinemeta poster timing; network-dependent"]
    fn live_poster_timing() {
        // Optional diagnostic: resolve outside the timed requests on hosts whose
        // system DNS exceeds the production five-second connection timeout.
        let resolved = if std::env::var_os("KINEMA_ARTWORK_BENCH_RESOLVE").is_some() {
            use std::net::ToSocketAddrs;
            [
                "cinemeta-catalogs.strem.io",
                "images.metahub.space",
                "live.metahub.space",
                "image.tmdb.org",
            ]
            .into_iter()
            .map(|host| {
                (
                    host,
                    (host, 443)
                        .to_socket_addrs()
                        .unwrap_or_else(|e| panic!("DNS: {e}"))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let bench_client = || {
            let mut builder = client_builder();
            for (host, addresses) in &resolved {
                builder = builder.resolve_to_addrs(host, addresses);
            }
            builder.build().unwrap_or_else(|e| panic!("{e:?}"))
        };
        let client = bench_client();
        let catalog_bytes = if let Some(path) = std::env::var_os("KINEMA_ARTWORK_BENCH_CATALOG") {
            fs::read(path).unwrap_or_else(|e| panic!("{e}"))
        } else {
            client
                // The public v3-cinemeta catalog endpoint currently returns 307
                // to this catalog host; avoid counting its redirect in setup.
                .get("https://cinemeta-catalogs.strem.io/top/catalog/movie/top.json")
                .send()
                .and_then(|r| r.bytes())
                .unwrap_or_else(|e| panic!("{e:?}"))
                .to_vec()
        };
        let catalog: serde_json::Value =
            serde_json::from_slice(&catalog_bytes).unwrap_or_else(|e| panic!("{e}"));
        let urls: Vec<String> = catalog["metas"]
            .as_array()
            .unwrap_or_else(|| panic!("metas"))
            .iter()
            .filter_map(|meta| meta["poster"].as_str())
            .filter(|url| ArtworkKey::new(ArtworkKind::Poster, url).is_some())
            .take(30)
            .map(str::to_owned)
            .collect();
        assert_eq!(urls.len(), 30);
        let run = |url: &str, client: &Client| {
            let start = Instant::now();
            let bytes = download(client, url).unwrap_or_else(|e| panic!("fetch: {e:?}"));
            let fetch_ms = start.elapsed().as_millis();
            let start = Instant::now();
            let _ = decode(&bytes, ArtworkKind::Poster).unwrap_or_else(|e| panic!("decode: {e:?}"));
            (fetch_ms, start.elapsed().as_millis())
        };
        let start = Instant::now();
        let old: Vec<_> = urls.iter().map(|url| run(url, &bench_client())).collect();
        eprintln!(
            "old sequential fresh-client: wall={:?}, fetch_sum={}ms decode_sum={}ms",
            start.elapsed(),
            old.iter().map(|x| x.0).sum::<u128>(),
            old.iter().map(|x| x.1).sum::<u128>()
        );
        let start = Instant::now();
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = urls
                .chunks(urls.len().div_ceil(ACTIVE))
                .map(|chunk| {
                    let client = &client;
                    let run = &run;
                    scope
                        .spawn(move || chunk.iter().map(|url| run(url, client)).collect::<Vec<_>>())
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_else(|_| panic!("worker")))
                .collect::<Vec<_>>()
        });
        eprintln!(
            "new pooled concurrent: wall={:?}, fetch_sum={}ms decode_sum={}ms",
            start.elapsed(),
            results.iter().map(|x| x.0).sum::<u128>(),
            results.iter().map(|x| x.1).sum::<u128>()
        );
    }
    #[::core::prelude::v1::test]
    fn global_slots_survive_obsolete_generations_and_queue_is_bounded() {
        let mut store = ArtworkStore::default();
        assert_eq!(store.advance_generation("browse"), 1);
        for index in 0..200 {
            let key = ArtworkKey::new(
                ArtworkKind::Poster,
                &format!("https://example.com/{index}.png"),
            )
            .unwrap_or_else(|| panic!("fixture URL"));
            store.enqueue("browse", 1, key);
        }
        assert_eq!(store.queue.len(), QUEUED);
        for _ in 0..ACTIVE {
            assert!(store.start_next().is_some());
        }
        assert!(store.start_next().is_none());
        assert_eq!(store.advance_generation("browse"), 2);
        let key = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/new.png")
            .unwrap_or_else(|| panic!("fixture URL"));
        // Obsolete active work still occupies all slots, even after queued
        // obsolete work is discarded for the new route epoch.
        assert!(store.queue.is_empty());
        store.enqueue("browse", 2, key);
        assert!(store.start_next().is_none());
        assert_eq!(store.active.len(), ACTIVE);
        let id = *store
            .active
            .keys()
            .next()
            .unwrap_or_else(|| panic!("active slot"));
        let old = store
            .active
            .remove(&id)
            .unwrap_or_else(|| panic!("active job"));
        assert_ne!(
            store.generations.get(&old.owner).copied(),
            Some(old.generation)
        );
        assert_eq!(store.start_next().map(|j| j.generation), Some(2));
        assert_eq!(store.active.len(), ACTIVE);
    }
    #[::core::prelude::v1::test]
    fn stale_queued_work_is_discarded_and_same_url_is_deduplicated() {
        let mut store = ArtworkStore::default();
        assert_eq!(store.advance_generation("browse"), 1);
        let key = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/a.png")
            .unwrap_or_else(|| panic!("fixture URL"));
        store.enqueue("browse", 1, key.clone());
        store.enqueue("browse", 1, key.clone());
        assert_eq!(store.queue.len(), 1);
        assert_eq!(store.advance_generation("browse"), 2);
        assert!(store.start_next().is_none());
        assert!(store.queue.is_empty());
        store.enqueue("browse", 1, key.clone());
        assert!(store.queue.is_empty());
        store.enqueue("browse", 2, key.clone());
        assert!(store.start_next().is_some());
        store.enqueue("browse", 2, key);
        assert!(store.queue.is_empty());
    }
    #[::core::prelude::v1::test]
    fn obsolete_completion_is_rejected_and_changed_urls_never_share_pixels() {
        let mut store = ArtworkStore::default();
        assert_eq!(store.advance_generation("browse"), 1);
        let old = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/old.png")
            .unwrap_or_else(|| panic!("fixture URL"));
        let new = ArtworkKey::new(ArtworkKind::Poster, "https://example.com/new.png")
            .unwrap_or_else(|| panic!("fixture URL"));
        store.enqueue("browse", 1, old.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("job"));
        assert_eq!(store.advance_generation("browse"), 2);
        let image = DecodedArtwork {
            image: Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::new(1, 1),
            )])),
            palette: ArtworkPalette::NEUTRAL,
        };
        assert!(!store.complete(&job, Ok(image.clone())));
        assert!(store.cache.is_empty());
        assert!(store.active.is_empty());
        store.enqueue("browse", 2, new.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("job"));
        assert!(store.complete(&job, Ok(image)));
        assert!(store.lookup(&new).is_some());
        assert!(store.lookup(&old).is_none());
    }
    #[::core::prelude::v1::test]
    fn decoded_lru_has_independent_media_quotas_and_bounded_failures() {
        let mut store = ArtworkStore::default();
        let image = DecodedArtwork {
            image: Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::new(1, 1),
            )])),
            palette: ArtworkPalette::NEUTRAL,
        };
        let key = |kind, index| {
            ArtworkKey::new(kind, &format!("https://example.com/{index}.png"))
                .unwrap_or_else(|| panic!("fixture URL"))
        };
        let first = key(ArtworkKind::Poster, 1);
        let second = key(ArtworkKind::Poster, 2);
        let backdrop = key(ArtworkKind::Backdrop, 3);
        for entry in [&first, &second] {
            store.cache.push_back(Cached {
                key: entry.clone(),
                result: Ok(image.clone()),
                bytes: ArtworkKind::Poster.quota() / 2,
            });
        }
        store.cache.push_back(Cached {
            key: backdrop.clone(),
            result: Ok(image.clone()),
            bytes: ArtworkKind::Backdrop.quota(),
        });
        assert!(store.lookup(&first).is_some());
        store.cache.push_back(Cached {
            key: key(ArtworkKind::Poster, 4),
            result: Ok(image),
            bytes: 1,
        });
        assert_eq!(store.trim_images().len(), 1);
        assert!(!store.cache.iter().any(|e| e.key == second));
        assert!(store.cache.iter().any(|e| e.key == first));
        assert!(store.cache.iter().any(|e| e.key == backdrop));
        for index in 5..ENTRIES + 5 {
            store.cache.push_back(Cached {
                key: key(ArtworkKind::Poster, index),
                result: Err(ArtworkError::Transport),
                bytes: 0,
            });
        }
        store.trim_images();
        assert_eq!(store.cache.len(), ENTRIES);
    }
    #[::core::prelude::v1::test]
    fn explicit_generation_retries_failures_but_keeps_ready_images_and_active_slots() {
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("browse");
        let key = |name| {
            ArtworkKey::new(
                ArtworkKind::Poster,
                &format!("https://example.com/{name}.png"),
            )
            .unwrap_or_else(|| panic!("fixture URL"))
        };
        let failed = key("failed");
        let ready = key("ready");
        store.enqueue("browse", generation, failed.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("failed job"));
        assert!(store.complete(&job, Err(ArtworkError::Transport)));
        store.enqueue("browse", generation, failed.clone());
        assert!(store.queue.is_empty()); // No automatic retries within an epoch.
        store.enqueue("browse", generation, ready.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("ready job"));
        let image = DecodedArtwork {
            image: Arc::new(RenderImage::new(vec![image::Frame::new(
                image::RgbaImage::new(1, 1),
            )])),
            palette: ArtworkPalette::NEUTRAL,
        };
        assert!(store.complete(&job, Ok(image.clone())));
        store.enqueue("browse", generation, key("active"));
        let active = store.start_next().unwrap_or_else(|| panic!("active job"));
        let next = store.advance_generation("browse");
        assert_eq!(store.active.len(), 1);
        assert!(store.active.contains_key(&active.id));
        assert_eq!(store.cache.len(), 1);
        assert!(
            store
                .lookup(&ready)
                .is_some_and(|cached| Arc::ptr_eq(&cached, &image.image))
        );
        store.enqueue("browse", next, failed.clone());
        assert_eq!(store.queue.len(), 1);
        let retry = store.start_next().unwrap_or_else(|| panic!("retry job"));
        assert_eq!(retry.key, failed);
        assert_eq!(retry.generation, next);
        assert_eq!(store.active.len(), 2); // Obsolete active work still owns its slot.
    }
    #[::core::prelude::v1::test]
    fn unsafe_urls_are_rejected() {
        for url in [
            "file:///tmp/a.png",
            "https://user:pass@example.com/a",
            "https://example.com/a?token=secret",
            "data:image/png;base64,AAAA",
            "https://example.com/a#fragment",
        ] {
            assert!(ArtworkKey::new(ArtworkKind::Poster, url).is_none());
        }
        assert!(ArtworkKey::new(ArtworkKind::Poster, "https://example.com/a.png").is_some());
    }
    #[cfg(feature = "ui-probe-test")]
    fn settle_artwork(cx: &mut gpui::TestAppContext) {
        // GPUI's deterministic executor does not own the std worker threads.
        // Drain UI completions until the real workers finish, with a wall-clock
        // bound so failures cannot hang the suite.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            cx.run_until_parked();
            if cx.update(|app| app.global::<ArtworkStore>().active.is_empty()) {
                break;
            }
            assert!(Instant::now() < deadline, "artwork workers timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(feature = "ui-probe-test")]
    #[gpui::test]
    fn app_pipeline_fetches_local_fixture_and_rejects_obsolete_completion(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::{io::Write, net::TcpListener, thread, time::Instant};
        // This integration test uses real sockets and owned std workers, whose
        // channel wakeups intentionally originate outside GPUI's test thread.
        cx.executor().allow_parking();
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(20, 30)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap_or_else(|e| panic!("fixture: {e}"));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("bind: {e}"));
        let address = listener
            .local_addr()
            .unwrap_or_else(|e| panic!("address: {e}"));
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|e| panic!("listener: {e}"));
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("fixture accept: {error}"),
                    }
                };
                // Accepted sockets may inherit nonblocking mode on macOS.
                stream
                    .set_nonblocking(false)
                    .unwrap_or_else(|e| panic!("blocking fixture socket: {e}"));
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap_or_else(|e| panic!("timeout: {e}"));
                let mut request = [0; 1024];
                let _ = stream
                    .read(&mut request)
                    .unwrap_or_else(|e| panic!("fixture read: {e}"));
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", bytes.len()).unwrap_or_else(|e| panic!("header: {e}"));
                stream
                    .write_all(&bytes)
                    .unwrap_or_else(|e| panic!("body: {e}"));
            }
        });
        let first = format!("http://{address}/first.png");
        let obsolete = format!("http://{address}/obsolete.png");
        let generation = cx.update(|app| begin_generation("integration", app));
        cx.update(|app| {
            assert!(matches!(
                lookup_or_request("integration", generation, &first, app),
                ArtworkState::Loading
            ))
        });
        settle_artwork(cx);
        cx.update(|app| {
            assert!(matches!(
                lookup_or_request("integration", generation, &first, app),
                ArtworkState::Ready(_)
            ));
            let key = ArtworkKey::new(ArtworkKind::Poster, &first)
                .unwrap_or_else(|| panic!("fixture URL"));
            assert_eq!(ambient(&key, app), Some(ArtworkPalette::NEUTRAL));
            assert!(backdrop(&key, app).is_none());
            assert!(matches!(
                backdrop_or_request("integration", generation, &key, app),
                ArtworkState::Loading
            ));
        });
        settle_artwork(cx);
        cx.update(|app| {
            let key = ArtworkKey::new(ArtworkKind::Poster, &first)
                .unwrap_or_else(|| panic!("fixture URL"));
            assert!(backdrop(&key, app).is_some());
            assert!(matches!(
                backdrop_or_request("integration", generation, &key, app),
                ArtworkState::Ready(_)
            ));
            // A source negative-cache hit must surface as Failed for the derived
            // variant too, without consuming the server's next response.
            let failed = ArtworkKey::new(
                ArtworkKind::Backdrop,
                &format!("http://{address}/failed.png"),
            )
            .unwrap_or_else(|| panic!("fixture URL"));
            app.update_global::<ArtworkStore, _>(|store, _| {
                store.enqueue("integration", generation, failed.clone());
                let job = store.start_next().unwrap_or_else(|| panic!("negative job"));
                assert!(store.complete(&job, Err(ArtworkError::Transport)));
            });
            assert!(matches!(
                backdrop_or_request("integration", generation, &failed, app),
                ArtworkState::Loading
            ));
        });
        settle_artwork(cx);
        cx.update(|app| {
            let failed = ArtworkKey::new(
                ArtworkKind::Backdrop,
                &format!("http://{address}/failed.png"),
            )
            .unwrap_or_else(|| panic!("fixture URL"));
            assert!(matches!(
                backdrop_or_request("integration", generation, &failed, app),
                ArtworkState::Failed
            ));
        });
        cx.update(|app| {
            assert!(matches!(
                lookup_or_request("integration", generation, &obsolete, app),
                ArtworkState::Loading
            ));
            begin_generation("integration", app);
            assert_eq!(app.global::<ArtworkStore>().active.len(), 1);
        });
        settle_artwork(cx);
        cx.update(|app| {
            let key = ArtworkKey::new(ArtworkKind::Poster, &obsolete)
                .unwrap_or_else(|| panic!("fixture URL"));
            assert_eq!(status(&key, app), None);
            assert_eq!(ambient(&key, app), None);
            assert!(app.global::<ArtworkStore>().active.is_empty());
            assert!(app.global::<ArtworkStore>().tasks.is_empty());
        });
        server
            .join()
            .unwrap_or_else(|_| panic!("fixture server failed"));
    }
    #[::core::prelude::v1::test]
    fn disk_still_revisit_uses_encoded_bytes_without_transport() {
        let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("temp: {e}"));
        let disk = DiskCache::new(temp.path().to_owned(), DISK_BYTES);
        let url = "https://invalid.invalid/no-network.jpg";
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(960, 540)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap_or_else(|e| panic!("encode: {e}"));
        disk.write(url, &bytes);
        let start = Instant::now();
        let decoded = fetch_cached(url, ArtworkKind::Still, false, Some(&disk))
            .unwrap_or_else(|e| panic!("disk hit: {e:?}"));
        eprintln!(
            "episode disk read+decode: {:?} (shimmer grace: 150 ms)",
            start.elapsed()
        );
        assert_eq!(
            decoded.image.as_bytes(0).map(<[u8]>::len),
            Some(480 * 270 * 4)
        );
    }

    #[::core::prelude::v1::test]
    fn permanent_misses_skip_transport_but_transient_failures_retry() {
        use std::net::TcpListener;
        for (status, body, persists) in [
            (404, "", true),
            (410, "", true),
            (200, "not-image", true),
            (503, "", false),
        ] {
            let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("temp: {e}"));
            let disk = DiskCache::new(temp.path().to_owned(), DISK_BYTES);
            let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("bind: {e}"));
            let address = listener
                .local_addr()
                .unwrap_or_else(|e| panic!("address: {e}"));
            listener
                .set_nonblocking(true)
                .unwrap_or_else(|e| panic!("listener: {e}"));
            let server = thread::spawn(move || {
                for _ in 0..2 {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(e)
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    && Instant::now() < deadline =>
                            {
                                thread::sleep(Duration::from_millis(5))
                            }
                            Err(e) => panic!("accept: {e}"),
                        }
                    };
                    // Accepted sockets may inherit nonblocking mode on macOS.
                    stream
                        .set_nonblocking(false)
                        .unwrap_or_else(|e| panic!("blocking fixture socket: {e}"));
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap_or_else(|e| panic!("timeout: {e}"));
                    let mut request = [0; 1024];
                    let _ = stream
                        .read(&mut request)
                        .unwrap_or_else(|e| panic!("read: {e}"));
                    write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap_or_else(|e| panic!("write: {e}"));
                }
            });
            let url = format!("http://{address}/image");
            assert!(fetch_cached(&url, ArtworkKind::Still, false, Some(&disk)).is_err());
            assert_eq!(disk.missed(&url, SystemTime::now()), persists);
            if persists {
                // No server response consumed: only the explicit refresh below fetches.
                assert!(matches!(
                    fetch_cached(&url, ArtworkKind::Still, false, Some(&disk)),
                    Err(ArtworkError::CachedMiss(_))
                ));
                assert!(fetch_cached(&url, ArtworkKind::Still, true, Some(&disk)).is_err());
            } else {
                assert!(fetch_cached(&url, ArtworkKind::Still, false, Some(&disk)).is_err());
            }
            server.join().unwrap_or_else(|_| panic!("server"));
        }
    }

    #[::core::prelude::v1::test]
    fn title_refresh_clears_prior_mount_misses_without_clearing_other_titles() {
        let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("temp: {e}"));
        let disk = DiskCache::new(temp.path().to_owned(), DISK_BYTES);
        let now = SystemTime::now();
        let old_season = "https://example.com/season1-still.jpg";
        let other_title = "https://example.com/another-title.jpg";
        disk.write_miss(old_season, now, None);
        disk.associate_scope(old_season, "tmdb:tv:1");
        disk.associate_scope(old_season, "tmdb:tv:shared");
        disk.write_miss(other_title, now, None);
        disk.associate_scope(other_title, "tmdb:tv:2");
        assert!(disk.missed(old_season, now));
        // The current detail need not have loaded this older season, even after restart.
        let reopened = DiskCache::new(temp.path().to_owned(), DISK_BYTES);
        reopened.clear_scope("tmdb:tv:1");
        assert!(!reopened.missed(old_season, now));
        assert!(reopened.missed(other_title, now));
    }

    #[::core::prelude::v1::test]
    fn persisted_misses_expire_and_refresh_clears_them() {
        let temp = tempfile::tempdir().unwrap_or_else(|e| panic!("fixture: {e:?}"));
        let disk = DiskCache::new(temp.path().to_owned(), DISK_BYTES);
        let url = "https://example.com/missing.jpg";
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        disk.write_miss(url, now, None);
        assert!(disk.missed(url, now + Duration::from_secs(86_399)));
        // A new cache instance models an application restart.
        let reopened = DiskCache::new(temp.path().to_owned(), DISK_BYTES);
        assert!(reopened.missed(url, now));
        assert!(!reopened.missed(url, now + Duration::from_secs(86_400)));
        assert!(!disk.path(url).with_extension("miss").exists());
        disk.write_miss(url, now, None);
        disk.clear_miss(url);
        assert!(!disk.missed(url, now));
        let stale = disk.miss_lease(url);
        disk.clear_miss(url);
        disk.write_miss(url, now, Some(&stale));
        assert!(!disk.missed(url, now)); // obsolete worker cannot resurrect a cleared miss
        let current = disk.miss_lease(url);
        disk.write_miss(url, now, Some(&current));
        assert!(disk.missed(url, now));
        disk.clear_miss(url);
        disk.write_miss(url, now + Duration::from_secs(1), None);
        assert!(!disk.missed(url, now)); // future/corrupt timestamps never suppress retries
        assert!(permanent(ArtworkError::Unavailable));
        assert!(permanent(ArtworkError::InvalidImage));
        assert!(!permanent(ArtworkError::Transport));
        assert!(!permanent(ArtworkError::TooLarge));
    }

    #[::core::prelude::v1::test]
    fn browse_pressure_cannot_evict_episode_stills_and_hits_are_synchronous() {
        let mut store = ArtworkStore::default();
        let generation = store.advance_generation("detail");
        let still = ArtworkKey::new(ArtworkKind::Still, "https://example.com/still.jpg")
            .unwrap_or_else(|| panic!("fixture key"));
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(480, 270)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap_or_else(|e| panic!("fixture: {e:?}"));
        let decoded =
            decode(&bytes, ArtworkKind::Still).unwrap_or_else(|e| panic!("fixture: {e:?}"));
        store.enqueue("detail", generation, still.clone());
        let job = store.start_next().unwrap_or_else(|| panic!("fixture job"));
        store.complete(&job, Ok(decoded.clone()));
        for index in 0..300 {
            store.cache.push_back(Cached {
                key: test_key(index),
                result: Ok(decoded.clone()),
                bytes: 384 * 576 * 4,
            });
        }
        store.trim_images();
        let start = Instant::now();
        assert!(store.lookup(&still).is_some());
        eprintln!("episode memory hit: {:?}; no job queued", start.elapsed());
        assert!(store.queue.is_empty());
        assert!(store.active.is_empty());
    }

    #[::core::prelude::v1::test]
    fn thumbnails_fit_kind_buckets_without_upscaling() {
        for (width, height, kind, expected) in [
            (1000, 1500, ArtworkKind::Poster, (384, 576)),
            (1920, 1080, ArtworkKind::Backdrop, (1024, 576)),
            (1920, 1080, ArtworkKind::Still, (480, 270)),
            (20, 30, ArtworkKind::Poster, (20, 30)),
        ] {
            let mut bytes = Vec::new();
            image::DynamicImage::new_rgb8(width, height)
                .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
                .unwrap_or_else(|e| panic!("fixture: {e}"));
            let image = decode(&bytes, kind).unwrap_or_else(|e| panic!("decode: {e:?}"));
            let size = image.image.size(0);
            assert_eq!((size.width.0, size.height.0), expected);
            assert_eq!(
                image.image.as_bytes(0).map(<[u8]>::len),
                Some((expected.0 * expected.1 * 4) as usize)
            );
            assert!(
                (expected.0 * expected.1 * 4) as usize * 40 < ArtworkKind::Poster.quota()
                    || kind == ArtworkKind::Backdrop
            );
            let mut store = ArtworkStore::default();
            let generation = store.advance_generation("thumbnail-test");
            let key = ArtworkKey::new(kind, "https://example.com/thumbnail.png")
                .unwrap_or_else(|| panic!("fixture URL"));
            store.enqueue("thumbnail-test", generation, key);
            let job = store.start_next().unwrap_or_else(|| panic!("job"));
            assert!(store.complete(&job, Ok(image)));
            assert_eq!(
                store.cache.front().map(|entry| entry.bytes),
                Some((expected.0 * expected.1 * 4) as usize)
            );
            assert!(store.trim_images().is_empty());
        }
    }
    #[::core::prelude::v1::test]
    fn headers_exceeding_dimension_or_pixel_budget_are_rejected() {
        // Header-only PNGs: no large fixture pixels or decoder output allocation.
        fn chunk(output: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
            output.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let start = output.len();
            output.extend_from_slice(tag);
            output.extend_from_slice(data);
            let mut crc = !0u32;
            for byte in &output[start..] {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(crc & 1)));
                }
            }
            output.extend_from_slice(&(!crc).to_be_bytes());
        }
        for (width, height) in [(EDGE + 1, 1u32), (4096, 4096)] {
            let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
            let mut header = Vec::new();
            header.extend_from_slice(&width.to_be_bytes());
            header.extend_from_slice(&height.to_be_bytes());
            header.extend_from_slice(&[8, 6, 0, 0, 0]);
            chunk(&mut bytes, b"IHDR", &header);
            chunk(&mut bytes, b"IDAT", &[]);
            chunk(&mut bytes, b"IEND", &[]);
            assert!(decode(&bytes, ArtworkKind::Poster).is_err());
        }
        let mut webp = b"RIFF\x0e\0\0\0WEBPVP8X\x02\0\0\0\x02\0".to_vec();
        assert!(animated(&webp, ImageFormat::WebP));
        webp[20] = 0;
        assert!(!animated(&webp, ImageFormat::WebP));
    }
    #[::core::prelude::v1::test]
    fn decode_is_bounded_and_rejects_animation() {
        assert_eq!(
            decode(&vec![0; ENCODED as usize + 1], ArtworkKind::Poster).err(),
            Some(ArtworkError::TooLarge)
        );
        assert_eq!(
            decode(b"not an image", ArtworkKind::Poster).err(),
            Some(ArtworkError::InvalidImage)
        );
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgba8(2, 3)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap_or_else(|e| panic!("fixture: {e}"));
        let image = decode(&bytes, ArtworkKind::Poster).unwrap_or_else(|e| panic!("decode: {e:?}"));
        assert_eq!(image.image.as_bytes(0).map(<[u8]>::len), Some(24));
        let mut animated_png = bytes[..8].to_vec();
        animated_png.extend_from_slice(&[0, 0, 0, 8]);
        animated_png.extend_from_slice(b"acTL");
        assert!(animated(&animated_png, ImageFormat::Png));
    }
}

#[cfg(test)]
mod logging_tests {
    use super::*;
    use crate::providers::{
        test_logging::{capture, excludes},
        tests::fixture,
    };

    #[test]
    fn diagnostics_exclude_sensitive_urls_bytes_and_cache_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "diagnostics_exclude_sensitive_urls_bytes_and_cache_paths",
        ) {
            return Ok(());
        }

        let (mut url, server) =
            fixture(vec![(403, serde_json::json!({"error":"payload-secret"}))])?;
        url.set_path("/opaque-secret");
        url.set_query(Some("signature=query-secret"));
        let client = build_client().map_err(|_| "client init failed")?;
        let root = tempfile::tempdir()?;
        let file = root.path().join("cache-path-secret");
        fs::write(&file, b"not a directory")?;
        let cache = DiskCache::new(file, DISK_BYTES);
        let (result, output) = capture(|| {
            let result = download(&client, url.as_str());
            assert!(decode(b"image-bytes-secret", ArtworkKind::Poster).is_err());
            cache.write(url.as_str(), b"cache-bytes-secret");
            result
        });
        assert_eq!(result, Err(ArtworkError::Transport));
        assert!(output.contains("403"));
        assert!(output.contains("Artwork decode failed"));
        assert!(output.contains("disk cache operation failed"));
        excludes(
            &output,
            &[
                "opaque-secret",
                "query-secret",
                "signature=",
                "payload-secret",
                "cache-path-secret",
                "image-bytes-secret",
                "cache-bytes-secret",
            ],
        );
        server.join().map_err(|_| "fixture panicked")?;
        Ok(())
    }
}
