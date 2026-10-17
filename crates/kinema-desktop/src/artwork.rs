//! App-owned bounded artwork. Call `request` from event handlers (or rendering: it
//! only queues work); all transport and decoding runs on the background executor.
//! UI elements should use returned RenderImages directly, not a RetainAll cache.
use gpui_kit::{App, BorrowAppContext, Global, RenderImage, Task};
use image::{ImageFormat, ImageReader};
use reqwest::{Url, blocking::Client, redirect::Policy};
use std::{
    collections::{HashMap, VecDeque},
    io::{Cursor, Read},
    sync::Arc,
    time::Duration,
};

const ACTIVE: usize = 4;
const QUEUED: usize = 128;
const ENCODED: u64 = 4 * 1024 * 1024;
const PIXELS: u64 = 8 * 1024 * 1024;
const EDGE: u32 = 8192;
const ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArtworkKind {
    Poster,
    Backdrop,
}
impl ArtworkKind {
    fn thumbnail_bounds(self) -> (u32, u32) {
        match self {
            Self::Poster => (384, 576),
            Self::Backdrop => (1024, 576),
        }
    }
    fn quota(self) -> usize {
        match self {
            Self::Poster => 48 * 1024 * 1024,
            Self::Backdrop => 16 * 1024 * 1024,
        }
    }
}
/// Stable identity includes the exact URL: changed artwork never reuses stale pixels.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtworkKey {
    pub kind: ArtworkKind,
    url: String,
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
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtworkError {
    Transport,
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
}
struct Cached {
    key: ArtworkKey,
    result: Result<Arc<RenderImage>, ArtworkError>,
    bytes: usize,
}
#[derive(Default)]
struct ArtworkStore {
    generations: HashMap<String, u64>,
    queue: VecDeque<Job>,
    active: HashMap<u64, Job>,
    tasks: HashMap<u64, Task<()>>,
    cache: VecDeque<Cached>,
    next: u64,
}
impl Global for ArtworkStore {}
pub fn init(cx: &mut App) {
    if !cx.has_global::<ArtworkStore>() {
        cx.set_global(ArtworkStore::default());
    }
}
/// Invalidate queued work for one view. Running work retains its global slot until
/// completion; its obsolete result is rejected, not detached/cancelled and replaced.
/// Use stable view identifiers, not per-title/query strings. At most 64 owners and
/// 128-byte owner names are accepted; zero denotes a rejected registration.
pub fn begin_generation(owner: &str, cx: &mut App) -> u64 {
    init(cx);
    cx.update_global::<ArtworkStore, _>(|store, _| store.advance_generation(owner))
}
/// Cache lookup touches the decoded LRU. None includes pending and failed images;
/// `status` distinguishes a negative-cache hit from missing work.
pub fn get(key: &ArtworkKey, cx: &mut App) -> Option<Arc<RenderImage>> {
    init(cx);
    cx.update_global::<ArtworkStore, _>(|store, _| store.lookup(key))
}
pub fn status(key: &ArtworkKey, cx: &App) -> Option<Result<(), ArtworkError>> {
    cx.try_global::<ArtworkStore>()?
        .cache
        .iter()
        .find(|e| &e.key == key)
        .map(|e| e.result.as_ref().map(|_| ()).map_err(|e| *e))
}
pub fn request(owner: &str, generation: u64, key: ArtworkKey, cx: &mut App) {
    init(cx);
    cx.update_global::<ArtworkStore, _>(|store, _| store.enqueue(owner, generation, key));
    pump(cx);
}
fn pump(cx: &mut App) {
    loop {
        let job = cx.update_global::<ArtworkStore, _>(|store, _| store.start_next());
        let Some(job) = job else {
            break;
        };
        let id = job.id;
        let task = cx.spawn(async move |cx| {
            let url = job.key.url.clone();
            let kind = job.key.kind;
            let result = cx
                .background_executor()
                .spawn(async move { fetch(&url, kind) })
                .await;
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
impl ArtworkStore {
    fn advance_generation(&mut self, owner: &str) -> u64 {
        if owner.len() > 128
            || (!self.generations.contains_key(owner) && self.generations.len() >= 64)
        {
            return 0;
        }
        let generation = self.generations.entry(owner.into()).or_default();
        *generation = generation.wrapping_add(1);
        self.queue.retain(|job| job.owner != owner);
        *generation
    }

    fn enqueue(&mut self, owner: &str, generation: u64, key: ArtworkKey) {
        if self.generations.get(owner).copied() != Some(generation)
            || self.cache.iter().any(|e| e.key == key)
            || self
                .queue
                .iter()
                .chain(self.active.values())
                .any(|j| j.key == key && j.owner == owner && j.generation == generation)
            || self.queue.len() >= QUEUED
        {
            return;
        }
        self.next = self.next.wrapping_add(1);
        self.queue.push_back(Job {
            id: self.next,
            owner: owner.into(),
            generation,
            key,
        });
    }
    fn start_next(&mut self) -> Option<Job> {
        if self.active.len() >= ACTIVE {
            return None;
        }
        self.queue.retain(|job| {
            self.generations.get(&job.owner).copied() == Some(job.generation)
                && !self.cache.iter().any(|e| e.key == job.key)
        });
        let index = self
            .queue
            .iter()
            .position(|j| !self.active.values().any(|active| active.key == j.key))?;
        let job = self.queue.remove(index)?;
        self.active.insert(job.id, job.clone());
        Some(job)
    }

    fn complete(&mut self, job: &Job, result: Result<Arc<RenderImage>, ArtworkError>) -> bool {
        self.active.remove(&job.id);
        self.tasks.remove(&job.id);
        if self.generations.get(&job.owner).copied() != Some(job.generation) {
            return false;
        }
        let bytes = result
            .as_ref()
            .map_or(0, |image| image.as_bytes(0).map_or(0, <[u8]>::len));
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
        let result = entry.result.clone().ok();
        self.cache.push_back(entry);
        result
    }
    fn trim_images(&mut self) -> Vec<Arc<RenderImage>> {
        let mut evicted = Vec::new();
        for kind in [ArtworkKind::Poster, ArtworkKind::Backdrop] {
            while self
                .cache
                .iter()
                .filter(|e| e.key.kind == kind)
                .map(|e| e.bytes)
                .sum::<usize>()
                > kind.quota()
            {
                if let Some(index) = self.cache.iter().position(|e| e.key.kind == kind)
                    && let Some(Cached {
                        result: Ok(image), ..
                    }) = self.cache.remove(index)
                {
                    evicted.push(image);
                }
            }
        }
        while self.cache.len() > ENTRIES {
            if let Some(Cached {
                result: Ok(image), ..
            }) = self.cache.pop_front()
            {
                evicted.push(image);
            }
        }
        evicted
    }
}

fn fetch(url: &str, kind: ArtworkKind) -> Result<Arc<RenderImage>, ArtworkError> {
    // No redirects: credential-bearing or non-HTTP redirect targets cannot bypass
    // key validation. No OS proxy credentials, cookies or provider token is used.
    let client = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| ArtworkError::Transport)?;
    let response = client
        .get(url)
        .send()
        .map_err(|_| ArtworkError::Transport)?;
    if !response.status().is_success() {
        return Err(ArtworkError::Transport);
    }
    if response.content_length().is_some_and(|n| n > ENCODED) {
        return Err(ArtworkError::TooLarge);
    }
    let mut bytes = Vec::new();
    response
        .take(ENCODED + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ArtworkError::Transport)?;
    decode(&bytes, kind)
}
fn decode(bytes: &[u8], kind: ArtworkKind) -> Result<Arc<RenderImage>, ArtworkError> {
    if bytes.len() as u64 > ENCODED {
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
    // GPUI's renderer expects BGRA (same convention as its built-in image loader).
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
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
        // Obsolete active work still occupies all four slots, even after queued
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
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(1, 1),
        )]));
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
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(1, 1),
        )]));
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
        for index in 5..205 {
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
    #[gpui_kit::test]
    fn app_pipeline_fetches_local_fixture_and_rejects_obsolete_completion(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use std::{io::Write, net::TcpListener, thread, time::Instant};
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
        cx.run_until_parked();
        cx.update(|app| {
            assert!(matches!(
                lookup_or_request("integration", generation, &first, app),
                ArtworkState::Ready(_)
            ))
        });
        cx.update(|app| {
            assert!(matches!(
                lookup_or_request("integration", generation, &obsolete, app),
                ArtworkState::Loading
            ));
            begin_generation("integration", app);
            assert_eq!(app.global::<ArtworkStore>().active.len(), 1);
        });
        cx.run_until_parked();
        cx.update(|app| {
            let key = ArtworkKey::new(ArtworkKind::Poster, &obsolete)
                .unwrap_or_else(|| panic!("fixture URL"));
            assert_eq!(status(&key, app), None);
            assert!(app.global::<ArtworkStore>().active.is_empty());
            assert!(app.global::<ArtworkStore>().tasks.is_empty());
        });
        server
            .join()
            .unwrap_or_else(|_| panic!("fixture server failed"));
    }
    #[::core::prelude::v1::test]
    fn thumbnails_fit_kind_buckets_without_upscaling() {
        for (width, height, kind, expected) in [
            (1000, 1500, ArtworkKind::Poster, (384, 576)),
            (1920, 1080, ArtworkKind::Backdrop, (1024, 576)),
            (20, 30, ArtworkKind::Poster, (20, 30)),
        ] {
            let mut bytes = Vec::new();
            image::DynamicImage::new_rgb8(width, height)
                .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
                .unwrap_or_else(|e| panic!("fixture: {e}"));
            let image = decode(&bytes, kind).unwrap_or_else(|e| panic!("decode: {e:?}"));
            let size = image.size(0);
            assert_eq!((size.width.0, size.height.0), expected);
            assert_eq!(
                image.as_bytes(0).map(<[u8]>::len),
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
        assert_eq!(image.as_bytes(0).map(<[u8]>::len), Some(24));
        let mut animated_png = bytes[..8].to_vec();
        animated_png.extend_from_slice(&[0, 0, 0, 8]);
        animated_png.extend_from_slice(b"acTL");
        assert!(animated(&animated_png, ImageFormat::Png));
    }
}
