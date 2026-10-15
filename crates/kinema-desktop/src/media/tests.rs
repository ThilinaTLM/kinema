use super::*;
use std::io::{Read, Seek, SeekFrom, Write};

pub(super) fn paths(root: &Path) -> AppPaths {
    AppPaths {
        data: root.into(),
        config: root.into(),
        cache: root.into(),
    }
}
pub(super) fn budget(bytes: u64) -> CacheBudget {
    CacheBudget::new(bytes).unwrap_or_else(|| panic!("test budget must be positive"))
}
fn service(root: &Path) -> MediaService {
    MediaService::start(&paths(root), budget(1024 * 1024))
        .unwrap_or_else(|e| panic!("start failed: {e}"))
}
pub(super) fn identity(size: u64) -> MediaIdentity {
    MediaIdentity {
        torrent: kinema_core::sources::InfoHash::new("0123456789012345678901234567890123456789")
            .unwrap_or_else(|e| panic!("{e}")),
        file_path: "movie.mp4".into(),
        size,
    }
}

#[test]
fn local_ranges_head_release_and_never_delete_original() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let original = root.path().join("original.mp4");
    std::fs::write(&original, b"0123456789")?;
    let service = service(root.path());
    let lease = service.register_local(&original)?;
    assert!(lease.url.starts_with("http://127.0.0.1:"));
    let client = reqwest::blocking::Client::new();
    let response = client.get(&lease.url).send()?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "video/mp4");
    assert_eq!(response.bytes()?.as_ref(), b"0123456789");
    for (range, expected, content_range) in [
        ("bytes=2-4", b"234".as_slice(), "bytes 2-4/10"),
        ("bytes=7-", b"789".as_slice(), "bytes 7-9/10"),
        ("bytes=-2", b"89".as_slice(), "bytes 8-9/10"),
        ("bytes=8-999", b"89".as_slice(), "bytes 8-9/10"),
    ] {
        let response = client.get(&lease.url).header("Range", range).send()?;
        assert_eq!(response.status(), 206);
        assert_eq!(response.headers()["content-range"], content_range);
        assert_eq!(response.bytes()?.as_ref(), expected);
    }
    let response = client.head(&lease.url).header("Range", "bytes=-3").send()?;
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["content-length"], "3");
    assert_eq!(response.headers()["content-type"], "video/mp4");
    assert!(response.bytes()?.is_empty());
    for invalid in ["bytes=10-", "bytes=0-0,2-3", "garbage", "bytes=-0"] {
        let response = client.get(&lease.url).header("Range", invalid).send()?;
        assert_eq!(response.status(), 416);
        assert_eq!(response.headers()["content-range"], "bytes */10");
    }
    assert_eq!(service.cleanup_unused()?, 0);
    assert_eq!(service.usage().reserved_bytes, 0);
    service.release(&lease.id);
    assert_eq!(client.get(&lease.url).send()?.status(), 404);
    assert_eq!(std::fs::read(original)?, b"0123456789");
    Ok(())
}
#[test]
fn empty_local_and_parallel_reads() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let empty = root.path().join("empty");
    std::fs::write(&empty, [])?;
    let service = service(root.path());
    let lease = service.register_local(&empty)?;
    let client = reqwest::blocking::Client::new();
    let response = client.get(&lease.url).send()?;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "application/octet-stream"
    );
    assert!(response.bytes()?.is_empty());
    assert_eq!(
        client.head(&lease.url).send()?.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(
        client
            .get(&lease.url)
            .header("Range", "bytes=0-")
            .send()?
            .status(),
        416
    );
    let bytes: Vec<u8> = (0..200_000).map(|i| (i % 251) as u8).collect();
    let file = root.path().join("parallel");
    std::fs::write(&file, &bytes)?;
    let lease = service.register_local(&file)?;
    let mut workers = Vec::new();
    for start in [0, 10000, 95000, 150000] {
        let url = lease.url.clone();
        let expected = bytes[start..start + 20000].to_vec();
        workers.push(std::thread::spawn(move || -> Result<(), String> {
            let response = reqwest::blocking::Client::new()
                .get(url)
                .header("Range", format!("bytes={start}-{}", start + 19999))
                .send()
                .map_err(|e| e.to_string())?;
            if response.bytes().map_err(|e| e.to_string())?.as_ref() != expected {
                return Err("parallel content differs".into());
            }
            Ok(())
        }));
    }
    for worker in workers {
        worker.join().map_err(|_| "worker panicked")??;
    }
    Ok(())
}
#[test]
fn durable_ranges_recovery_quota_cleanup_and_lock() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let managed = root.path().join("media");
    {
        let (_lock, _) = storage::open(&managed)?;
        assert!(storage::open(&managed).is_err());
        let identity = identity(100);
        let asset = storage::create(&managed, &storage::key(&identity), identity)?;
        let mut payload = std::fs::OpenOptions::new()
            .write(true)
            .open(asset.dir.join("payload"))?;
        payload.seek(SeekFrom::Start(20))?;
        payload.write_all(b"durable bytes")?;
        payload.sync_all()?;
        storage::checkpoint(&asset, &[(20, 33)])?;
        // Simulate a later uncheckpointed write. It must never be advertised after restart.
        payload.seek(SeekFrom::Start(50))?;
        payload.write_all(b"uncommitted")?;
        payload.sync_all()?;
    }
    let service = service(root.path());
    assert_eq!(service.usage().reserved_bytes, 100);
    assert_eq!(service.usage().downloaded_bytes, 13);
    assert!(matches!(
        service.set_limit(budget(99)),
        Err(MediaError::CacheFull)
    ));
    let state = service.shared.state.lock().map_err(|_| "poison")?;
    let asset = state.assets.values().next().ok_or("missing asset")?.clone();
    assert!(!storage::available(
        &asset.state.lock().map_err(|_| "poison")?.ranges,
        50,
        61
    ));
    drop(state);
    let lease = service.lease(Source::Remote(asset.clone()))?;
    assert_eq!(service.cleanup_unused()?, 0);
    let mut file = File::open(asset.dir.join("payload"))?;
    file.seek(SeekFrom::Start(20))?;
    let mut content = [0u8; 13];
    file.read_exact(&mut content)?;
    assert_eq!(&content, b"durable bytes");
    drop(asset);
    service.release(&lease.id);
    assert_eq!(service.cleanup_unused()?, 100);
    assert_eq!(service.usage().reserved_bytes, 0);
    Ok(())
}
#[test]
fn reject_corrupt_manifest_without_serving_sparse_holes() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    let managed = root.path().join("media");
    {
        let (_lock, _) = storage::open(&managed)?;
        let identity = identity(100);
        let asset = storage::create(&managed, &storage::key(&identity), identity)?;
        storage::checkpoint(&asset, &[(0, 101)])?;
    }
    assert!(matches!(
        MediaService::start(&paths(root.path()), budget(1000)),
        Err(MediaError::Storage)
    ));
    Ok(())
}
#[cfg(unix)]
#[test]
fn symlinks_are_not_followed_or_cleaned() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    symlink(outside.path(), root.path().join("media"))?;
    assert!(matches!(
        MediaService::start(&paths(root.path()), budget(1000)),
        Err(MediaError::Storage)
    ));
    assert_eq!(std::fs::read_dir(outside.path())?.count(), 0);
    Ok(())
}

#[test]
fn creation_deletion_crash_phases_and_checkpoint_leftovers_recover()
-> Result<(), Box<dyn std::error::Error>> {
    // These are the complete crash states before/after each durable protocol step.
    for prefix in [".create-", ".delete-"] {
        for phase in 0..4 {
            let root = tempfile::tempdir()?;
            let managed = root.path().join("media");
            std::fs::create_dir(&managed)?;
            let key = storage::key(&identity(100));
            let staged = managed.join(format!("{prefix}{key}"));
            std::fs::create_dir(&staged)?;
            let files: &[&str] = match (prefix, phase) {
                (".create-", 0) | (".delete-", 3) => &[],
                (".create-", 1) => &["payload"],
                (".create-", 2) => &["payload", "manifest.next"],
                (".create-", 3) | (".delete-", 1) => &["payload", "manifest.json"],
                (".delete-", 0) => &["payload", "manifest.json", "manifest.next"],
                (".delete-", 2) => &["manifest.json"],
                _ => return Err("invalid test phase".into()),
            };
            for file in files {
                std::fs::write(staged.join(file), b"interrupted artifact")?;
            }
            let service = service(root.path());
            assert_eq!(service.usage().reserved_bytes, 0);
            assert!(!staged.exists());
        }
    }
    let root = tempfile::tempdir()?;
    let managed = root.path().join("media");
    {
        let (_lock, _) = storage::open(&managed)?;
        let identity = identity(100);
        let asset = storage::create(&managed, &storage::key(&identity), identity)?;
        std::fs::write(asset.dir.join("manifest.next"), b"half written JSON")?;
        std::fs::write(managed.join("unrelated-user-file"), b"keep")?;
    }
    let service = service(root.path());
    assert_eq!(service.usage().reserved_bytes, 100);
    assert_eq!(service.cleanup_unused()?, 100);
    assert_eq!(std::fs::read(managed.join("unrelated-user-file"))?, b"keep");
    Ok(())
}
#[test]
fn failed_cleanup_remains_counted_and_released_http_reader_pins_payload()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let service = service(root.path());
    let identity = identity(100);
    let asset = storage::create(&service.shared.root, &storage::key(&identity), identity)?;
    service
        .shared
        .state
        .lock()
        .map_err(|_| "poison")?
        .assets
        .insert(storage::key(&asset.identity), asset.clone());
    let lease = service.lease(Source::Remote(asset.clone()))?;
    // The route clones Source while holding the session-map lock, independently pinning the asset.
    let reader = service
        .shared
        .state
        .lock()
        .map_err(|_| "poison")?
        .sessions
        .get(&lease.id)
        .cloned()
        .ok_or("missing lease")?;
    let dir = asset.dir.clone();
    drop(asset);
    service.release(&lease.id);
    assert_eq!(service.cleanup_unused()?, 0);
    drop(reader);
    std::fs::write(dir.join("unknown-user-file"), b"keep")?;
    assert_eq!(service.cleanup_unused().err(), Some(MediaError::Storage));
    assert_eq!(service.usage().reserved_bytes, 100);
    let tombstone = service.shared.root.join(format!(
        ".delete-{}",
        storage::key(&super::tests::identity(100))
    ));
    assert_eq!(std::fs::read(tombstone.join("unknown-user-file"))?, b"keep");
    std::fs::remove_file(tombstone.join("unknown-user-file"))?;
    assert_eq!(service.cleanup_unused()?, 100);
    assert_eq!(service.usage().reserved_bytes, 0);
    Ok(())
}
#[test]
fn outstanding_disk_reservations_and_overflow_are_rejected() {
    let gib = 1024 * 1024 * 1024;
    assert!(admission(10 * gib, 30 * gib, 0, 0, 15 * gib).is_ok());
    assert_eq!(
        admission(10 * gib, 30 * gib, 10 * gib, 0, 15 * gib),
        Err(MediaError::CacheFull)
    );
    assert!(admission(10 * gib, 30 * gib, 10 * gib, 10 * gib, 15 * gib).is_ok());
    assert_eq!(
        admission(10, u64::MAX, u64::MAX - 5, 0, u64::MAX),
        Err(MediaError::CacheFull)
    );
    assert_eq!(
        admission(1, u64::MAX, 0, 1, u64::MAX),
        Err(MediaError::Storage)
    );
    let asset = |name: &str| {
        Arc::new(Asset {
            dir: PathBuf::new(),
            identity: MediaIdentity {
                file_path: name.into(),
                ..identity(u64::MAX)
            },
            state: Mutex::new(AssetState {
                ranges: RangeSet::new(),
                priority: None,
                error: None,
            }),
            changed: Condvar::new(),
            cancel: AtomicBool::new(true),
            running: AtomicBool::new(false),
        })
    };
    let mut assets = HashMap::new();
    assets.insert("a".into(), asset("a"));
    assets.insert("b".into(), asset("b"));
    assert_eq!(
        totals(&State {
            sessions: HashMap::new(),
            assets,
            limit: u64::MAX
        }),
        Err(MediaError::Storage)
    );
}

#[test]
fn unknown_expiry_has_five_minute_cap_and_known_expiry_margin_is_checked() {
    assert!(!storage::needs_renewal(
        None,
        Duration::from_secs(299),
        Some(100)
    ));
    assert!(storage::needs_renewal(
        None,
        Duration::from_secs(300),
        Some(100)
    ));
    assert!(!storage::needs_renewal(
        Some(106),
        Duration::ZERO,
        Some(100)
    ));
    assert!(storage::needs_renewal(Some(105), Duration::ZERO, Some(100)));
    assert!(storage::needs_renewal(Some(100), Duration::ZERO, None));
    assert!(storage::needs_renewal(
        Some(u64::MAX),
        Duration::ZERO,
        Some(u64::MAX - 1)
    ));
}

#[test]
fn mime_selection_is_static_case_insensitive_and_defaults_safely() {
    for (name, expected) in [
        ("movie.MP4", "video/mp4"),
        ("movie.m4v", "video/mp4"),
        ("movie.mkv", "video/x-matroska"),
        ("movie.webm", "video/webm"),
        ("movie.mov", "video/quicktime"),
        ("movie.avi", "video/x-msvideo"),
        ("movie.mpeg", "video/mpeg"),
        ("movie.mpg", "video/mpeg"),
        ("audio.mp3", "audio/mpeg"),
        ("audio.flac", "audio/flac"),
        ("audio.wav", "audio/wav"),
        ("audio.ogg", "audio/ogg"),
        ("unknown.bin", "application/octet-stream"),
        ("no-extension", "application/octet-stream"),
        ("movie.mp4\r\ninjected", "application/octet-stream"),
    ] {
        assert_eq!(content_type(Path::new(name)), expected);
    }
}

#[cfg(unix)]
#[test]
fn native_alias_parent_is_allowed_but_owned_root_and_asset_symlinks_are_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir()?;
    let real = root.path().join("real-parent");
    std::fs::create_dir(&real)?;
    let alias = root.path().join("native-alias");
    symlink(&real, &alias)?;
    let service = service(&alias);
    assert_eq!(service.shared.root, real.canonicalize()?.join("media"));
    let original = real.join("external.mp4");
    std::fs::write(&original, b"external bytes")?;
    let lease = service.register_local(&alias.join("external.mp4"))?;
    assert_eq!(
        reqwest::blocking::get(&lease.url)?.bytes()?.as_ref(),
        b"external bytes"
    );
    service.release(&lease.id);
    assert_eq!(service.cleanup_unused()?, 0);
    assert_eq!(std::fs::read(&original)?, b"external bytes");
    // Existing owned-root symlink rejection is also covered by symlinks_are_not_followed_or_cleaned.
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("keep"), b"keep")?;
    std::fs::create_dir(root.path().join("media"))?;
    symlink(
        outside.path(),
        root.path().join("media").join(storage::key(&identity(10))),
    )?;
    assert!(matches!(
        MediaService::start(&paths(root.path()), budget(1000)),
        Err(MediaError::Storage)
    ));
    assert_eq!(std::fs::read(outside.path().join("keep"))?, b"keep");
    Ok(())
}
#[test]
fn partial_managed_payload_cannot_be_registered_as_complete_local_media()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let service = service(root.path());
    let identity = identity(100);
    let asset = storage::create(&service.shared.root, &storage::key(&identity), identity)?;
    service
        .shared
        .state
        .lock()
        .map_err(|_| "poison")?
        .assets
        .insert(storage::key(&asset.identity), asset.clone());
    assert_eq!(
        service.register_local(&asset.dir.join("payload")).err(),
        Some(MediaError::InvalidLocalFile)
    );
    #[cfg(unix)]
    {
        let alias = root.path().join("external-alias.mp4");
        std::os::unix::fs::symlink(asset.dir.join("payload"), &alias)?;
        assert_eq!(
            service.register_local(&alias).err(),
            Some(MediaError::InvalidLocalFile)
        );
    }
    assert_eq!(service.usage().downloaded_bytes, 0);
    assert!(
        service
            .shared
            .state
            .lock()
            .map_err(|_| "poison")?
            .sessions
            .is_empty()
    );
    drop(asset);
    assert_eq!(service.cleanup_unused()?, 100);
    Ok(())
}
#[cfg(unix)]
#[test]
fn managed_payload_symlink_is_rejected_without_touching_external_file()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let managed = root.path().join("media");
    let external = root.path().join("external.mp4");
    std::fs::write(&external, b"keep")?;
    {
        let (_lock, _) = storage::open(&managed)?;
        let identity = identity(4);
        let asset = storage::create(&managed, &storage::key(&identity), identity)?;
        std::fs::remove_file(asset.dir.join("payload"))?;
        std::os::unix::fs::symlink(&external, asset.dir.join("payload"))?;
    }
    assert!(matches!(
        MediaService::start(&paths(root.path()), budget(1000)),
        Err(MediaError::Storage)
    ));
    assert_eq!(std::fs::read(external)?, b"keep");
    Ok(())
}
