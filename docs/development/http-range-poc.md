# Isolated HTTP parallel-range cache POC

`crates/kinema-desktop/examples/http_range_poc.rs` is a runnable, synthetic
experiment, **not production-ready** and not connected to acquisition, MPV, GPUI,
or the production cache. No Cargo/dependency or production changes are required
by this example. Commands run from the repository root with Rust 1.97.1:

```sh
cargo run -p kinema-desktop --example http_range_poc --locked
cargo test -p kinema-desktop --example http_range_poc --locked -- --nocapture
cargo clippy -p kinema-desktop --example http_range_poc --locked -- -D warnings
cargo fmt --all -- --check
```

The executable runs six approximately one-second scenarios: fixed concurrency 1
and adaptive concurrency for each of moderate demand (0.5 MiB/s), high demand
(8 MiB/s = approximately 67 Mbit/s), and latency/flaky/seek (2 MiB/s). Each run
starts a fresh loopback server with the same deterministic byte oracle and fault
rules. No external service, media, credentials, GUI session, or large file is
needed. Desktop native build prerequisites still apply; see `../development.md`.

## What it demonstrates

- **Bounded payload:** 64 KiB chunks, eight shared reservations (512 KiB), and
  four owned blocking workers. Ready chunks, active requests, queued jobs and
  queued completions share reservations. A seek clears ready data, but old
  requests keep their reservation until completion. Retried ranges stay within
  the eight-chunk lookahead window; no bitmap/vector scales with logical length.
  Channel capacities are four. Worker handles are joined on shutdown.
- **Range readiness:** only fully read, validated chunks become visible. Contiguous
  readiness starts at the exact cursor and stops at the first missing chunk;
  out-of-order successes beyond a hole are not readable. Every consumed byte is
  checked against an offset-derived oracle. Consumed chunks are discarded.
- **Seek priority:** generation changes invalidate all obsolete completions and
  retry state. The first new dispatch is the chunk containing the target, then
  ascending offsets. Cancellation checks precede requests and occur between
  4 KiB reads. In-progress blocking I/O is not forcibly interrupted; a request
  timeout bounds that delay. No extra worker is created to bypass old requests.
- **Adaptive policy:** starts at one request, samples every ~80 ms, uses contiguous
  bytes ahead and aggregate successful-byte throughput EWMA (weight 0.5), and
  adds one request up to four when underbuffered. Below 150 ms of demand, buffer
  shortage alone triggers growth; below 400 ms, insufficient throughput also
  triggers growth. Three healthy samples above 400 ms are needed to shrink by
  one. Errors halve the target (rounding up); reductions impose two samples of
  cooldown. Already-running requests drain rather than being counted as new
  capacity. Fixed-1 runs bypass this policy but otherwise use the same scheduler.
- **Failures:** exponential per-range retry backoff of 20/40/80 ms, at most four
  attempts. Transport errors, 429 and 5xx are retryable. Invalid range responses
  are fatal, not silently accepted or retried forever. The fixture deterministically
  returns 503 on selected first attempts and truncates other first attempts.
- **Validation:** require 206, exact Content-Range start/end/known total, exact
  Content-Length, expected strong ETag, no Content-Encoding, complete body and
  EOF. Requests send Range, If-Range and Accept-Encoding: identity. Tests reject
  ignored ranges (200), wrong start/total/identity/length and truncated bodies;
  also exercise an unaligned final range, holes, stale generations, old-flight
  reservation accounting, backoff exhaustion and controller hysteresis.

The fixture adds 15/25/45 ms response latency and paces 4 KiB writes by 1 ms
(roughly 4 MiB/s per connection before latency/OS overhead). Fault locations and
bytes are deterministic; wall-clock timings and worker completion order are not.
These are per-connection limits, **not a shared bottleneck or realistic WAN**.
The fixture generates a 1 TiB logical resource without materializing it and seeks
near its end after ~350 ms; only the small requested windows are transferred.
Other scenarios advertise 64 MiB + 137 bytes without downloading the whole file.

## Metrics and local results

The executable prints per-run valid downloaded and consumed KiB, first contiguous
readiness and post-seek readiness latency, stalled consumer observations, retry/
error/stale counts, concurrency bounds/changes, reserved payload high-water mark,
and **peak/final contiguous bytes ahead**. Aggregate downloaded bytes are transfer
accounting, not a buffer-readiness metric. Downloads exclude failed partial bodies
and stale responses; they are not wire-byte billing. A stalled tick is an event
where requested consumption exceeded contiguous readiness; tick spacing varies,
so counts are neither exact stalled time nor a playback smoothness score. Demand
credit is capped to one chunk (no unbounded playback catch-up after starvation).
Byte-oracle verification also adds CPU work absent from a normal player.

### Recorded Linux loopback run — 2026-10-28

Rust 1.97.1, default desktop features, commands above with `--locked`:
executable **passed all six scenarios**; example tests **6 passed, 0 failed**;
scoped strict Clippy and root format check **passed**. Scoped whitespace diff
check also passed. Workspace-wide Clippy/tests were not rerun for this isolated
example. Other agents' Cargo/lockfile work was left untouched.

Each row covers ~1.00 s; units below are KiB except latency and request count.
Contiguous peak/final means actual readable bytes ahead, not downloaded totals.

| Scenario | Mode | Valid downloaded / consumed | Contiguous peak / final | Target requests | Stalled ticks | Seek ready ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Moderate | fixed-1 | 960 / 512 | 495 / 447 | 1 | 6 | — |
| Moderate | adaptive | 960 / 512 | 495 / 447 | 1 | 6 | — |
| High bitrate | fixed-1 | 1408 / 1408 | 64 / 0 | 1 | 180 | — |
| High bitrate | adaptive | 5056 / 4992 | 134 / 63 | 1–4 | 111 | — |
| Latency/flaky/1 TiB seek | fixed-1 | 384 / 383 | 64 / 0 | 1 | 200 | 189 |
| Latency/flaky/1 TiB seek | adaptive | 384 / 383 | 64 / 0 | 1 | 198 | 188 |

First readiness was 33 ms (moderate), 42–43 ms (high), and 163–164 ms
(flaky). Payload reservation peaks were 512 KiB for both moderate runs,
64/384 KiB for fixed/adaptive high bitrate, and 64 KiB for both flaky runs.
Only high adaptive reached four active requests, with three target changes.
Both flaky runs recorded 10 errors, 10 scheduled retries and one stale completion;
all other runs had none.

**Interpretation:** parallelism improves this fixture's high-demand transfer
(~4.88 MiB consumed versus ~1.38 MiB), but still fails to meet 8 MiB/s demand.
Moderate demand does not need extra concurrency. Frequent first-attempt faults
repeatedly reset the error cooldown, pinning adaptive to one request: no material
improvement over fixed-1. This is an exposed controller limitation, not evidence
of good recovery on real unreliable networks. Unit tests separately prove target
reduction/recovery and hysteresis. The 1 TiB runs transferred only 384 KiB of
validated data each, plus bounded failed/obsolete attempts; no full download was
performed. Timing differences between runs are expected, not regression limits.

## Deliberate limits / production implications

**A 1 TiB logical fixture is not evidence that production can play a file larger
than its cache quota.** Production currently reserves the full logical file against
quota and performs physical disk admission. This POC deliberately implements a
small, evicting **RAM** window instead. It neither tests nor changes those admission
rules. There is no sparse disk file here, no disk allocation experiment, no durable
cache, eviction across files, resume manifest, or filesystem free-space test.

The demo uses a **two-second whole-request deadline**, chosen only for bounded
local tests/shutdown. Production's existing **20-second whole-request deadline**
is problematic on slow links: a legitimately progressing large range can exceed
it. Merely raising the number or adding workers is not a demonstrated fix. A
production design needs separately considered connect/header deadlines, progress
or idle timeouts, cancellation and range sizing based on measured link behavior.
Blocking cancellation here can wait for the whole-request deadline; this is not
an async transport or proof of immediate seek cancellation.

Length and ETag are injected from the known fixture contract, not discovered via
HEAD/probing. There is no provider authentication, redirect/security policy,
validator refresh, multipart/chunked-range support, compression, Retry-After,
retry jitter, rate-limit coordination, global bandwidth cap or multiple readers.
The HTTP library owns framing, so EOF is application-body EOF, not an independent
raw-wire parser. The fixture is a controlled local test server, not hardened for
untrusted requests; its per-range attempt metadata is scoped to short scenarios.
The payload bound excludes reqwest/HTTP/TLS buffers, allocator overhead, OS socket
buffers, thread stacks, and the fixture's 4 KiB per-response scratch space. No RSS
bound is claimed. This is Linux loopback evidence only, not Windows/macOS, real
provider, decoder, playback, seek UX or production-readiness qualification.
