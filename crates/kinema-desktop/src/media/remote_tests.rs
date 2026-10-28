use super::*;
use crate::providers::SensitiveUrl;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::AtomicUsize,
};

struct Fixture {
    url: reqwest::Url,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    requests: Arc<Mutex<Vec<(u64, u64)>>>,
    calls: Arc<AtomicUsize>,
    mode: Arc<AtomicUsize>,
    bytes: Arc<Vec<u8>>,
    gate: Arc<AtomicBool>,
    stage: Arc<AtomicUsize>,
}
impl Fixture {
    fn start(size: usize, mode: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let url = reqwest::Url::parse(&format!("http://{}/bytes", listener.local_addr()?))?;
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let mode = Arc::new(AtomicUsize::new(mode));
        let bytes = Arc::new((0..size).map(|i| (i % 251) as u8).collect::<Vec<_>>());
        let gate = Arc::new(AtomicBool::new(false));
        let permitted = gate.clone();
        let stage = Arc::new(AtomicUsize::new(0));
        let progress = stage.clone();
        let (canceled, recorded, count, behavior, data) = (
            stop.clone(),
            requests.clone(),
            calls.clone(),
            mode.clone(),
            bytes.clone(),
        );
        let worker = std::thread::spawn(move || {
            while !canceled.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                // BSD/macOS can inherit the listener's nonblocking mode. The
                // fixture uses blocking read_exact/write_all, including large bodies.
                stream
                    .set_nonblocking(false)
                    .unwrap_or_else(|e| panic!("blocking fixture socket: {e}"));
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut headers = Vec::new();
                let mut byte = [0];
                while headers.len() < 8192 && !headers.ends_with(b"\r\n\r\n") {
                    if stream.read_exact(&mut byte).is_err() {
                        break;
                    }
                    headers.push(byte[0]);
                }
                let headers = String::from_utf8_lossy(&headers).to_ascii_lowercase();
                let range = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="));
                let Some((start, end)) = range
                    .and_then(|r| r.split_once('-'))
                    .and_then(|(s, e)| Some((s.parse::<u64>().ok()?, e.parse::<u64>().ok()? + 1)))
                else {
                    continue;
                };
                recorded
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((start, end));
                let call = count.fetch_add(1, Ordering::Relaxed);
                let mut mode = behavior.load(Ordering::Relaxed);
                if mode == 13 && call == 0 {
                    progress.store(1, Ordering::Release);
                    wait_gate(&permitted, &canceled);
                    mode = behavior.load(Ordering::Relaxed);
                }
                if mode == 12 {
                    // Close before sending response headers.
                    continue;
                }
                if mode == 1 && call == 0 || mode == 5 && call % 2 == 0 || mode == 6 {
                    let _ = stream.write_all(
                        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    continue;
                }
                if mode == 3 {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        data.len()
                    );
                    let _ = stream.write_all(&data);
                    continue;
                }
                if mode == 7 {
                    let _ = stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                if mode == 15 {
                    let _ = stream.write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let reported_start = if mode == 2 { start + 1 } else { start };
                let reported_size = data.len() + usize::from(mode == 11);
                let encoding = if mode == 4 {
                    "Content-Encoding: gzip\r\n"
                } else {
                    ""
                };
                let length = if mode == 10 {
                    end - start + 1
                } else {
                    end - start
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {reported_start}-{}/{}\r\n{encoding}Connection: close\r\n\r\n",
                    length,
                    end - 1,
                    reported_size
                );
                if mode == 14 && call == 0 {
                    let prefix_end = end.min(start + 64 * 1024);
                    let _ = stream.write_all(&data[start as usize..prefix_end as usize]);
                    progress.store(2, Ordering::Release);
                    wait_gate(&permitted, &canceled);
                    let _ = stream.write_all(&data[prefix_end as usize..end as usize]);
                    continue;
                }
                if mode == 8 {
                    let _ = stream.write_all(&data[start as usize..(start + 1) as usize]);
                    continue;
                }
                if mode == 9 && call == 0 {
                    let deadline = std::time::Instant::now() + Duration::from_secs(5);
                    while !permitted.load(Ordering::Acquire)
                        && !canceled.load(Ordering::Relaxed)
                        && std::time::Instant::now() < deadline
                    {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                std::thread::sleep(Duration::from_millis(15));
                if end <= data.len() as u64 {
                    let _ = stream.write_all(&data[start as usize..end as usize]);
                }
            }
        });
        Ok(Self {
            url,
            stop,
            worker: Some(worker),
            requests,
            calls,
            mode,
            bytes,
            gate,
            stage,
        })
    }
    fn resolved(&self) -> ResolvedMedia {
        ResolvedMedia {
            identity: super::tests::identity(self.bytes.len() as u64),
            file_name: "movie.mp4".into(),
            url: SensitiveUrl::for_test(self.url.clone()),
            expires_at: None,
        }
    }
}
fn wait_gate(permitted: &AtomicBool, canceled: &AtomicBool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !permitted.load(Ordering::Acquire)
        && !canceled.load(Ordering::Relaxed)
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(5));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[track_caller]
fn wait_until(predicate: impl FnMut() -> bool) {
    wait_for(Duration::from_secs(45), predicate);
}
#[track_caller]
fn wait_for(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + timeout;
    while !predicate() {
        assert!(std::time::Instant::now() < end, "condition timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn service(root: &Path, limit: u64) -> MediaService {
    MediaService::start(&super::tests::paths(root), super::tests::budget(limit))
        .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn fixture_handles_fragmented_headers_and_backpressured_body()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::start(CHUNK as usize, 0)?;
    let address = fixture.url.socket_addrs(|| None)?[0];
    let mut stream = std::net::TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET /bytes HTTP/1.1\r\nHost: localhost\r\n")?;
    // A slow client must not make a blocking fixture abandon a partial request.
    // macOS accepted sockets inherit nonblocking mode unless reset explicitly.
    std::thread::sleep(Duration::from_millis(50));
    write!(stream, "Range: bytes=0-{}\r\n\r\n", CHUNK - 1)?;
    // Let the large response exceed socket buffering before consuming it.
    std::thread::sleep(Duration::from_millis(50));
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("missing response headers")?
        + 4;
    assert!(response.starts_with(b"HTTP/1.1 206 Partial Content\r\n"));
    assert_eq!(&response[end..], fixture.bytes.as_slice());
    Ok(())
}

#[test]
fn remote_background_progress_seek_and_durable_restart() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start((CHUNK * 5) as usize, 9)?;
    let service = service(root.path(), CHUNK * 10);
    let resolved = fixture.resolved();
    let renewal = resolved.clone();
    let lease = service.register_remote(resolved, Arc::new(move || Ok(renewal.clone())))?;
    // Acquisition starts without an HTTP reader. Gate its first response while a
    // deterministic missing-tail seek queues priority before the next fetch.
    wait_until(|| fixture.calls.load(Ordering::Relaxed) == 1);
    let url = lease.url.clone();
    let reader = std::thread::spawn(move || -> Result<Vec<u8>, String> {
        let response = reqwest::blocking::Client::new()
            .get(url)
            .header("Range", format!("bytes={}-", CHUNK * 4 + 100))
            .send()
            .map_err(|e| e.to_string())?;
        assert_eq!(response.status(), 206);
        Ok(response.bytes().map_err(|e| e.to_string())?.to_vec())
    });
    wait_until(|| {
        service.shared.state.lock().is_ok_and(|s| {
            s.assets.values().any(|a| {
                a.state
                    .lock()
                    .is_ok_and(|s| s.priority == Some(CHUNK * 4 + 100))
            })
        })
    });
    fixture.gate.store(true, Ordering::Release);
    assert_eq!(
        reader.join().map_err(|_| "reader panicked")??,
        &fixture.bytes[(CHUNK * 4 + 100) as usize..]
    );
    wait_until(|| service.usage().downloaded_bytes == fixture.bytes.len() as u64);
    assert!(service.session_errors().is_empty());
    assert_eq!(service.cleanup_unused()?.freed_bytes, 0);
    let requested = fixture.requests.lock().map_err(|_| "poison")?.clone();
    assert!(requested.iter().any(|&(s, _)| s == CHUNK * 4 + 100));
    service.release(&lease.id)?;
    wait_until(|| {
        service.shared.state.lock().is_ok_and(|s| {
            s.assets
                .values()
                .all(|a| !a.running.load(Ordering::Acquire))
        })
    });
    drop(service);
    // The service thread owns the root lock until it has really stopped.
    let service = reopen(root.path(), CHUNK * 10)?;
    assert_eq!(service.usage().downloaded_bytes, fixture.bytes.len() as u64);
    assert_eq!(
        service.cleanup_unused()?.freed_bytes,
        fixture.bytes.len() as u64
    );
    Ok(())
}
#[test]
fn expired_and_rejected_access_renew_without_discarding_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start((CHUNK * 4) as usize, 5)?;
    let service = service(root.path(), CHUNK * 10);
    let mut resolved = fixture.resolved();
    resolved.expires_at = Some(0);
    let next = fixture.resolved();
    let count = Arc::new(AtomicUsize::new(0));
    let renewals = count.clone();
    let lease = service.register_remote(
        resolved,
        Arc::new(move || {
            renewals.fetch_add(1, Ordering::Relaxed);
            Ok(next.clone())
        }),
    )?;
    wait_until(|| {
        service.usage().downloaded_bytes == CHUNK * 4 || !service.session_errors().is_empty()
    });
    assert!(
        service.session_errors().is_empty(),
        "errors: {:?}; calls: {}; renewals: {}",
        service.session_errors(),
        fixture.calls.load(Ordering::Relaxed),
        count.load(Ordering::Relaxed)
    );
    assert!(count.load(Ordering::Relaxed) >= 4);
    assert!(fixture.calls.load(Ordering::Relaxed) >= 8);
    service.release(&lease.id)?;
    Ok(())
}
#[test]
fn upstream_failures_are_safe_visible_and_do_not_publish_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    for (mode, expected) in [
        (2, MediaError::IdentityMismatch),
        (3, MediaError::UnsupportedRange),
        (4, MediaError::IdentityMismatch),
        (6, MediaError::RenewalExhausted),
        (7, MediaError::UnsupportedRange),
        (8, MediaError::BodyRead),
        (10, MediaError::IdentityMismatch),
        (11, MediaError::IdentityMismatch),
        (12, MediaError::Upstream),
        (15, MediaError::UnsupportedRange),
    ] {
        let root = tempfile::tempdir()?;
        let mut fixture = Fixture::start(CHUNK as usize, mode)?;
        fixture.url.set_path("/signed-path");
        fixture.url.set_query(Some("token=SECRET_TOKEN"));
        let service = service(root.path(), CHUNK * 3);
        let resolved = fixture.resolved();
        let next = resolved.clone();
        let lease = service.register_remote(resolved, Arc::new(move || Ok(next.clone())))?;
        wait_until(|| !service.session_errors().is_empty());
        let errors = service.session_errors();
        assert_eq!(errors, vec![(lease.id.clone(), expected)]);
        for (_, error) in errors {
            let rendered = format!("{error} {error:?}");
            for secret in ["SECRET_TOKEN", "signed-path", "token=", "http://"] {
                assert!(!rendered.contains(secret));
            }
        }
        if mode == 6 {
            assert_eq!(fixture.calls.load(Ordering::Relaxed), 3);
        }
        assert_eq!(service.usage().downloaded_bytes, 0);
        // Transient transport/body failures receive three bounded retries;
        // protocol/identity failures still fail immediately.
        let attempts = fixture.calls.load(Ordering::Relaxed);
        assert_eq!(
            attempts,
            if matches!(mode, 8 | 12) {
                4
            } else if mode == 6 {
                3
            } else {
                1
            }
        );
        service.release(&lease.id)?;
    }
    Ok(())
}
#[test]
fn renewal_identity_change_preserves_prior_checkpoint() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start((CHUNK * 6) as usize, 0)?;
    let service = service(root.path(), CHUNK * 10);
    let resolved = fixture.resolved();
    let mut changed = resolved.clone();
    changed.identity.size += 1;
    let lease = service.register_remote(resolved, Arc::new(move || Ok(changed.clone())))?;
    wait_until(|| service.usage().downloaded_bytes >= CHUNK);
    fixture.mode.store(6, Ordering::Relaxed);
    wait_until(|| !service.session_errors().is_empty());
    assert_eq!(service.session_errors()[0].1, MediaError::IdentityMismatch);
    assert!(service.usage().downloaded_bytes >= CHUNK);
    service.release(&lease.id)?;
    Ok(())
}
#[test]
fn quotas_and_worker_capacity_do_not_leak_new_reservations()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start((CHUNK * 2) as usize, 0)?;
    let service = service(root.path(), CHUNK);
    let resolved = fixture.resolved();
    let next = resolved.clone();
    assert!(matches!(
        service.register_remote(resolved, Arc::new(move || Ok(next.clone()))),
        Err(MediaError::FileExceedsLimit { .. })
    ));
    assert_eq!(service.usage().reserved_bytes, 0);
    {
        let mut state = service.shared.state.lock().map_err(|_| "poison")?;
        for i in 0..MAX_ASSETS {
            let mut identity = super::tests::identity(1);
            identity.file_path = i.to_string();
            let asset = storage::create(&service.shared.root, &storage::key(&identity), identity)?;
            asset.running.store(true, Ordering::Release);
            state.assets.insert(storage::key(&asset.identity), asset);
        }
    }
    let resolved = fixture.resolved();
    let next = resolved.clone();
    assert_eq!(
        service
            .register_remote(resolved, Arc::new(move || Ok(next.clone())))
            .err(),
        Some(MediaError::Busy)
    );
    assert_eq!(service.usage().reserved_bytes, MAX_ASSETS as u64);
    Ok(())
}

fn reopen(root: &Path, limit: u64) -> Result<MediaService, MediaError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        match MediaService::start(&super::tests::paths(root), super::tests::budget(limit)) {
            Ok(service) => return Ok(service),
            Err(MediaError::Storage) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(error),
        }
    }
}
#[test]
fn actual_interruption_restarts_only_missing_durable_ranges()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start((CHUNK * 8) as usize, 0)?;
    let service = service(root.path(), CHUNK * 10);
    let resolved = fixture.resolved();
    let next = resolved.clone();
    let lease = service.register_remote(resolved, Arc::new(move || Ok(next.clone())))?;
    wait_until(|| service.usage().downloaded_bytes >= CHUNK);
    service.release(&lease.id)?;
    wait_until(|| {
        service.shared.state.lock().is_ok_and(|s| {
            s.assets
                .values()
                .all(|a| !a.running.load(Ordering::Acquire))
        })
    });
    let downloaded = service.usage().downloaded_bytes;
    assert!((CHUNK..CHUNK * 8).contains(&downloaded));
    let key = storage::key(&fixture.resolved().identity);
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(
        service.shared.root.join(key).join("manifest.json"),
    )?)?;
    let durable: Vec<(u64, u64)> = serde_json::from_value(manifest["ranges"].clone())?;
    drop(service);
    let service = reopen(root.path(), CHUNK * 10)?;
    assert_eq!(service.usage().downloaded_bytes, downloaded);
    let request_count = fixture.requests.lock().map_err(|_| "poison")?.len();
    let resolved = fixture.resolved();
    let next = resolved.clone();
    let lease = service.register_remote(resolved, Arc::new(move || Ok(next.clone())))?;
    wait_until(|| {
        service.usage().downloaded_bytes == CHUNK * 8 || !service.session_errors().is_empty()
    });
    assert!(service.session_errors().is_empty());
    let requests = fixture.requests.lock().map_err(|_| "poison")?;
    assert!(
        requests[request_count..]
            .iter()
            .all(|&(start, end)| durable.iter().all(|&(s, e)| end <= s || start >= e))
    );
    service.release(&lease.id)?;
    Ok(())
}
#[test]
fn drop_does_not_wait_for_owned_renewal_and_no_fetch_starts_after_cancel()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start(CHUNK as usize, 0)?;
    let service = service(root.path(), CHUNK * 10);
    let mut resolved = fixture.resolved();
    resolved.expires_at = Some(0);
    let next = fixture.resolved();
    let entered = Arc::new(AtomicBool::new(false));
    let proceed = Arc::new(AtomicBool::new(false));
    let (called, allowed) = (entered.clone(), proceed.clone());
    service.register_remote(
        resolved,
        Arc::new(move || {
            called.store(true, Ordering::Release);
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !allowed.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(next.clone())
        }),
    )?;
    wait_until(|| entered.load(Ordering::Acquire));
    let before = std::time::Instant::now();
    drop(service);
    assert!(before.elapsed() < Duration::from_millis(100));
    proceed.store(true, Ordering::Release);
    let _service = reopen(root.path(), CHUNK * 10)?;
    assert_eq!(fixture.calls.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn failed_creation_artifacts_remain_reserved_until_successful_cleanup()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start(CHUNK as usize, 0)?;
    let service = service(root.path(), CHUNK * 10);
    let resolved = fixture.resolved();
    let next = resolved.clone();
    let key = storage::key(&resolved.identity);
    let staged = service.shared.root.join(format!(".create-{key}"));
    std::fs::create_dir(&staged)?;
    std::fs::write(staged.join("unrecognized"), b"preserve")?;
    assert_eq!(
        service
            .register_remote(resolved, Arc::new(move || Ok(next.clone())))
            .err(),
        Some(MediaError::Storage)
    );
    assert_eq!(service.usage().reserved_bytes, CHUNK);
    assert_eq!(service.usage().downloaded_bytes, 0);
    assert_eq!(service.cleanup_unused().err(), Some(MediaError::Storage));
    assert_eq!(service.usage().reserved_bytes, CHUNK);
    let tombstone = service.shared.root.join(format!(".delete-{key}"));
    assert_eq!(std::fs::read(tombstone.join("unrecognized"))?, b"preserve");
    std::fs::remove_file(tombstone.join("unrecognized"))?;
    assert_eq!(service.cleanup_unused()?.freed_bytes, CHUNK);
    Ok(())
}

#[test]
fn remote_identity_filename_sets_get_and_head_mime_not_display_filename()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start(CHUNK as usize, 0)?;
    let service = service(root.path(), CHUNK * 3);
    let mut resolved = fixture.resolved();
    resolved.identity.file_path = "release/movie.MKV".into();
    resolved.file_name = "untrusted.mp4".into();
    let next = resolved.clone();
    let lease = service.register_remote(resolved, Arc::new(move || Ok(next.clone())))?;
    let client = reqwest::blocking::Client::new();
    let response = client.head(&lease.url).send()?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "video/x-matroska");
    assert!(response.bytes()?.is_empty());
    let response = client.get(&lease.url).send()?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "video/x-matroska");
    assert_eq!(response.bytes()?.as_ref(), fixture.bytes.as_slice());
    service.release(&lease.id)?;
    Ok(())
}

#[test]
fn media_diagnostics_exclude_signed_urls_identities_and_session_tokens()
-> Result<(), Box<dyn std::error::Error>> {
    if !crate::providers::test_logging::isolated(
        module_path!(),
        "media_diagnostics_exclude_signed_urls_identities_and_session_tokens",
    ) {
        return Ok(());
    }

    use crate::providers::test_logging::{capture, excludes};
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start(1024, 3)?; // HTTP 200 instead of required 206
    let service = service(root.path(), 4096);
    let mut resolved = fixture.resolved();
    let mut url = resolved.url.as_url().clone();
    url.set_path("/opaque-media-secret");
    url.set_query(Some("signature=signed-query-secret"));
    resolved.url = SensitiveUrl::for_test(url.clone());
    resolved.identity.file_path = "filename-secret.mp4".into();
    let key = storage::key(&resolved.identity);
    let asset = storage::create(&service.shared.root, &key, resolved.identity.clone())?;
    let renewal = resolved.clone();
    let signed_url = url.as_str().to_owned();
    let (result, output) = capture(|| {
        let error = reqwest::blocking::Client::new()
            .get("invalid-url")
            .build()
            .err()
            .ok_or("expected request error")?
            .with_url(url);
        assert_eq!(request_error(error), MediaError::Upstream);
        let result = storage::acquire(
            &service.shared,
            &asset,
            resolved,
            Arc::new(move || Ok(renewal.clone())),
        );
        let original = root.path().join("local-filename-secret.mp4");
        std::fs::write(&original, b"fixture")?;
        let session = service.register_local(&original)?;
        service.release(&session.id)?;
        Ok::<_, Box<dyn std::error::Error>>((result, session.id))
    });
    let (result, token) = result?;
    assert_eq!(result, Err(MediaError::UnsupportedRange));
    assert!(output.contains("range_status"));
    assert!(output.contains("Media session created"));
    if diagnostics::enabled() {
        assert!(output.contains("Media operation ended"));
        assert!(output.contains("Some(UnsupportedRange)"));
    }
    excludes(
        &output,
        &[
            "opaque-media-secret",
            "signed-query-secret",
            "signature=",
            "filename-secret",
            "local-filename-secret",
            &signed_url,
            &key,
            &root.path().to_string_lossy(),
            &token,
        ],
    );
    Ok(())
}

#[test]
fn remote_acquisition_adapts_ranges_with_four_mib_ceiling() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    let size = CHUNK * 8 + 123;
    // Range sizes adapt to measured reads. Assert exact coverage and the size
    // ceiling, not a fixed ramp schedule or wall-clock throughput.
    let fixture = Fixture::start(size as usize, 0)?;
    let service = service(root.path(), CHUNK * 10);
    let resolved = fixture.resolved();
    let renewal = resolved.clone();
    let lease = service.register_remote(resolved, Arc::new(move || Ok(renewal.clone())))?;
    wait_until(|| service.usage().downloaded_bytes == size || !service.session_errors().is_empty());
    assert!(service.session_errors().is_empty());
    assert_eq!(service.usage().downloaded_bytes, size);
    let requests = fixture
        .requests
        .lock()
        .map_err(|_| "request lock poisoned")?;
    let mut ordered = requests.clone();
    ordered.sort_unstable();
    assert_eq!(ordered[0].0, 0);
    assert_eq!(ordered.last().ok_or("missing requests")?.1, size);
    assert!(ordered.windows(2).all(|pair| pair[0].1 == pair[1].0));
    assert!(
        ordered
            .iter()
            .all(|&(start, end)| start < end && end - start <= CHUNK)
    );
    assert!(ordered.len() >= size.div_ceil(CHUNK) as usize);
    drop(requests);
    // Read the entire durable result through the same route players use.
    let bytes = reqwest::blocking::get(&lease.url)?.bytes()?;
    assert_eq!(bytes.as_ref(), fixture.bytes.as_slice());
    service.release(&lease.id)?;
    Ok(())
}

// The headers and first 64KiB arriving upstream do not imply a durable range.
// Gate both phases independently; no real provider or wall-time latency oracle.
#[test]
fn slow_headers_and_partial_body_keep_reader_waiting_until_durable()
-> Result<(), Box<dyn std::error::Error>> {
    for (mode, stage) in [(13, 1), (14, 2)] {
        let root = tempfile::tempdir()?;
        let fixture = Fixture::start(CHUNK as usize, mode)?;
        let service = service(root.path(), CHUNK * 2);
        let resolved = fixture.resolved();
        let next = resolved.clone();
        let lease = service.register_remote(resolved, Arc::new(move || Ok(next.clone())))?;
        wait_until(|| fixture.stage.load(Ordering::Acquire) == stage);
        let response = reqwest::blocking::Client::new()
            .get(&lease.url)
            .header("Range", "bytes=0-65535")
            .send()?;
        assert_eq!(response.status(), 206);
        let (sent, received) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let _ = sent.send(response.bytes().map(|b| b.to_vec()).map_err(|_| ()));
        });
        wait_until(|| {
            service.shared.state.lock().is_ok_and(|s| {
                s.assets
                    .values()
                    .any(|a| a.state.lock().is_ok_and(|s| s.priority == Some(0)))
            })
        });
        assert_eq!(service.usage().downloaded_bytes, 0);
        assert!(matches!(
            received.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        fixture.gate.store(true, Ordering::Release);
        let bytes = received
            .recv_timeout(Duration::from_secs(8))?
            .map_err(|_| "body failed")?;
        assert_eq!(bytes, fixture.bytes[..65536]);
        reader.join().map_err(|_| "reader panicked")?;
        service.release(&lease.id)?;
    }
    Ok(())
}

#[test]
fn unavailable_range_reader_aborts_on_release_or_upstream_failure()
-> Result<(), Box<dyn std::error::Error>> {
    for (cancel, mode, stage) in [(false, 13, 1), (true, 13, 1), (true, 14, 2)] {
        let root = tempfile::tempdir()?;
        let fixture = Fixture::start(CHUNK as usize, mode)?;
        let service = service(root.path(), CHUNK * 2);
        let resolved = fixture.resolved();
        let next = resolved.clone();
        let lease = service.register_remote(resolved, Arc::new(move || Ok(next.clone())))?;
        wait_until(|| fixture.stage.load(Ordering::Acquire) == stage);
        let response = reqwest::blocking::Client::new()
            .get(&lease.url)
            .header("Range", "bytes=0-65535")
            .send()?;
        let (sent, received) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let _ = sent.send(response.bytes().is_err());
        });
        wait_until(|| {
            service.shared.state.lock().is_ok_and(|s| {
                s.assets
                    .values()
                    .any(|a| a.state.lock().is_ok_and(|s| s.priority == Some(0)))
            })
        });
        if cancel {
            service.release(&lease.id)?;
            // Reader cancellation does not wait for the blocked upstream header.
            assert!(received.recv_timeout(Duration::from_secs(3))?);
            fixture.gate.store(true, Ordering::Release);
        } else {
            fixture.mode.store(3, Ordering::Relaxed); // unsupported HTTP 200
            fixture.gate.store(true, Ordering::Release);
            assert!(received.recv_timeout(Duration::from_secs(3))?);
            assert_eq!(service.session_errors()[0].1, MediaError::UnsupportedRange);
            service.release(&lease.id)?;
        }
        reader.join().map_err(|_| "reader panicked")?;
        wait_until(|| {
            service.shared.state.lock().is_ok_and(|s| {
                s.assets
                    .values()
                    .all(|a| !a.running.load(Ordering::Acquire))
            })
        });
        assert_eq!(service.usage().downloaded_bytes, 0);
    }
    Ok(())
}
