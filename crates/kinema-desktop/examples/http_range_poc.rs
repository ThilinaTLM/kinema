//! Isolated, synthetic HTTP range experiment. No production integration.
//! Run: cargo run -p kinema-desktop --example http_range_poc --locked
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, RANGE};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const CHUNK: u64 = 64 * 1024;
const SLOTS: usize = 8;
const WORKERS: usize = 4;
const TICK: Duration = Duration::from_millis(5);
const SAMPLE: Duration = Duration::from_millis(80);
const IDENTITY: &str = "\"fixture-v1\"";

fn byte_at(offset: u64) -> u8 {
    ((offset.wrapping_mul(31) ^ (offset >> 8)) & 255) as u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    None,
    Flaky,
    WrongRange,
    WrongTotal,
    WrongIdentity,
    IgnoreRange,
    Short,
    Oversized,
}

/// The fixture generates only requested bytes, never allocates the logical file.
struct Fixture {
    url: String,
    listener: Option<JoinHandle<()>>,
}

impl Fixture {
    fn start(total: u64, latency: Duration, fault: Fault) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let address = listener.local_addr()?;
        let handle = thread::spawn(move || {
            let mut clients = Vec::new();
            let attempts = Arc::new(Mutex::new(BTreeMap::<u64, usize>::new()));
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                // A wake connection sends an explicit stop request, avoiding polling.
                let Ok(copy) = stream.try_clone() else { break };
                let mut reader = BufReader::new(copy);
                let _ = reader
                    .get_mut()
                    .set_read_timeout(Some(Duration::from_secs(2)));
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    continue;
                }
                if first.starts_with("STOP ") {
                    break;
                }
                let attempts = Arc::clone(&attempts);
                clients.push(thread::spawn(move || {
                    let _ = serve(stream, reader, total, latency, fault, attempts);
                }));
                // Reap finished fixture threads; metadata stays bounded too.
                let mut index = 0;
                while index < clients.len() {
                    if clients[index].is_finished() {
                        let _ = clients.swap_remove(index).join();
                    } else {
                        index += 1;
                    }
                }
            }
            for client in clients {
                let _ = client.join();
            }
        });
        Ok(Self {
            url: format!("http://{address}/fixture"),
            listener: Some(handle),
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(address) = self
            .url
            .strip_prefix("http://")
            .and_then(|s| s.split('/').next())
            && let Ok(mut stream) = TcpStream::connect(address)
        {
            let _ = stream.write_all(b"STOP / HTTP/1.1\r\n\r\n");
        }
        if let Some(handle) = self.listener.take() {
            let _ = handle.join();
        }
    }
}

fn serve(
    mut stream: TcpStream,
    mut reader: BufReader<TcpStream>,
    total: u64,
    latency: Duration,
    fault: Fault,
    attempts: Arc<Mutex<BTreeMap<u64, usize>>>,
) -> Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut range = None;
    let mut header_bytes = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        header_bytes += line.len();
        if header_bytes > 8192 {
            return Err("fixture header limit".into());
        }
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range = Some(value.trim().to_owned());
        }
    }
    let (start, end) = range
        .as_deref()
        .and_then(|s| s.strip_prefix("bytes="))
        .and_then(|s| s.split_once('-'))
        .ok_or("missing range")?;
    let start: u64 = start.parse()?;
    let end: u64 = end.parse()?;
    if start > end || end >= total || end - start >= CHUNK {
        stream.write_all(
            b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        return Ok(());
    }
    let attempt = {
        let mut map = attempts.lock().map_err(|_| "fixture poisoned")?;
        // Only active scenario ranges are tracked; this fixture is not a service.
        let count = map.entry(start).or_default();
        *count += 1;
        *count
    };
    thread::sleep(latency);
    if fault == Fault::Flaky && attempt == 1 && (start / CHUNK).is_multiple_of(3) {
        stream.write_all(
            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )?;
        return Ok(());
    }
    let size = end - start + 1;
    let declared_size = size + u64::from(fault == Fault::Oversized);
    let status = if fault == Fault::IgnoreRange {
        200
    } else {
        206
    };
    let range_start = start + u64::from(fault == Fault::WrongRange);
    let range_total = total + u64::from(fault == Fault::WrongTotal);
    let identity = if fault == Fault::WrongIdentity {
        "\"v2\""
    } else {
        IDENTITY
    };
    write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Range: bytes {range_start}-{end}/{range_total}\r\nContent-Length: {declared_size}\r\nETag: {identity}\r\nConnection: close\r\n\r\n"
    )?;
    let sent = if fault == Fault::Short || (fault == Fault::Flaky && attempt == 1) {
        size / 2
    } else {
        size
    };
    // Fixed 4 KiB scratch buffer; pace a connection to roughly 4 MiB/s.
    let mut buffer = [0; 4096];
    let mut offset = 0;
    while offset < sent {
        let n = (sent - offset).min(buffer.len() as u64) as usize;
        for (i, byte) in buffer[..n].iter_mut().enumerate() {
            *byte = byte_at(start + offset + i as u64);
        }
        stream.write_all(&buffer[..n])?;
        offset += n as u64;
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Job {
    start: u64,
    end: u64,
    total: u64,
    generation: u64,
    attempt: u32,
}

#[derive(Debug, PartialEq)]
enum Failure {
    Cancelled,
    Transport,
    RetryableStatus,
    InvalidRange,
}

fn fetch(
    client: &Client,
    url: &str,
    job: &Job,
    generation: &AtomicU64,
) -> std::result::Result<Vec<u8>, Failure> {
    let current = || generation.load(Ordering::Acquire) == job.generation;
    if !current() {
        return Err(Failure::Cancelled);
    }
    let mut response = client
        .get(url)
        .header(RANGE, format!("bytes={}-{}", job.start, job.end))
        .header(IF_RANGE, IDENTITY)
        .header("Accept-Encoding", "identity")
        .send()
        .map_err(|_| Failure::Transport)?;
    if response.status().is_server_error() || response.status().as_u16() == 429 {
        return Err(Failure::RetryableStatus);
    }
    let expected = format!("bytes {}-{}/{}", job.start, job.end, job.total);
    let size = job.end - job.start + 1;
    let headers = response.headers();
    if response.status().as_u16() != 206
        || headers.get(CONTENT_RANGE).and_then(|v| v.to_str().ok()) != Some(expected.as_str())
        || headers.get(ETAG).and_then(|v| v.to_str().ok()) != Some(IDENTITY)
        || headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            != Some(size)
        || headers.contains_key("content-encoding")
    {
        return Err(Failure::InvalidRange);
    }
    // Fixed allocation, bounded reads, no partial publication. EOF is required.
    let mut bytes = vec![0; size as usize];
    for part in bytes.chunks_mut(4096) {
        if !current() {
            return Err(Failure::Cancelled);
        }
        response.read_exact(part).map_err(|_| Failure::Transport)?;
    }
    let mut extra = [0];
    if response.read(&mut extra).map_err(|_| Failure::Transport)? != 0 {
        return Err(Failure::InvalidRange);
    }
    Ok(bytes)
}

struct Completion {
    job: Job,
    result: std::result::Result<Vec<u8>, Failure>,
    elapsed: Duration,
}

/// Pure controller: additive increase, multiplicative decrease, cooldown and
/// three healthy samples before shrinking. Throughput is an aggregate EWMA.
struct Controller {
    limit: usize,
    healthy: usize,
    cooldown: usize,
}

impl Controller {
    fn new() -> Self {
        Self {
            limit: 1,
            healthy: 0,
            cooldown: 0,
        }
    }
    fn sample(&mut self, ahead: u64, demand: f64, throughput: f64, errors: usize) {
        if errors > 0 {
            self.limit = self.limit.div_ceil(2);
            self.cooldown = 2;
            self.healthy = 0;
            return;
        }
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return;
        }
        if (ahead as f64) < demand * 0.15
            || ((ahead as f64) < demand * 0.4 && throughput < demand * 1.15)
        {
            self.limit = (self.limit + 1).min(WORKERS);
            self.healthy = 0;
        } else if (ahead as f64) > demand * 0.4 {
            self.healthy += 1;
            if self.healthy >= 3 {
                self.limit = self.limit.saturating_sub(1).max(1);
                self.healthy = 0;
                self.cooldown = 2;
            }
        } else {
            self.healthy = 0;
        }
    }
}

#[derive(Default, Debug)]
struct Metrics {
    downloaded: u64,
    consumed: u64,
    errors: usize,
    retries: usize,
    stale: usize,
    peak_payload: usize,
    peak_flights: usize,
    peak_ahead: u64,
    final_ahead: u64,
    min_limit: usize,
    max_limit: usize,
    changes: usize,
    stalls: usize,
    first_ready_ms: Option<u128>,
    seek_ready_ms: Option<u128>,
}

struct Retry {
    attempt: u32,
    after: Instant,
}

struct Cache {
    total: u64,
    cursor: u64,
    generation: Arc<AtomicU64>,
    ready: BTreeMap<u64, Vec<u8>>,
    flights: BTreeMap<(u64, u64), Job>,
    retries: BTreeMap<u64, Retry>,
    tx: Option<mpsc::SyncSender<Job>>,
    rx: mpsc::Receiver<Completion>,
    workers: Vec<JoinHandle<()>>,
    controller: Controller,
    metrics: Metrics,
    sample_bytes: u64,
    sample_errors: usize,
    throughput: f64,
    fatal: bool,
    // Kept only for verifying immediate post-seek dispatch; not an unbounded log.
    first_dispatch: Option<u64>,
}

impl Cache {
    fn new(url: String, total: u64) -> Result<Self> {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let (tx, jobs) = mpsc::sync_channel::<Job>(WORKERS);
        let jobs = Arc::new(Mutex::new(jobs));
        let (results, rx) = mpsc::sync_channel(WORKERS);
        let generation = Arc::new(AtomicU64::new(0));
        let mut workers = Vec::new();
        for _ in 0..WORKERS {
            let jobs = Arc::clone(&jobs);
            let results = results.clone();
            let client = client.clone();
            let url = url.clone();
            let generation = Arc::clone(&generation);
            workers.push(thread::spawn(move || {
                loop {
                    let job = match jobs.lock() {
                        Ok(receiver) => match receiver.recv() {
                            Ok(job) => job,
                            Err(_) => break,
                        },
                        Err(_) => break,
                    };
                    let now = Instant::now();
                    let result = fetch(&client, &url, &job, &generation);
                    if results
                        .send(Completion {
                            job,
                            result,
                            elapsed: now.elapsed(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        Ok(Self {
            total,
            cursor: 0,
            generation,
            ready: BTreeMap::new(),
            flights: BTreeMap::new(),
            retries: BTreeMap::new(),
            tx: Some(tx),
            rx,
            workers,
            controller: Controller::new(),
            metrics: Metrics {
                min_limit: 1,
                max_limit: 1,
                ..Metrics::default()
            },
            sample_bytes: 0,
            sample_errors: 0,
            throughput: 0.0,
            fatal: false,
            first_dispatch: None,
        })
    }

    fn seek(&mut self, cursor: u64) {
        self.cursor = cursor.min(self.total);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.ready.clear();
        self.retries.clear();
        self.first_dispatch = None;
        // Old requests retain their reservations until completion. No oversubscription.
    }

    fn ahead(&self) -> u64 {
        let mut end = self.cursor;
        while end < self.total {
            let start = end / CHUNK * CHUNK;
            let Some(bytes) = self.ready.get(&start) else {
                break;
            };
            let next = start + bytes.len() as u64;
            if next <= end {
                break;
            }
            end = next;
        }
        end - self.cursor
    }

    fn consume(&mut self, wanted: u64) -> Result<u64> {
        let count = wanted.min(self.ahead());
        let end = self.cursor + count;
        // Verify every exposed byte against the oracle, including unaligned seeks.
        for offset in self.cursor..end {
            let bytes = self
                .ready
                .get(&(offset / CHUNK * CHUNK))
                .ok_or("exposed hole")?;
            if bytes[(offset % CHUNK) as usize] != byte_at(offset) {
                return Err("corrupt byte exposed".into());
            }
        }
        self.cursor = end;
        self.ready
            .retain(|start, bytes| *start + bytes.len() as u64 > end);
        self.metrics.consumed += count;
        Ok(count)
    }

    fn schedule(&mut self, now: Instant) -> Result<()> {
        if self.fatal {
            return Ok(());
        }
        let generation = self.generation.load(Ordering::Acquire);
        let base = self.cursor / CHUNK * CHUNK;
        for index in 0..SLOTS {
            let start = base + index as u64 * CHUNK;
            if start >= self.total
                || self.flights.len() >= self.controller.limit
                || self.ready.len() + self.flights.len() >= SLOTS
            {
                break;
            }
            if self.ready.contains_key(&start) || self.flights.contains_key(&(generation, start)) {
                continue;
            }
            let attempt = if let Some(retry) = self.retries.get(&start) {
                if now < retry.after {
                    continue;
                }
                retry.attempt
            } else {
                0
            };
            let job = Job {
                start,
                end: (start + CHUNK).min(self.total) - 1,
                total: self.total,
                generation,
                attempt,
            };
            self.tx
                .as_ref()
                .ok_or("closed workers")?
                .send(job.clone())?;
            self.first_dispatch.get_or_insert(start);
            self.flights.insert((generation, start), job);
            self.metrics.peak_flights = self.metrics.peak_flights.max(self.flights.len());
        }
        // Includes buffers in workers and queued completions, not only published data.
        self.metrics.peak_payload = self
            .metrics
            .peak_payload
            .max((self.ready.len() + self.flights.len()) * CHUNK as usize);
        Ok(())
    }

    fn complete(&mut self, completion: Completion, now: Instant) {
        let Completion {
            job,
            result,
            elapsed,
        } = completion;
        self.flights.remove(&(job.generation, job.start));
        if job.generation != self.generation.load(Ordering::Acquire) {
            self.metrics.stale += 1;
            return;
        }
        match result {
            Ok(bytes) => {
                self.metrics.downloaded += bytes.len() as u64;
                self.sample_bytes += bytes.len() as u64;
                // Request elapsed is measured, but aggregate sample rate controls concurrency.
                debug_assert!(elapsed > Duration::ZERO);
                self.retries.remove(&job.start);
                self.ready.insert(job.start, bytes);
            }
            Err(Failure::Cancelled) => {
                self.metrics.stale += 1;
            }
            Err(failure) => {
                self.metrics.errors += 1;
                self.sample_errors += 1;
                if failure == Failure::InvalidRange || job.attempt >= 3 {
                    self.fatal = true;
                } else {
                    self.metrics.retries += 1;
                    self.retries.insert(
                        job.start,
                        Retry {
                            attempt: job.attempt + 1,
                            after: now + Duration::from_millis(20 * (1 << job.attempt)),
                        },
                    );
                }
            }
        }
    }

    fn sample(&mut self, elapsed: Duration, demand: f64) {
        let rate = self.sample_bytes as f64 / elapsed.as_secs_f64();
        self.throughput = self.throughput * 0.5 + rate * 0.5;
        let old = self.controller.limit;
        self.controller
            .sample(self.ahead(), demand, self.throughput, self.sample_errors);
        let limit = self.controller.limit;
        self.metrics.changes += usize::from(limit != old);
        self.metrics.min_limit = self.metrics.min_limit.min(limit);
        self.metrics.max_limit = self.metrics.max_limit.max(limit);
        self.sample_bytes = 0;
        self.sample_errors = 0;
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.tx.take();
        // At most WORKERS completions exist, so the bounded result channel cannot
        // deadlock shutdown. Blocking requests have a two-second timeout.
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn scenario(
    name: &str,
    demand: u64,
    latency_ms: u64,
    fault: Fault,
    seek: bool,
    adaptive: bool,
) -> Result<Metrics> {
    let total = if seek {
        1_u64 << 40
    } else {
        1024 * CHUNK + 137
    };
    let fixture = Fixture::start(total, Duration::from_millis(latency_ms), fault)?;
    let mut cache = Cache::new(fixture.url.clone(), total)?;
    let started = Instant::now();
    let mut last_tick = started;
    let mut last_sample = started;
    let mut sought = None;
    let mut credit = 0.0;
    let duration = Duration::from_millis(1000);
    while started.elapsed() < duration {
        let now = Instant::now();
        if seek && sought.is_none() && started.elapsed() >= Duration::from_millis(350) {
            let target = total - 64 * CHUNK + 123;
            cache.seek(target);
            sought = Some(now);
            credit = 0.0;
        }
        cache.schedule(now)?;
        match cache.rx.recv_timeout(TICK) {
            Ok(completion) => cache.complete(completion, Instant::now()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(error) => return Err(error.into()),
        }
        let now = Instant::now();
        cache.metrics.peak_ahead = cache.metrics.peak_ahead.max(cache.ahead());
        if cache.ahead() > 0 {
            cache
                .metrics
                .first_ready_ms
                .get_or_insert(started.elapsed().as_millis());
            if let Some(seek_time) = sought {
                cache
                    .metrics
                    .seek_ready_ms
                    .get_or_insert(seek_time.elapsed().as_millis());
            }
        }
        credit = (credit + demand as f64 * now.duration_since(last_tick).as_secs_f64())
            .min(CHUNK as f64);
        let wanted = credit as u64;
        let consumed = cache.consume(wanted)?;
        credit -= consumed as f64;
        if consumed < wanted && cache.cursor < total {
            cache.metrics.stalls += 1;
        }
        last_tick = now;
        if now.duration_since(last_sample) >= SAMPLE {
            if adaptive {
                cache.sample(now.duration_since(last_sample), demand as f64);
            }
            last_sample = now;
        }
        if cache.fatal {
            return Err(format!("{name}: fatal download error").into());
        }
    }
    if seek
        && (cache.metrics.seek_ready_ms.is_none()
            || cache.first_dispatch != Some((total - 64 * CHUNK + 123) / CHUNK * CHUNK))
    {
        return Err("seek failed to prioritize target".into());
    }
    if cache.metrics.consumed == 0
        || cache.metrics.peak_payload > SLOTS * CHUNK as usize
        || cache.metrics.peak_flights > WORKERS
    {
        return Err("scenario invariant failed".into());
    }
    cache.metrics.final_ahead = cache.ahead();
    let mode = if adaptive { "adaptive" } else { "fixed-1" };
    println!(
        "{name}/{mode}: demand={:.2}MiB/s elapsed={}ms valid_download={}KiB consumed={}KiB first={:?}ms seek={:?}ms stalled_ticks={} errors={} retries={} stale={} concurrency={}..{} changes={} peak_payload={}KiB peak_requests={} contiguous_peak={}KiB contiguous_final={}KiB",
        demand as f64 / 1048576.0,
        started.elapsed().as_millis(),
        cache.metrics.downloaded / 1024,
        cache.metrics.consumed / 1024,
        cache.metrics.first_ready_ms,
        cache.metrics.seek_ready_ms,
        cache.metrics.stalls,
        cache.metrics.errors,
        cache.metrics.retries,
        cache.metrics.stale,
        cache.metrics.min_limit,
        cache.metrics.max_limit,
        cache.metrics.changes,
        cache.metrics.peak_payload / 1024,
        cache.metrics.peak_flights,
        cache.metrics.peak_ahead / 1024,
        cache.metrics.final_ahead / 1024
    );
    Ok(std::mem::take(&mut cache.metrics))
}

fn main() -> Result<()> {
    println!(
        "Synthetic HTTP range POC; 64KiB chunks, 512KiB reserved payload, <=4 requests; NOT production ready."
    );
    for adaptive in [false, true] {
        scenario("moderate", 512 * 1024, 15, Fault::None, false, adaptive)?;
        scenario(
            "high-bitrate",
            8 * 1024 * 1024,
            25,
            Fault::None,
            false,
            adaptive,
        )?;
        scenario(
            "latency-flaky-seek-1TiB",
            2 * 1024 * 1024,
            45,
            Fault::Flaky,
            true,
            adaptive,
        )?;
    }
    println!(
        "Stalled ticks are observations, not wall-time or playback qualification; payload bound excludes HTTP/TLS, allocator, stacks and fixture overhead."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_hysteresis_demand_throughput_and_errors() {
        let mut c = Controller::new();
        for _ in 0..10 {
            c.sample(0, 1_000_000.0, 0.0, 0);
        }
        assert_eq!(c.limit, WORKERS);
        c.sample(0, 1_000_000.0, 0.0, 1);
        assert_eq!(c.limit, 2);
        for _ in 0..2 {
            c.sample(0, 1_000_000.0, 0.0, 0);
        }
        assert_eq!(c.limit, 2);
        c.sample(200_000, 1_000_000.0, 2_000_000.0, 0);
        assert_eq!(c.limit, 2);
        c.sample(200_000, 1_000_000.0, 100_000.0, 0);
        assert_eq!(c.limit, 3);
        for _ in 0..2 {
            c.sample(500_000, 1_000_000.0, 2_000_000.0, 0);
        }
        assert_eq!(c.limit, 3);
        c.sample(500_000, 1_000_000.0, 2_000_000.0, 0);
        assert_eq!(c.limit, 2);
    }

    #[test]
    fn holes_seek_generations_and_memory_reservations() -> Result<()> {
        let mut cache = Cache::new("http://127.0.0.1:1/unused".into(), 1 << 40)?;
        let make = |start, generation| Completion {
            job: Job {
                start,
                end: start + CHUNK - 1,
                total: 1 << 40,
                generation,
                attempt: 0,
            },
            result: Ok((start..start + CHUNK).map(byte_at).collect()),
            elapsed: TICK,
        };
        cache.complete(make(CHUNK, 0), Instant::now());
        assert_eq!(cache.ahead(), 0);
        assert_eq!(cache.consume(10)?, 0);
        cache.complete(make(0, 0), Instant::now());
        assert_eq!(cache.ahead(), 2 * CHUNK);
        assert_eq!(cache.consume(CHUNK + 123)?, CHUNK + 123);
        assert_eq!(cache.ahead(), CHUNK - 123);
        cache.seek((1 << 40) - CHUNK + 99);
        cache.complete(make(2 * CHUNK, 0), Instant::now());
        assert!(cache.ready.is_empty());
        assert_eq!(cache.metrics.stale, 1);
        cache.controller.limit = WORKERS;
        cache.schedule(Instant::now())?;
        assert_eq!(cache.first_dispatch, Some((1 << 40) - CHUNK));
        assert!(cache.metrics.peak_payload <= SLOTS * CHUNK as usize);
        Ok(())
    }

    #[test]
    fn real_seek_keeps_old_reservations_and_rejects_stale_results() -> Result<()> {
        let total = 1_u64 << 40;
        let fixture = Fixture::start(total, Duration::from_millis(25), Fault::None)?;
        let mut cache = Cache::new(fixture.url.clone(), total)?;
        cache.controller.limit = WORKERS;
        cache.schedule(Instant::now())?;
        assert_eq!(cache.flights.len(), WORKERS);
        let target = total - 8 * CHUNK + 123;
        cache.seek(target);
        cache.schedule(Instant::now())?;
        assert_eq!(cache.flights.len(), WORKERS);
        assert_eq!(cache.first_dispatch, None);
        let deadline = Instant::now() + Duration::from_secs(3);
        while cache.ahead() == 0 {
            if Instant::now() >= deadline {
                return Err("seek readiness deadline".into());
            }
            let completion = cache.rx.recv_timeout(Duration::from_secs(2))?;
            cache.complete(completion, Instant::now());
            cache.schedule(Instant::now())?;
            assert!(cache.flights.len() <= WORKERS);
            assert!(cache.ready.len() + cache.flights.len() <= SLOTS);
            assert!(
                cache
                    .ready
                    .keys()
                    .all(|start| *start >= target / CHUNK * CHUNK)
            );
        }
        assert_eq!(cache.first_dispatch, Some(target / CHUNK * CHUNK));
        assert!(cache.metrics.stale > 0);
        assert!(cache.consume(1000)? > 0);
        Ok(())
    }

    #[test]
    fn rejects_status_range_total_identity_length_and_truncation() -> Result<()> {
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()?;
        let generation = AtomicU64::new(0);
        let job = Job {
            start: 0,
            end: CHUNK - 1,
            total: 2 * CHUNK,
            generation: 0,
            attempt: 0,
        };
        for fault in [
            Fault::WrongRange,
            Fault::WrongTotal,
            Fault::WrongIdentity,
            Fault::IgnoreRange,
            Fault::Short,
            Fault::Oversized,
        ] {
            let fixture = Fixture::start(job.total, Duration::ZERO, fault)?;
            let result = fetch(&client, &fixture.url, &job, &generation);
            assert!(result.is_err(), "accepted {fault:?}");
        }
        let fixture = Fixture::start(job.total, Duration::ZERO, Fault::None)?;
        let tail = Job {
            start: CHUNK + 123,
            end: 2 * CHUNK - 1,
            ..job
        };
        let bytes =
            fetch(&client, &fixture.url, &tail, &generation).map_err(|e| format!("{e:?}"))?;
        assert_eq!(bytes.len() as u64, CHUNK - 123);
        assert!(
            bytes
                .iter()
                .enumerate()
                .all(|(i, b)| *b == byte_at(tail.start + i as u64))
        );
        Ok(())
    }

    #[test]
    fn retry_backoff_exhaustion_and_permanent_failure() -> Result<()> {
        let mut cache = Cache::new("http://127.0.0.1:1/unused".into(), CHUNK)?;
        let now = Instant::now();
        for attempt in 0..4 {
            let job = Job {
                start: 0,
                end: CHUNK - 1,
                total: CHUNK,
                generation: 0,
                attempt,
            };
            cache.complete(
                Completion {
                    job,
                    result: Err(Failure::Transport),
                    elapsed: TICK,
                },
                now,
            );
            if attempt < 3 {
                assert_eq!(
                    cache.retries[&0].after.duration_since(now),
                    Duration::from_millis(20 * (1 << attempt))
                );
                cache.schedule(now)?;
                assert!(cache.flights.is_empty(), "retried before backoff");
                assert!(!cache.fatal);
            }
        }
        assert!(cache.fatal);
        assert_eq!(cache.metrics.retries, 3);
        cache.fatal = false;
        let job = Job {
            start: 0,
            end: CHUNK - 1,
            total: CHUNK,
            generation: 0,
            attempt: 0,
        };
        cache.complete(
            Completion {
                job,
                result: Err(Failure::InvalidRange),
                elapsed: TICK,
            },
            now,
        );
        assert!(cache.fatal);
        Ok(())
    }

    #[test]
    fn real_http_scenario_suite() -> Result<()> {
        let moderate = scenario("test-moderate", 512 * 1024, 15, Fault::None, false, true)?;
        assert!(moderate.consumed > 0);
        let high = scenario("test-high", 8 * 1024 * 1024, 25, Fault::None, false, true)?;
        assert!(high.max_limit > 1);
        let flaky = scenario(
            "test-flaky-seek",
            2 * 1024 * 1024,
            45,
            Fault::Flaky,
            true,
            true,
        )?;
        assert!(flaky.errors > 0 && flaky.retries > 0);
        assert!(flaky.seek_ready_ms.is_some());
        Ok(())
    }
}
