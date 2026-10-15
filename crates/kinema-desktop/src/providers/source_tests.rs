use super::*;
use crate::providers::tests::fixture;
use serde_json::json;
type TestResult = Result<(), Box<dyn std::error::Error>>;
const HASH: &str = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
#[test]
fn public_indexers_use_movie_and_episode_routes() -> TestResult {
    for indexer in [IndexerKind::Torrentio, IndexerKind::Peerflix] {
        let (base, server) = fixture(vec![
            (200, json!({"streams":[]})),
            (200, json!({"streams":[]})),
        ])?;
        let mut c = IndexerClient::new()?;
        c.torrentio = base.clone();
        c.peerflix = base;
        let id = kinema_core::catalog::ImdbId::new("tt0133093")?;
        assert!(
            c.fetch(
                indexer,
                &PlaybackTarget::Movie(id.clone()),
                &AtomicBool::new(false)
            )?
            .candidates
            .is_empty()
        );
        assert!(
            c.fetch(
                indexer,
                &PlaybackTarget::Episode {
                    series: id,
                    season: 2,
                    episode: 3
                },
                &AtomicBool::new(false)
            )?
            .candidates
            .is_empty()
        );
        let requests = server.join().map_err(|_| "fixture failed")?;
        assert!(requests[0].starts_with("GET /stream/movie/tt0133093.json "));
        assert!(requests[1].starts_with("GET /stream/series/tt0133093:2:3.json "));
        assert!(
            requests
                .iter()
                .all(|r| !r.to_ascii_lowercase().contains("authorization"))
        );
    }
    Ok(())
}
#[test]
fn structured_magnets_dedupe_and_unsupported_count() -> TestResult {
    let result = parse(&json!({"streams":[
        {"infoHash":HASH,"fileIdx":1,"name":"Indexer 1080p","seeders":12,"behaviorHints":{"videoSize":123,"filename":"episode.mkv"}},
        {"url":format!("magnet:?xt=urn:btih:{HASH}&dn=ignored"),"fileIdx":1},
        {"url":"https://authorized.invalid/private-token"},
        {"infoHash":HASH,"fileIdx":2,"name":"Other file"}
    ]}))?;
    assert_eq!(result.candidates.len(), 2);
    assert_eq!(result.omitted, 2);
    assert_eq!(
        result.candidates[0].torrent.hash.as_str(),
        HASH.to_ascii_lowercase()
    );
    assert_eq!(result.candidates[0].size, Some(123));
    assert_eq!(result.candidates[0].seeders, Some(12));
    assert!(!format!("{result:?}").contains("private-token"));
    Ok(())
}
#[test]
fn malformed_is_not_successful_empty() {
    for value in [
        json!({}),
        json!({"streams":null}),
        json!({"streams":[{"infoHash":"not-a-hash"}]}),
        json!({"streams":[{}]}),
        json!({"streams":[{"infoHash":HASH,"fileIdx":-1}]}),
        json!({"streams":[{"url":"magnet:?xt=urn:btih:invalid"}]}),
    ] {
        assert_eq!(parse(&value), Err(SourceError::Malformed));
    }
    assert_eq!(
        parse(&json!({"streams":vec![json!({});MAX_RESULTS+1]})),
        Err(SourceError::TooLarge)
    );
}
#[test]
fn status_errors_and_bad_json_are_safe() -> TestResult {
    for (status, expected) in [
        (403, SourceError::Service),
        (429, SourceError::RateLimited),
        (500, SourceError::Service),
        (200, SourceError::Malformed),
    ] {
        let (base, server) = fixture(vec![(status, json!({"error":"private-token"}))])?;
        let mut c = IndexerClient::new()?;
        c.peerflix = base;
        let target = PlaybackTarget::Movie(kinema_core::catalog::ImdbId::new("tt0133093")?);
        let error = c
            .fetch(IndexerKind::Peerflix, &target, &AtomicBool::new(false))
            .err()
            .ok_or("expected failure")?;
        assert_eq!(error, expected);
        assert!(!format!("{error:?} {error}").contains("private-token"));
        server.join().map_err(|_| "fixture failed")?;
    }
    Ok(())
}

#[test]
fn realistic_addon_labels_preserve_release_identity_and_quality() -> TestResult {
    let result = parse(&json!({"streams":[
        {"infoHash":HASH,"fileIdx":4,"name":"Torrentio\n1080p","description":"Release.Movie.2026.WEB-DL.x265\n👤 99 💾 2 GB\nhttps://private.invalid/signed?token=secret","sizebytes":2147483648_u64,"behaviorHints":{"filename":"Release.Movie.2026.mkv"}},
        {"infoHash":HASH,"fileIdx":5,"name":"Peerflix 720p","title":"Serie.S02E03.SPANISH.WEBRip\nhttps://private.invalid/token","sizebytes":123456789},
        {"infoHash":HASH,"fileIdx":6,"name":"Peerflix 2160p","description":"   ","title":"Fallback.Title"},
        {"infoHash":null,"externalUrl":"https://private.invalid/authorized?token=secret"}
    ]}))?;
    assert_eq!(result.candidates.len(), 3);
    assert_eq!(result.omitted, 1);
    assert!(
        result.candidates[0]
            .name
            .starts_with("Release.Movie.2026.WEB-DL.x265")
    );
    assert_eq!(result.candidates[0].quality.as_deref(), Some("1080p"));
    assert_eq!(result.candidates[0].size, Some(2147483648));
    assert_eq!(result.candidates[0].seeders, Some(99));
    assert_eq!(result.candidates[0].torrent.file_index, Some(4));
    assert_eq!(
        result.candidates[0].torrent.filename.as_deref(),
        Some("Release.Movie.2026.mkv")
    );
    assert!(
        result.candidates[1]
            .name
            .starts_with("Serie.S02E03.SPANISH.WEBRip")
    );
    assert_eq!(result.candidates[1].quality.as_deref(), Some("720p"));
    assert_eq!(result.candidates[2].name, "Fallback.Title");
    assert_eq!(result.candidates[2].quality.as_deref(), Some("2160p"));
    let debug = format!("{result:?}");
    assert!(!debug.contains("private.invalid"));
    assert!(!debug.contains("token=secret"));
    Ok(())
}

#[test]
fn peerflix_structured_hints_override_approximate_labels() -> TestResult {
    let result = parse(&json!({"streams":[{
        "infoHash":HASH,"fileIdx":4,"name":"Peerflix 🇪🇸 4K",
        "description":"Matrix [4K UHDremux]\nMatrix.mkv\n👤 999 💾 56.19 GB 🌐 Peerflix",
        "seed":14,"sizebytes":60333553090_u64,"quality":"4K"
    }]}))?;
    let row = &result.candidates[0];
    assert_eq!(row.seeders, Some(14));
    assert_eq!(row.size, Some(60333553090));
    assert_eq!(row.quality.as_deref(), Some("2160p"));
    assert_eq!(row.torrent.file_index, Some(4));
    assert_eq!(row.torrent.filename, None);
    Ok(())
}

#[test]
fn emoji_hints_are_bounded_checked_and_unambiguous() {
    assert_eq!(
        label_hints("Release\n👤331 💾18.45 GB ⚙️ Source"),
        (Some(18_450_000_000), Some(331))
    );
    assert_eq!(
        label_hints("👤 0 💾 1.5 GiB"),
        (Some(1_610_612_736), Some(0))
    );
    for text in [
        "👤 -1 💾 -2 GB",
        "👤 1.5 💾 NaN GB",
        "👤 4294967296 💾 18446744073709551615 TB",
        "👤 1 👤 2 💾 1 GB 💾 2 GB",
        "👤 1,000 💾 1,000 GB",
        "💾 1.0001 GB",
        "💾 1. GB",
        "💾 1 gb",
        "💾 0 GB",
    ] {
        assert_eq!(label_hints(text), (None, None));
    }
    let text = format!("{}👤 12 💾 4 GB", "x".repeat(2048));
    assert_eq!(label_hints(&text), (None, None));
    assert_eq!(
        label_hints("Release 1080p 331 seeders 18.45 GB"),
        (None, None)
    );
}

#[test]
#[ignore = "opt-in: performs bounded requests to the two public noncredentialed addon endpoints"]
fn public_indexers_live_opt_in() -> TestResult {
    let client = IndexerClient::new()?;
    let target = PlaybackTarget::Movie(kinema_core::catalog::ImdbId::new("tt0133093")?);
    for indexer in [IndexerKind::Torrentio, IndexerKind::Peerflix] {
        let result = client.fetch(indexer, &target, &AtomicBool::new(false))?;
        println!(
            "{indexer:?}: candidates={} omitted={} size_hints={} seed_hints={}",
            result.candidates.len(),
            result.omitted,
            result
                .candidates
                .iter()
                .filter(|c| c.size.is_some())
                .count(),
            result
                .candidates
                .iter()
                .filter(|c| c.seeders.is_some())
                .count()
        );
        assert!(!result.candidates.is_empty());
    }
    Ok(())
}

#[test]
fn canceled_indexer_fetch_never_starts_network() -> TestResult {
    let client = IndexerClient::new()?;
    let target = PlaybackTarget::Movie(kinema_core::catalog::ImdbId::new("tt0133093")?);
    for indexer in [IndexerKind::Torrentio, IndexerKind::Peerflix] {
        assert_eq!(
            client.fetch(indexer, &target, &AtomicBool::new(true)),
            Err(SourceError::Canceled)
        );
    }
    Ok(())
}

#[test]
fn indexer_canceled_during_http_does_not_parse_returned_body() -> TestResult {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, atomic::Ordering},
        thread,
        time::Duration,
    };
    for indexer in [IndexerKind::Torrentio, IndexerKind::Peerflix] {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let base = Url::parse(&format!("http://{}/", listener.local_addr()?))?;
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancel);
        let server = thread::spawn(move || -> std::io::Result<()> {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut request = [0; 4096];
            let _ = stream.read(&mut request)?;
            signal.store(true, Ordering::Relaxed);
            // Would yield Malformed if obsolete work were parsed instead of canceled.
            let body = "{}";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
            Ok(())
        });
        let mut client = IndexerClient::new()?;
        client.torrentio = base.clone();
        client.peerflix = base;
        let target = PlaybackTarget::Movie(kinema_core::catalog::ImdbId::new("tt0133093")?);
        assert_eq!(
            client.fetch(indexer, &target, &cancel),
            Err(SourceError::Canceled)
        );
        server.join().map_err(|_| "fixture failed")??;
    }
    Ok(())
}
