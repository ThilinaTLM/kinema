//! Disk cache for downloaded online subtitles (`cache/subtitles`).
//!
//! Blocking filesystem work only: call from a background executor, never from
//! rendering. Files are named `<sha256("opensubtitles:" + file_id) hex>.<ext>`
//! with an extension from [`SubtitleFormat`]. The cache has its own byte budget,
//! separate from the media quota, with LRU eviction by modification time (a
//! lookup hit touches it). It only ever deletes files whose names it owns, so a
//! misconfigured directory cannot lose other data. Reusing a cached file avoids
//! spending download allowance. This is rebuildable cache, not personal data.
//!
//! Errors and logs carry categories only: no file ids, names, paths or contents.
use kinema_core::subtitles::{MAX_SUBTITLE_BYTES, SubtitleFormat};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime},
};

/// Default byte budget for the whole directory.
pub const DEFAULT_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// Interrupted writes older than this are removed during trimming.
const STALE_PARTIAL: Duration = Duration::from_secs(60 * 60);
const PARTIAL_PREFIX: &str = ".partial-";

// Serializes lookups, writes and trims inside the process (one cache exists).
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub enum StoreError {
    /// Empty content is never a subtitle.
    Empty,
    /// Over the subtitle size cap or the cache budget.
    TooLarge,
    /// The cache directory is not a real directory.
    BadRoot,
    Io(io::ErrorKind),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "subtitle is empty",
            Self::TooLarge => "subtitle is too large to cache",
            Self::BadRoot => "subtitle cache directory is unusable",
            Self::Io(_) => "subtitle cache could not be written",
        })
    }
}
impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[derive(Clone, Debug)]
pub struct SubtitleCache {
    root: PathBuf,
    budget: u64,
}

impl SubtitleCache {
    pub fn new(root: PathBuf, budget: u64) -> Self {
        Self { root, budget }
    }

    pub fn from_paths(paths: &crate::paths::AppPaths) -> Self {
        Self::new(paths.subtitle_cache(), DEFAULT_BUDGET_BYTES)
    }

    /// The cached file for an OpenSubtitles file id, with its mtime refreshed.
    /// Missing, empty, oversized, non-regular or unreadable entries are misses;
    /// empty and oversized ones are also removed.
    pub fn lookup(&self, file_id: u64) -> Option<PathBuf> {
        let _guard = LOCK.lock().ok()?;
        if !is_real_dir(&self.root) {
            return None;
        }
        for format in SubtitleFormat::ALL {
            let path = self.root.join(file_name(file_id, format));
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if metadata.len() == 0 || metadata.len() > MAX_SUBTITLE_BYTES.min(self.budget) {
                let _ = fs::remove_file(&path);
                continue;
            }
            if let Ok(file) = fs::OpenOptions::new().write(true).open(&path)
                && file.set_modified(SystemTime::now()).is_ok()
            {
                return Some(path);
            }
            // Cannot touch: still usable if it can be read at all.
            if fs::File::open(&path).is_ok() {
                return Some(path);
            }
        }
        None
    }

    /// Atomically store a downloaded file (temp file, then rename), replacing a
    /// cached copy under the same id, then evict oldest entries over budget.
    /// The new entry is never evicted by its own write.
    pub fn store(
        &self,
        file_id: u64,
        format: SubtitleFormat,
        bytes: &[u8],
    ) -> Result<PathBuf, StoreError> {
        if bytes.is_empty() {
            return Err(StoreError::Empty);
        }
        let len = bytes.len() as u64;
        if len > MAX_SUBTITLE_BYTES || len > self.budget {
            return Err(StoreError::TooLarge);
        }
        let _guard = LOCK
            .lock()
            .map_err(|_| StoreError::Io(io::ErrorKind::Other))?;
        fs::create_dir_all(&self.root)?;
        if !is_real_dir(&self.root) {
            return Err(StoreError::BadRoot);
        }
        let path = self.root.join(file_name(file_id, format));
        let mut file = tempfile::Builder::new()
            .prefix(PARTIAL_PREFIX)
            .tempfile_in(&self.root)?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        // A different extension for the same id would make lookup ambiguous.
        for other in SubtitleFormat::ALL {
            if other != format {
                let stale = self.root.join(file_name(file_id, other));
                if fs::symlink_metadata(&stale).is_ok_and(|m| m.is_file()) {
                    let _ = fs::remove_file(stale);
                }
            }
        }
        file.persist(&path).map_err(|e| StoreError::from(e.error))?;
        // The write succeeded; trimming problems must not fail it.
        if self.trim(Some(&path)).is_err() {
            tracing::warn!(category = "trim", "Subtitle cache trim failed");
        }
        Ok(path)
    }

    /// Remove oldest owned entries until the directory fits the budget.
    fn trim(&self, keep: Option<&Path>) -> io::Result<()> {
        let now = SystemTime::now();
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let partial = name.starts_with(PARTIAL_PREFIX);
            if !(partial || owned_name(name)) || !entry.file_type()?.is_file() {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if partial {
                // Another writer is excluded by LOCK, so any partial is leftover
                // from a crash; the age check is only a safety margin.
                if now
                    .duration_since(modified)
                    .is_ok_and(|age| age > STALE_PARTIAL)
                {
                    let _ = fs::remove_file(&path);
                }
                continue;
            }
            entries.push((modified, path, metadata.len()));
        }
        let mut total: u64 = entries.iter().map(|e| e.2).sum();
        entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        for (_, path, size) in entries {
            if total <= self.budget {
                break;
            }
            if keep == Some(path.as_path()) {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                total -= size;
            }
        }
        Ok(())
    }
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

/// `<hex sha256 of "opensubtitles:" + file_id>.<ext>`.
pub fn file_name(file_id: u64, format: SubtitleFormat) -> String {
    let digest = Sha256::digest(format!("opensubtitles:{file_id}").as_bytes());
    let mut name = String::with_capacity(64 + 1 + 3);
    for byte in digest {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push('.');
    name.push_str(format.extension());
    name
}

/// A name this cache could have written: 64 lowercase hex digits and a known
/// subtitle extension.
fn owned_name(name: &str) -> bool {
    let Some((stem, extension)) = name.split_once('.') else {
        return false;
    };
    stem.len() == 64
        && stem
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && SubtitleFormat::from_extension(extension).is_some_and(|f| f.extension() == extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn set_age(path: &Path, seconds_ago: u64) -> io::Result<()> {
        File::options()
            .write(true)
            .open(path)?
            .set_modified(SystemTime::now() - Duration::from_secs(seconds_ago))
    }

    #[test]
    fn names_hash_provider_and_id_with_a_known_extension() {
        let name = file_name(7, SubtitleFormat::Srt);
        let digest = Sha256::digest(b"opensubtitles:7");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(name, format!("{hex}.srt"));
        assert!(owned_name(&name));
        assert_ne!(name, file_name(8, SubtitleFormat::Srt));
        assert!(owned_name(&file_name(1, SubtitleFormat::WebVtt)));
        for foreign in [
            "notes.txt",
            "abc.srt",
            &format!("{}.exe", &name[..64]),
            &format!("{}.SRT", &name[..64]),
            &format!("{}.srt", name[..64].to_uppercase()),
            &format!("../{name}"),
        ] {
            assert!(!owned_name(foreign), "{foreign}");
        }
    }

    #[test]
    fn store_then_lookup_round_trips_and_misses_other_ids() -> TestResult {
        let dir = tempfile::tempdir()?;
        let cache = SubtitleCache::new(dir.path().join("subtitles"), 1024);
        assert_eq!(cache.lookup(1), None, "missing directory is a miss");
        let path = cache.store(1, SubtitleFormat::Ass, b"hello")?;
        assert_eq!(
            path,
            dir.path()
                .join("subtitles")
                .join(file_name(1, SubtitleFormat::Ass))
        );
        assert_eq!(fs::read(&path)?, b"hello");
        assert_eq!(cache.lookup(1), Some(path.clone()));
        assert_eq!(cache.lookup(2), None);
        // Same id, new format: one entry only.
        let srt = cache.store(1, SubtitleFormat::Srt, b"again")?;
        assert_eq!(cache.lookup(1), Some(srt));
        assert!(!path.exists());
        // No temp files survive.
        let names: Vec<_> = fs::read_dir(dir.path().join("subtitles"))?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<_, _>>()?;
        assert_eq!(names.len(), 1);
        Ok(())
    }

    #[test]
    fn rejects_empty_and_oversized_content() -> TestResult {
        let dir = tempfile::tempdir()?;
        let cache = SubtitleCache::new(dir.path().to_path_buf(), 8);
        assert!(matches!(
            cache.store(1, SubtitleFormat::Srt, b""),
            Err(StoreError::Empty)
        ));
        assert!(matches!(
            cache.store(1, SubtitleFormat::Srt, b"123456789"),
            Err(StoreError::TooLarge)
        ));
        assert_eq!(fs::read_dir(dir.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn corrupt_empty_or_non_regular_entries_are_misses() -> TestResult {
        let dir = tempfile::tempdir()?;
        let cache = SubtitleCache::new(dir.path().to_path_buf(), 1024);
        let empty = dir.path().join(file_name(1, SubtitleFormat::Srt));
        File::create(&empty)?;
        assert_eq!(cache.lookup(1), None);
        assert!(!empty.exists(), "empty entry is dropped");
        let oversized = dir.path().join(file_name(2, SubtitleFormat::Srt));
        fs::write(&oversized, vec![b'x'; 2048])?;
        assert_eq!(cache.lookup(2), None);
        assert!(!oversized.exists());
        // A directory squatting on the name is neither returned nor removed.
        let squatter = dir.path().join(file_name(3, SubtitleFormat::Srt));
        fs::create_dir(&squatter)?;
        assert_eq!(cache.lookup(3), None);
        assert!(squatter.is_dir());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_entries_are_misses() -> TestResult {
        let dir = tempfile::tempdir()?;
        let outside = dir.path().join("outside.txt");
        fs::write(&outside, b"secret")?;
        let root = dir.path().join("cache");
        fs::create_dir(&root)?;
        std::os::unix::fs::symlink(&outside, root.join(file_name(1, SubtitleFormat::Srt)))?;
        let cache = SubtitleCache::new(root, 1024);
        assert_eq!(cache.lookup(1), None);
        assert_eq!(fs::read(outside)?, b"secret");
        Ok(())
    }

    #[test]
    fn eviction_removes_least_recently_used_first_and_keeps_the_new_entry() -> TestResult {
        let dir = tempfile::tempdir()?;
        let cache = SubtitleCache::new(dir.path().to_path_buf(), 30);
        let a = cache.store(1, SubtitleFormat::Srt, &[b'a'; 10])?;
        let b = cache.store(2, SubtitleFormat::Srt, &[b'b'; 10])?;
        let c = cache.store(3, SubtitleFormat::Srt, &[b'c'; 10])?;
        set_age(&a, 300)?;
        set_age(&b, 200)?;
        set_age(&c, 100)?;
        // A hit makes `a` the freshest, so `b` is next to go.
        assert_eq!(cache.lookup(1), Some(a.clone()));
        let d = cache.store(4, SubtitleFormat::Ass, &[b'd'; 10])?;
        assert!(a.exists() && !b.exists() && c.exists() && d.exists());
        // A larger write may evict several, but never itself.
        set_age(&a, 300)?;
        set_age(&c, 200)?;
        let e = cache.store(5, SubtitleFormat::Srt, &[b'e'; 25])?;
        assert!(e.exists());
        assert!(!a.exists() && !c.exists() && !d.exists());
        Ok(())
    }

    #[test]
    fn total_size_stays_within_budget_and_foreign_files_survive() -> TestResult {
        let dir = tempfile::tempdir()?;
        let foreign = dir.path().join("keep.txt");
        fs::write(&foreign, vec![b'f'; 500])?;
        let lookalike = dir
            .path()
            .join(format!("{}.exe", &file_name(9, SubtitleFormat::Srt)[..64]));
        fs::write(&lookalike, vec![b'f'; 500])?;
        let cache = SubtitleCache::new(dir.path().to_path_buf(), 25);
        for id in 0..10 {
            cache.store(id, SubtitleFormat::Srt, &[b'x'; 10])?;
        }
        let owned: u64 = fs::read_dir(dir.path())?
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_str().is_some_and(owned_name))
            .map(|e| e.metadata().map(|m| m.len()))
            .sum::<Result<_, _>>()?;
        assert!(owned <= 25, "{owned}");
        assert!(foreign.exists() && lookalike.exists());
        Ok(())
    }

    #[test]
    fn stale_partial_files_are_cleaned_but_fresh_ones_are_not() -> TestResult {
        let dir = tempfile::tempdir()?;
        let old = dir.path().join(format!("{PARTIAL_PREFIX}old"));
        let fresh = dir.path().join(format!("{PARTIAL_PREFIX}fresh"));
        fs::write(&old, b"x")?;
        fs::write(&fresh, b"x")?;
        set_age(&old, 2 * 60 * 60)?;
        let cache = SubtitleCache::new(dir.path().to_path_buf(), 100);
        cache.store(1, SubtitleFormat::Srt, b"data")?;
        assert!(!old.exists() && fresh.exists());
        Ok(())
    }

    #[test]
    fn a_file_in_place_of_the_directory_is_reported() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("subtitles");
        fs::write(&root, b"not a directory")?;
        let cache = SubtitleCache::new(root.clone(), 100);
        assert_eq!(cache.lookup(1), None);
        assert!(cache.store(1, SubtitleFormat::Srt, b"data").is_err());
        assert_eq!(fs::read(root)?, b"not a directory");
        Ok(())
    }
}
