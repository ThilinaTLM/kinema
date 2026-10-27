# Media integration contract

Use `crate::media::{MediaService, MediaError, PlaybackSession, UsageSnapshot}`.
All service construction, registration, quota changes, cleanup, and snapshot polling
belong on the application service worker. No GPUI dependencies are used here.

- `MediaService::start(&AppPaths, CacheBudget) -> Result<Self, MediaError>`
- `register_remote(ResolvedMedia, Arc<dyn Fn() -> Result<ResolvedMedia, MediaError> + Send + Sync>) -> Result<PlaybackSession, MediaError>`
- `register_local(&Path) -> Result<PlaybackSession, MediaError>`
- `release(&str) -> Result<(), MediaError>` (idempotent, reports lease-state failures); `set_limit(CacheBudget) -> Result<(), MediaError>`
- `usage() -> UsageSnapshot`; `cleanup_unused() -> Result<CleanupReport, MediaError>`
- `session_errors() -> Vec<(String, MediaError)>` contains asynchronous acquisition
  errors for currently leased sessions, never provider URLs or credentials.
- `session_updates() -> Vec<SessionUpdate>` reports local/remote origin, per-asset
  downloaded/total bytes for remote leases, and reveal capability. Shared-asset
  leases share progress, not quota reservations.
- `session(&str) -> Result<PlaybackSession, MediaError>` validates an existing
  lease for replay; it never creates another lease.
- `reveal_path(&str) -> Result<PathBuf, MediaError>` resolves an active lease's
  canonical local file or validated managed payload. Only the worker passes it to
  the platform file-manager adapter; the UI never submits arbitrary reveal paths.

`CleanupReport { freed_bytes, protected_files }` counts protected cached payloads,
not duplicate leases or original local files. Freed bytes are released cache
reservations, not a measurement of physical filesystem blocks.

`PlaybackSession { id, url }` contains only a random local capability URL.
Retain its lease while an external player needs access; release invalidates new
requests and signals existing readers. An in-flight buffered chunk may already be
in the player's socket. Existing local files are opened in place, held by file
identity, excluded from managed quota, and never copied or deleted. Canonical local
paths inside the owned media root are rejected, so managed sparse payloads cannot
bypass availability or lease tracking through local registration.

## HTTP and acquisition

Bind is **127.0.0.1 only**, ephemeral port, one opaque route. GET/HEAD support full
files, single closed/open/suffix byte ranges with 206 and inclusive Content-Range;
invalid, unsatisfiable, duplicate, and multiple ranges deliberately return 416
with `Content-Range: bytes */size`. Full empty local files return 200/length zero;
explicit ranges on them return 416. GET and HEAD use static MIME values selected
from the original local filename or remote identity file-path extension (MP4/M4V,
MKV, WebM, MOV, AVI, MPEG/MPG, MP3, FLAC, WAV, OGG); unknown extensions use
`application/octet-stream`. Filenames are never added to routes or header values.
Core `parse_http_range`, `ByteRange`, and
`RangeSet` implement range policies. Headers do not claim any missing bytes are
available: streaming waits for durable range membership and aborts on failure.

Each active remote asset has one acquisition owner. It fetches at most 4MiB per
request, clipped to the next missing gap or file end, prioritizing missing reader
offsets over sequential background progress. The larger range amortizes measured
request-header latency; it does not introduce parallel requests per asset. Each
worker buffers one full range plus one validation probe byte (about 32MiB of
payload buffers across the maximum 8 acquisition workers, excluding HTTP/TLS,
allocator and other service overhead). A range becomes available only after full
validation, payload sync and manifest checkpoint. Larger ranges can increase
initial/seek latency: a new reader priority is honored after the in-flight cycle,
not by interrupting it. Cancellation checks and durability ordering are unchanged.

Loopback HTTP bodies use 64KiB buffers and a two-message queue. Limits: 8 acquisition
workers, 8 HTTP workers, 32 accepted sockets and 32 session leases. Socket inactivity
is 60s; a missing chunk waits at most 45s; a stalled body consumer at most 30s.
Upstream connect/whole-request deadlines remain 10s/20s, including body transfer
in the whole-request deadline. A slow 4MiB transfer may therefore fail rather than
receive more time; larger chunks do not guarantee adequate provider throughput.
Drop only sets cancellation
and signals the owned server thread; it never joins or waits for provider I/O.
The root lock remains owned until all service/network/reader owners finish.

Upstream must return exact 206, Content-Range, Content-Length and identity
Content-Encoding. HTTP 200 fallback/full download is not implemented and returns
`UnsupportedRange`. Redirects are disabled (including HTTPS redirects); provider
resolution must yield a usable final HTTPS URL. Test-only provider constructors
allow loopback HTTP fixtures. Expired or 5-minute-old links renew before a missing
fetch, as do 401/403 responses. Two renewals without validated durable byte
progress fail; progress resets that budget. Renewed hash/path/size must match.
**Renewal callbacks must themselves have bounded I/O/timeouts**, as the service
cannot forcibly interrupt an arbitrary synchronous caller-supplied closure.

## Storage and recovery

`paths.cache/media/<SHA256 identity>/payload` and a version-1 `manifest.json`
contain only identity and durable ranges, not URLs/secrets. Full known logical
sizes reserve quota; new admissions check `new size + outstanding unwritten
reservations <= physical free space` as well as the cache budget. No eviction.
Lowering quota below reserved usage fails. Failed deletions stay reserved.

The cache parent is canonicalized once, allowing native OS aliases above it;
the owned media root itself and managed asset/payload/manifest symlinks are
rejected. A locked root rejects another instance. New assets are prepared in recognizable
`.create-<hash>` directories and atomically published only after a synced payload
and manifest. Checkpoints sync payload first, write/sync `manifest.next`, atomically
replace the manifest with tempfile's platform primitive, and sync directories on
Unix. Cleanup first renames inactive assets to `.delete-<hash>` tombstones. Startup
finishes known staging/tombstone cleanup and discards uncommitted `manifest.next`;
it trusts only validated manifest ranges in payloads of the exact known length.
Unknown entries are preserved; deletion never recurses or follows symlinks.
Session, acquisition, and HTTP-reader ownership independently pin an asset.

Tests use actual loopback HTTP byte fixtures and temporary storage. Windows/macOS
build/runtime and power-loss filesystem behavior are not established by Linux
fixture tests. No real provider account or external-player playback was tested.

## Startup investigation / documentation handoff (2026-10-25)

Source evidence, not a native performance measurement:

1. Registration reserves/prepares storage on the service worker, then starts one
   acquisition owner per asset. Loopback GET sends response headers before its
   body worker waits for data. A successful/fast loopback `send()` alone therefore
   says nothing about first playable bytes.
2. Each upstream request selects one missing range, at most 4 MiB. Header timing
   includes connection setup and upstream response latency; it does not separate
   DNS, TCP, TLS, provider processing, or network scheduling. The body must finish
   and validate before payload write/sync and manifest checkpoint. Only then are
   ranges published and readers notified. This is the documented durability and
   latency tradeoff, **not a newly demonstrated reader bug**. Even a 64 KiB reader
   can wait for a complete 4 MiB upstream response. No parallel range fetch or
   partial-range publication was introduced in this investigation.
3. Missing reader offsets outrank sequential acquisition at the next cycle only.
   In-flight work is not interrupted by a head/tail seek. Actual container probing
   can request the tail, but these fixture tests do not show which probes a real
   demuxer performs or how long native decoder startup/presentation takes.
4. A demonstrated scheduling defect was fixed: readers repeatedly leave a priority
   while its range is in flight. Once that offset became durable, the old selection
   scanned forward from the stale offset into a *later* gap, skipping earlier
   background holes for another entire cycle. Selection now honors only an
   actually missing priority byte; otherwise it returns to the earliest gap.
   Reader hints now point to the first actually missing byte within the requested
   64 KiB buffer, not blindly its starting offset. This matters when a buffer
   begins in a durable seek range but crosses its end: rejecting the durable
   starting byte must not suppress the missing suffix's interactive priority.
   The deterministic overlapping-boundary regression uses durable ranges
   `[0, CHUNK)` and `[16*CHUNK, 17*CHUNK)`, with a reader spanning 32 KiB on either
   side of `17*CHUNK`; its suffix is selected before the early background hole.
   After the window becomes durable, stale hints again fall back correctly.
   A real stream-worker regression additionally checks the priority actually
   published by `server.rs`, not just the scheduler/helper composition.
   Scheduler regressions also cover missing, satisfied and out-of-bounds hints. This can affect a middle seek with later gaps; an EOF-tail seek
   with no later gap already fell back correctly. No universal startup-speedup or
   fair arbitration among simultaneous readers is claimed; priority remains a
   single last-writer-wins offset, not an owned/cancelable demand queue.
5. Release wakes readers independently of an upstream blocked header/body. The
   synchronous HTTP call itself cannot be forcibly interrupted: cancellation is
   checked after I/O returns, bounded by the existing 20 s whole-request deadline
   (10 s connect deadline), except caller renewal closures must bound themselves.
   The loopback missing-buffer wait is 45 s, polled every 200 ms; expiration and
   disconnection both currently use the fixed `Canceled` category. Cancellation
   does not promise zero bytes already queued to a socket. An error after loopback
   response headers aborts the body, rather than changing that response status.

### Opt-in observations

Exact `KINEMA_PLAYER_DIAGNOSTICS=1` enables the new observations on target
`kinema_desktop::media_performance`. Disabled means no timer owners or startup
summaries. IDs are process-local monotonic numbers, not identity hashes, pointers,
capability/session IDs, URLs or persistent identifiers:

- `asset_id` correlates acquisition and remote readers in this process.
- `operation_id` identifies one acquisition invocation or HTTP body reader.
- `request_id` is the upstream attempt number inside an acquisition; readers use
  zero and are distinguished by `operation_id`.
- `range_start`/`range_end` are numeric half-open offsets. Reader events retain the
  original HTTP body range; acquisition pending/end events retain its current
  fetch range. `near_tail` means start is within the final 4 MiB, **not proof of a
  demuxer/container probe** (small files also satisfy it).

A timer reports the latest fixed phase and its age every five seconds, at most
**12 pending events per operation** (first minute). It does not queue per-chunk
updates. Pending phase age identifies where work currently waits even if no range
has completed. Fast phase transitions may never appear in pending samples;
absence of pending logs does not prove an operation is idle or finished.

One first-durable event per acquisition includes elapsed worker time and first
successful range's header/body/payload-sync/checkpoint times. First queued reader
bytes get one event, measuring handoff to the bounded body queue, **not delivery
at mpv or physical presentation**. One terminal summary reports success/failure,
fixed typed error category, elapsed and latest-phase time. Reader terminal events
also aggregate `range_wait_count`, `range_wait_total_ms`, `range_wait_max_ms` over
all 64 KiB buffer segments that actually encountered unavailable bytes, including
failed/canceled waits. These counts are not HTTP request counts. `phase_ms` is
only the latest segment/phase; it is not total wait time. Still-open waits appear
in pending phase age and enter aggregate totals only when they exit.

These first/end/pending events complement the existing five-second completed-
durable-range aggregates, which exclude failed/retried/incomplete attempts and
cannot by themselves explain a blocked startup. No per-chunk INFO was added.
Only fixed enums/categories, numbers and booleans enter these observations.

There is at most one extra timer thread per active opt-in acquisition/body reader:
**8 acquisition + 8 HTTP = 16 live timer threads**, each with one constant-size
snapshot, no history. Timer ownership is inside `storage::acquire` or the blocking
HTTP stream worker, and Drop signals/joins only there, never from UI/service Drop.
Timer spawn failure disables that observation without failing playback. Existing
five-second acquisition summaries remain separate and are not lifetime-capped.

### Fixtures, optimization and remaining qualification

Local TCP gates hold before headers or after headers plus the first 64 KiB. Tests
prove no sparse/partial bytes become visible until durability, normal ranged
recovery after opening the gate, and failed body reads on lease release or HTTP
200/no-range failure. Both blocked-header and blocked-body cancellation preserve
zero downloaded bytes. Existing fixtures cover malformed Content-Range/length,
encoding, redirects, truncation, renewal exhaustion, missing-tail prioritization,
restart and request counts. Assertions use gates/request shape, not throughput or
host-sensitive latency thresholds. Observer tests cover exact opt-in parsing,
12-event cap, numeric correlations, fixed failure categories, wait totals, first-
event deduplication, privacy exclusions and interruptible owned timer teardown.

Read-only root `Cargo.toml` review: dev workspace code uses default opt-level 0
with debug=1; third-party packages use opt-level 2; `kinema-mpv` explicitly uses
opt-level 2. This is intentional existing configuration. No manifest/lock/vendor
change is needed to explain the durable-range wait. A release-vs-dev native A/B
might distinguish CPU/build effects, but cannot infer network/header/body delays
from configuration alone.

No native/GUI/GPU probes, provider access, user configuration, credentials, user
DB or user media were used. Focused commands (Rust 1.97.1, from repository root):

```sh
cargo test -p kinema-desktop --locked media:: -- --test-threads=4
KINEMA_PLAYER_DIAGNOSTICS=1 cargo test -p kinema-desktop --locked media:: -- --test-threads=4
cargo clippy -p kinema-desktop --all-targets --locked -- -D warnings
rustfmt --edition 2024 --check crates/kinema-desktop/src/media/{diagnostics,mod,storage,server,remote_tests,tests}.rs
git diff --check -- crates/kinema-desktop/src/media
```

Final focused validation (including overlapping-boundary QA correction): **41
media tests passed with diagnostics disabled and 41 passed with diagnostics enabled**, strict all-targets locked desktop Clippy
passed, owned-file rustfmt check passed, and owned-path diff check passed. Initial
attempts were temporarily blocked by concurrent backend edits; no workaround or
out-of-scope correction was made. Full workspace checks/native probes were not
run by this owner.

For the policy/documentation owner: reconcile the opt-in section of
`docs/development.md` with these new first/end/pending observations; retain the
completed-only aggregation limitations. No changes outside `src/media/` were
made by this owner. Before native qualification, the lead should schedule the
existing **offline PlayerWindow fixture** command from `docs/development.md`,
first disabled then with `KINEMA_PLAYER_DIAGNOSTICS=1` and info tracing, optionally
repeat release profile. Do not start ordinary Kinema: changing CWD is not account
or credential isolation. The direct local-file native probe bypasses acquisition,
so it cannot qualify these HTTP wait timings; a lead-approved synthetic local
upstream-to-loopback fixture driver is still needed for native end-to-end head/
tail probing comparisons. No such native acquisition command is claimed here.

Recommended **lead-authorized only**, Linux/offline native command sequence (not
run by this owner; requires native prerequisites and an isolated fixture session):

```sh
cargo build -p kinema-desktop --bin ui-probe --features ui-probe --locked
mkdir -p target/playback-startup-fixture
ffmpeg -n -f lavfi -i testsrc2=size=1280x720:rate=30 -t 10 -c:v mpeg4 -q:v 5 -an target/playback-startup-fixture/generated.mp4
# Baseline: leave KINEMA_PLAYER_DIAGNOSTICS unset.
target/debug/ui-probe --scene player-native --media target/playback-startup-fixture/generated.mp4 --smoke-seconds 8
# Opt-in comparison; do not share unreviewed native/application output.
KINEMA_PLAYER_DIAGNOSTICS=1 RUST_LOG=kinema_desktop=info target/debug/ui-probe --scene player-native --media target/playback-startup-fixture/generated.mp4 --smoke-seconds 8
```

This qualifies only the supplied offline player path, **not acquisition**. Native
acquisition comparison remains gated on a separately approved fixture driver.
