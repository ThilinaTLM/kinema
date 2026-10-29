# Isolated librqbit streaming/seek POC

This is an **opt-in API experiment**, not a Kinema download backend or native
playback integration. No player, HTTP range proxy, media decoder, user settings,
credentials, or public torrent is involved.

## Run

From the repository root with Rust **1.97.1**, edition 2024:

```sh
cargo run -p kinema-desktop --example rqbit_poc --no-default-features --features rqbit-poc --locked
cargo test -p kinema-desktop --example rqbit_poc --no-default-features --features rqbit-poc --locked -- --nocapture
cargo clippy -p kinema-desktop --example rqbit_poc --no-default-features --features rqbit-poc --locked -- -D warnings
```

The example requires `rqbit-poc`; normal desktop defaults do not enable librqbit
or anyhow through this feature. `--no-default-features` avoids the optional GPU
surface dependencies for this experiment, but desktop's existing unconditional
GPUI/platform dependencies still compile. The native build prerequisites in
[development.md](../development.md#native-prerequisites) therefore still apply.
No separate workspace, registry edits, system torrent client or seed setup is
needed. Compilation can download crates; runtime needs **only loopback TCP**.

There are no CLI inputs: content, torrent metadata, destinations and peers are
constructed internally. Do not change this to accept arbitrary torrents without
revisiting isolation, storage limits and untrusted path handling.

## Isolation and API choices

Pinned `librqbit =9.0.1`, `default-features = false`, `rust-tls` (provides the
SHA-1 implementation required to hash torrent pieces, without OpenSSL's default
TLS backend). No HTTP API/server, web UI, database or UPnP-serving feature is
enabled. An intentional Cargo.lock update resolves the opt-in dependencies.
Versions are declared centrally in the root workspace. This adds 80 locked
packages (none removed). Several discovery/UPnP crates are unconditional upstream
dependencies despite being disabled at runtime; the footprint is not just the
single librqbit crate. No application code references these optional dependencies.

- `create_torrent` + `CreateTorrentOptions`: one generated `fixture.bin`, 8 MiB,
  64 KiB pieces, no tracker URLs. The deterministic absolute-offset byte oracle
  is unrelated to user media and is also used to verify every returned window.
- Two `Session::new_with_opts` instances, separate temporary seed/download dirs.
  DHT is `None`, trackers disabled at session **and** torrent level, local service
  discovery disabled, persistence/fastresume off, no custom trackers, blocklist
  or allowlist URL, no proxy. UPnP port forwarding is explicitly off.
- Only the seed listens: TCP-only default listener mode, `127.0.0.1:0`, actual
  port read from `listen_addr()` after binding (no free-port reservation race).
  Downloader has no listener. No uTP listener is enabled.
- Downloader starts paused. Only the seed's validated loopback address is
  supplied through `AddTorrentOptions::initial_peers`; public `unpause()` starts
  the transfer after the missing-piece test. The internal live add-peer method
  is not public API in 9.0.1. There is no public swarm,
  bootstrap, tracker, DNS discovery or multicast. This is configuration-based
  isolation, not an OS network sandbox; use a firewall/network namespace too
  if an independent egress guarantee is required.
- Seed is hash-checked complete before any download. Seed upload is limited to
  256 KiB/s, peer limit one. This creates time to observe seek behavior before
  the complete 8 MiB has arrived; it is not a throughput benchmark.
- `handle.stream(0)` yields an AsyncRead/AsyncSeek `FileStream`. A stream requires
  initialized metadata; stream creation alone is **not** piece readiness.
  Read completion and independent byte comparison are the readiness criterion.

## Checks performed by the runnable example

1. Seed initialization verifies complete; downloader starts at zero progress;
   invalid file index is rejected.
2. While paused, before connecting to the seed, a 4 KiB read must remain pending for 200 ms. Preallocated zeroes
   must not be exposed. The timeout cancels the read future and the entire reader
   is dropped, avoiding reuse of a partially filled `read_exact` buffer.
3. Connect the sole loopback peer. Seek/read/verify 4 KiB at tail, head, middle
   (unaligned offset), then head again. Each printed latency includes seek,
   waiting for pieces, reading, and byte verification. Every observation requires
   download still incomplete; full-file download followed by seeking fails.
4. Seek to EOF and require a zero-byte read.
5. Stop the incomplete session with a three-second deadline. Stop all sessions
   also on error/40-second experiment timeout; drop owners before explicitly
   closing the TempDir. Cleanup failure is a failure, not silent success.
6. Check logical file sizes: seed exactly 8 MiB; download at most 8 MiB. Temporary
   files are removed. This bounds this fixed fixture, **not a disk quota or
   sliding-window cache**. librqbit uses full-file storage/preallocation, not a
   proven bounded-cache playback policy. RAM/allocated filesystem blocks are not
   measured. No download-completion requirement is imposed.

The test target additionally checks oracle corruption/wrong-offset detection,
fixture size and discovery configuration, and executes the actual local swarm.
The whole-experiment deadline bounds async network waits, not arbitrary
uninterruptible filesystem calls. No automatic retry hides failures.

### Cancellation limitation

Upstream `Session::stop()` pauses torrents, cancels its token and waits a fixed
one second; it does **not** expose a task-join completion guarantee. This probe
measures that API returning within its deadline and verifies temporary file
cleanup on the tested host. The dropped missing-piece reader tests caller-owned
read cancellation. It does not claim that stopping the session wakes every
possible outstanding reader, or proves zero leaked tasks on all platforms.
A production adapter needs explicit ownership, generation rejection, cancellation
semantics, storage eviction and error/readiness policy beyond this experiment.

## License and product implications

librqbit 9.0.1 declares **Apache-2.0**, matching this workspace's declared license.
Its transitive graph has additional licenses; this is not a complete distribution
license audit. Shipping it requires reviewing the resolved graph, retaining
required license/NOTICE material, security review and considering BitTorrent
upload/privacy/bandwidth implications. Opt-in does not qualify it for production.
No upstream source is copied or vendored by this POC. Generated data contains no
copyrighted media. This test seeds/uploads only to its own loopback downloader.

## Recorded validation

Linux host, Rust 1.97.1, 2026-10-28: the locked example run **passed**.
Generated torrent seeded over loopback; no user content or credentials used.
First successful run (milliseconds, not a benchmark or latency guarantee):

| Seek | Offset | Independently verified | Seek-to-verified-bytes | Download progress after read |
| --- | ---: | ---: | ---: | ---: |
| tail | 8,384,512 | 4,096 bytes | 41 ms | 65,536 / 8,388,608 |
| head | 0 | 4,096 bytes | <1 ms | 131,072 / 8,388,608 |
| middle | 4,194,427 | 4,096 bytes | 7,748 ms | 2,293,760 / 8,388,608 |
| head again | 0 | 4,096 bytes | <1 ms | 2,293,760 / 8,388,608 |

The middle seek was noticeably slower: this is evidence of usable seek/read APIs,
**not uniformly low-latency seeking**. Inspection of pinned 9.0.1's
`torrent_state/streaming.rs` shows a fixed 32 MiB per-stream lookahead
(`PER_STREAM_BUF_DEFAULT`); investigate that and already queued peer requests
when profiling the seek delay. This is a hypothesis, not an established cause. The experiment does not configure or prove
a production scheduling/readahead policy. Missing-piece read stayed pending for
200 ms, EOF succeeded, incomplete-session stop returned in 1,001 ms, logical
storage was exactly 8,388,608 bytes in each directory, and explicit temporary
cleanup succeeded.

Final checks **passed**:

- The locked `cargo run` command above.
- Locked example tests: **4 passed**, including a second real localhost swarm
  run (11.06 s overall; middle seek again 7,748 ms, stop 1,000 ms).
- Strict locked example Clippy (`-D warnings`).
- `cargo fmt --all -- --check` and `git diff --check`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `cargo test --workspace --locked`: **802 passed, 26 ignored**. These default
  workspace checks skip the feature-gated example; its separate checks above
  exercise the POC.
- Default desktop `cargo tree --locked` contains no librqbit entry. The lockfile
  adds 80 package/version pairs, removes none, and keeps existing versions.

The initial compile exposed the private live add-peer API; the runnable design
uses supported paused initialization + initial peers + unpause instead, without
patching librqbit. No unresolved blocker remains for this local experiment.
No Windows/macOS, real media playback, native MPV, public-provider, malicious
swarm, large-file, disk-full, eviction or production-backend qualification is
claimed by this experiment.
