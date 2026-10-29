//! Controlled localhost streaming/seek experiment. See docs/development/rqbit-poc.md.
#[path = "rqbit_poc/fixture.rs"]
mod fixture;

use anyhow::{Context, Result, ensure};
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerOptions, Session, SessionOptions,
    limits::LimitsConfig, spawn_utils::BlockingSpawner,
};
use std::{io::SeekFrom, net::Ipv4Addr, num::NonZeroU32, path::Path, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::time::{Instant, timeout};

const DEADLINE: Duration = Duration::from_secs(40);
const MISSING_WAIT: Duration = Duration::from_millis(200);

fn options(seed: bool) -> SessionOptions {
    SessionOptions {
        dht: None,
        disable_trackers: true,
        disable_local_service_discovery: true,
        persistence: None,
        fastresume: false,
        ipv4_only: true,
        listen: seed.then(|| ListenerOptions {
            listen_addr: (Ipv4Addr::LOCALHOST, 0).into(),
            enable_upnp_port_forwarding: false,
            ipv4_only: true,
            ..Default::default()
        }),
        peer_limit: Some(1),
        runtime_worker_threads: Some(4),
        // Keep the transfer incomplete long enough to observe distant seeks.
        ratelimits: LimitsConfig {
            upload_bps: seed.then(|| NonZeroU32::new(256 * 1024)).flatten(),
            download_bps: None,
        },
        ..Default::default()
    }
}

async fn experiment(root: &Path, sessions: &mut Vec<Arc<Session>>) -> Result<()> {
    let seed_dir = root.join("seed");
    let download_dir = root.join("download");
    std::fs::create_dir(&seed_dir)?;
    std::fs::create_dir(&download_dir)?;
    let source = seed_dir.join("fixture.bin");
    fixture::write(&source)?;
    let torrent = librqbit::create_torrent(
        &source,
        CreateTorrentOptions {
            piece_length: Some(fixture::PIECE),
            ..Default::default()
        },
        &BlockingSpawner::new(2),
    )
    .await?;
    let bytes = torrent.as_bytes()?;
    let seed = Session::new_with_opts(seed_dir.clone(), options(true)).await?;
    sessions.push(seed.clone());
    let seeding = seed
        .add_torrent(
            AddTorrent::from_bytes(bytes.clone()),
            Some(AddTorrentOptions {
                overwrite: true,
                disable_trackers: true,
                ..Default::default()
            }),
        )
        .await?
        .into_handle()
        .context("seed not added")?;
    seeding.wait_until_initialized().await?;
    ensure!(seeding.stats().finished, "seed did not hash-check complete");
    let address = seed.listen_addr().context("seed has no TCP listener")?;
    ensure!(address.ip().is_loopback(), "seed listener is not loopback");

    let download = Session::new_with_opts(download_dir.clone(), options(false)).await?;
    sessions.push(download.clone());
    let handle = download
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions {
                disable_trackers: true,
                paused: true,
                initial_peers: Some(vec![address]),
                ..Default::default()
            }),
        )
        .await?
        .into_handle()
        .context("download not added")?;
    handle.wait_until_initialized().await?;
    ensure!(handle.stats().progress_bytes == 0, "download was not empty");
    ensure!(
        handle.clone().stream(1).await.is_err(),
        "invalid file accepted"
    );

    // Paused before connecting: absent pieces must not expose preallocated zeroes.
    // Drop the timed-out reader entirely; do not reuse a partially read_exact buffer.
    let mut absent = handle.clone().stream(0).await?;
    let mut buffer = [0; fixture::WINDOW];
    ensure!(
        timeout(MISSING_WAIT, absent.read_exact(&mut buffer))
            .await
            .is_err(),
        "missing piece read unexpectedly completed"
    );
    drop(absent);
    println!("missing_piece: pending for 200ms; timed-out reader dropped");

    download.unpause(&handle).await?;
    handle.wait_until_initialized().await?;
    let mut stream = handle.clone().stream(0).await?;
    // Tail, head, middle, and cached head: random access during an incomplete transfer.
    for (label, offset) in [
        ("tail", fixture::LENGTH - fixture::WINDOW as u64),
        ("head", 0),
        ("middle", fixture::LENGTH / 2 + 123),
        ("head_again", 0),
    ] {
        let started = Instant::now();
        ensure!(stream.seek(SeekFrom::Start(offset)).await? == offset);
        stream.read_exact(&mut buffer).await?;
        ensure!(fixture::matches(offset, &buffer), "incorrect {label} bytes");
        let stats = handle.stats();
        ensure!(
            !stats.finished,
            "transfer completed before seek observation"
        );
        ensure!(stats.progress_bytes < fixture::LENGTH);
        println!(
            "seek={label} offset={offset} verified_bytes={} elapsed_ms={} downloaded={} total={}",
            buffer.len(),
            started.elapsed().as_millis(),
            stats.progress_bytes,
            stats.total_bytes
        );
    }
    ensure!(stream.seek(SeekFrom::End(0)).await? == fixture::LENGTH);
    ensure!(stream.read(&mut buffer).await? == 0, "EOF not reported");
    drop(stream);

    // Stop while the transfer is incomplete. stop() cancels the session tasks;
    // callers must also drop their stream/handle owners before removing files.
    let started = Instant::now();
    timeout(Duration::from_secs(3), download.stop())
        .await
        .context("stop deadline")?;
    println!(
        "cancel_incomplete: stop_ms={}",
        started.elapsed().as_millis()
    );
    drop(handle);
    drop(seeding);
    let seed_size = fixture::storage_bytes(&seed_dir)?;
    let download_size = fixture::storage_bytes(&download_dir)?;
    ensure!(seed_size == fixture::LENGTH);
    ensure!(download_size <= fixture::LENGTH);
    println!(
        "storage: seed_logical_bytes={seed_size} download_logical_bytes={download_size} bound_each={}",
        fixture::LENGTH
    );
    Ok(())
}

async fn run() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut sessions = Vec::new();
    let result = timeout(DEADLINE, experiment(directory.path(), &mut sessions)).await;
    // Cleanup also runs after experiment errors/timeouts. Explicitly stop and drop
    // session-owned file handles before TempDir cleanup (important on Windows).
    let mut cleanup_error = None;
    for session in &sessions {
        if timeout(Duration::from_secs(3), session.stop())
            .await
            .is_err()
        {
            cleanup_error = Some(anyhow::anyhow!("cleanup stop deadline"));
        }
    }
    drop(sessions);
    directory.close().context("temporary fixture cleanup")?;
    if let Some(error) = cleanup_error {
        return Err(error);
    }
    result.context("experiment exceeded 40s deadline")??;
    println!("PASS: isolated torrent bytes/seek experiment; not a playback qualification");
    Ok(())
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?
        .block_on(run())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_is_disabled() {
        for seed in [false, true] {
            let config = options(seed);
            assert!(config.dht.is_none());
            assert!(config.disable_trackers);
            assert!(config.disable_local_service_discovery);
            assert!(config.persistence.is_none());
            assert!(config.trackers.is_empty());
            if let Some(listener) = config.listen {
                assert!(listener.listen_addr.ip().is_loopback());
                assert!(!listener.enable_upnp_port_forwarding);
            }
        }
    }

    #[test]
    fn local_swarm() -> Result<()> {
        main()
    }
}
