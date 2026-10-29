# Streaming acquisition POCs

These experiments are not wired into default playback. They use synthetic local
content, not user media, provider credentials, or public torrent swarms.

- [Adaptive HTTP range cache](http-range-poc.md)
- [Torrent streaming with librqbit](rqbit-poc.md)

## Initial findings — 2026-10-28

Both runnable POCs passed their scoped Linux tests. See the linked reports for
exact commands, measurements and caveats.

- HTTP: fixed-1 versus adaptive 1–4 delivered about 1.38 versus 4.88 MiB of
  consumed data in the one-second high-demand fixture. Neither sustained the
  configured 8 MiB/s consumer. At moderate demand, one request was sufficient.
- Frequent first-attempt faults kept the adaptive controller at one request.
  Blind multiplicative decrease on every error is not a validated smart recovery
  policy. The next experiment should distinguish origin-wide throttling from
  isolated failures, retain an occasional bounded recovery probe, and measure a
  shared bottleneck plus changing bitrate/network phases over longer runs.
- A generated 1 TiB HTTP resource required only a 512 KiB payload reservation
  budget; this is a bounded RAM-window experiment, **not** a disk-cache test.
- librqbit served independently verified tail/head/middle reads while the torrent
  was incomplete and kept missing data pending. The rate-limited seed's middle
  seek took about 7.7 seconds: API feasibility is established locally, not smooth
  seek performance. Inspect scheduling/readahead before choosing production defaults.
- Neither experiment uses actual video decoding or changes the existing player.

Recommendation: keep the current default path while extending HTTP measurements
and investigating the torrent seek delay. Preserve the production durable-cache
contract; do not transplant the RAM-window POC as a replacement without a separate
storage/quota/recovery design.

## Questions and acceptance gates

The performance target is contiguous playable data, not aggregate download MB/s.
Measure time to first readable bytes, seek-to-readable latency, stalls, contiguous
buffer duration, useful throughput, retry bytes, and peak connections/memory.
A synthetic byte consumer approximates bitrate; it does not establish decoder or
container-demux behavior, actual time to first frame, or variable-bitrate playback.

| Condition | Required behavior |
| --- | --- |
| Low bitrate, healthy source | Avoid unnecessary requests/connections once buffer is healthy |
| High bitrate, per-request latency/limits | Increase bounded parallelism only when it improves useful delivery |
| Source slower than playback | Report/rebuffer honestly; more connections cannot create bandwidth |
| Variable bitrate | Use a safety margin and conservative demand estimate; average bitrate is insufficient |
| Transient failure or throttling | Bounded retries, backoff/jitter, server Retry-After where available, global retry budget |
| Seek | Prioritize new playback window; reject obsolete completions using generations |
| Large file | Bound memory and in-flight work independently of file length; explicitly choose disk quota policy |
| Missing bytes | Wait or fail explicitly, never serve unwritten/preallocated holes |
| Changed URL/content | Renew once per generation; validate identity before sharing cached ranges |
| Stop/shutdown | Own and join workers; ensure blocked operations have deadlines/cancellation |

## Proposed production policy (not qualification)

Start conservatively, for example two in-flight ranges with a configurable ceiling
of four. Use a contiguous-buffer low/high watermark and throughput relative to
estimated consumption. Increase by one only after a sustained deficit; decrease
on repeated errors, throttling, or lack of useful throughput improvement. Apply a
cooldown/hysteresis so noisy measurements do not cause oscillation. Keep a global
budget across assets as well as a per-asset cap. Distinguish origin throttling from
a single connection failure and disk backpressure.

Give urgent reads priority, followed by a bounded forward window, then optional
background completion. Container tail/index requests must also be serviceable;
sequential-from-zero scheduling alone is insufficient. Do not infer useful buffer
from the total number of downloaded bytes spread across the file.

Separate validated-readable ranges from crash-durable resume checkpoints if disk
sync is batched. After restart, trust only durable checkpoints. Parallel writes
must not lose range-set updates, publish incomplete chunks, or share a mutable
file cursor without coordination.

The existing production cache reserves the full asset size. A multi-hundred-GB
logical-file experiment with bounded fetched data is **not** proof that production
can play a file larger than its cache quota. Sliding-window caching would be a
separate policy with reader pinning, eviction, re-fetch, and resume semantics.

## Torrent scope

Direct torrents must be explicit opt-in: peers see the user's network address and
the engine may upload content. A localhost seed proves API integration and piece
availability behavior, not public-swarm performance, NAT traversal, privacy, or
Windows/macOS qualification. Review DHT, trackers, LAN discovery, UPnP, listening
interfaces, upload limits, and seeding policy before exposing this to users.

## Validation

Baseline validation on 2026-10-28, before the POC implementation landed, passed:
`cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`, and
`cargo test --workspace --locked`.

Each linked report records its exact commands, observations, and limitations.
Final lead validation after both examples landed passed on 2026-10-28:

- Root formatting, strict locked all-target workspace Clippy, whitespace diff check.
- Locked workspace tests: 802 passed, 26 ignored.
- HTTP example: six tests passed and all six executable scenarios passed again.
- Opt-in torrent example: strict locked Clippy and four tests passed, including
  the real local swarm.

The repeat run consumed 1,408 versus 4,965 KiB for fixed/adaptive high-demand HTTP;
the torrent middle seek took 7,998 ms. These reproduce the broad findings, not
exact timing thresholds. No result here should be interpreted as live-provider
or native player qualification.
