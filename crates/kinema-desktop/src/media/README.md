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

Each active remote asset has one acquisition coordinator and 1–4 parallel range
fetches in bounded waves. The process-wide limit is 16 reservations, including
completed bodies waiting for disk commit; every scoped fetch thread is joined.
Only the coordinator writes payloads, syncs and checkpoints, then publishes durable
ranges. Out-of-order completion cannot expose sparse holes or incomplete bodies.
The maximum reserved response payload is 16 × (4 MiB + one probe byte), plus up
to 16 × 64 KiB read scratch buffers. HTTP/TLS, allocator, thread-stack, range-set
and filesystem overhead are excluded; this is not a strict process RSS bound.

Range size starts at 256 KiB and adapts between 64 KiB and 4 MiB, targeting about
two seconds of observed body delivery, growing at most twofold per healthy wave.
Body failures/timeouts halve it. The first missing interactive range is at most
256 KiB. Local reader delivery provides bounded, decaying demand hints, **not media
bitrate measurements**. The controller uses contiguous durable bytes ahead, reader
waits, useful throughput and hysteresis: sustained pressure grows concurrency,
healthy buffer coverage shrinks it, and throttling halves it. Isolated transport
failures do not automatically halve concurrency. With no active reader, background
completion uses at most two requests after two healthy waves.

An outstanding missing reader hint is retained until satisfied, avoiding two
readers continually canceling each other. A distant new hint (>16 MiB from the
previous hint) advances a generation; obsolete results cannot become live ranges.
Cancellation is checked between body reads and before publication, but cannot
forcibly interrupt a blocking request. A seek during checkpoint may leave validated
bytes in the durable manifest without publishing them live. Guaranteed fairness,
immediate cancellation and real-provider performance remain unqualified.

Loopback HTTP bodies use 64KiB buffers and a two-message queue. Limits: 8 acquisition
workers, 8 HTTP workers, 32 accepted sockets and 32 session leases. Socket inactivity
is 60s; a missing chunk waits at most 45s; a stalled body consumer at most 30s.
Upstream connect/whole-request deadlines remain 10s/20s, including body transfer
in the whole-request deadline. Adaptive sizing reduces slow-link exposure but
cannot guarantee progress below the minimum-range/deadline combination or during
long outages. Requests are not governed by a separate idle-progress deadline.
Drop only sets cancellation
and signals the owned server thread; it never joins or waits for provider I/O.
The root lock remains owned until all service/network/reader owners finish.

Upstream must return exact 206, Content-Range, Content-Length and identity
Content-Encoding. HTTP 200 fallback/full download is not implemented. A strong
ETag on the first accepted response binds the acquisition: later requests send
If-Range and require the same strong ETag, including after URL renewal; changed or
missing validators and HTTP 200 in this mode fail with IdentityMismatch. Without
an initial strong validator, the existing provider MediaIdentity (torrent hash,
path, size) is trusted. Validators are not persisted across acquisition restarts;
this is not cryptographic payload verification or independent proof of immutable
content from an untrusted origin.

Redirects remain disabled (including HTTPS redirects); provider resolution must
yield a usable final HTTPS URL. No URL/ETag/header content or raw HTTP errors are
logged or persisted. Test-only provider constructors allow loopback HTTP fixtures.
Expired or 5-minute-old links renew before a missing fetch, as do 401/403 responses.
Renewal belongs to the coordinator, never individual parallel workers. Two
renewals without validated durable byte progress fail; progress resets that
budget. Renewed hash/path/size must match.

Transient transport/body failures and HTTP 500/502/504 receive at most three
consecutive-wave retries, with 250/500/1000 ms backoff. HTTP 429/503 also reduce
concurrency and respect delta-seconds or IMF-fixdate Retry-After up to 30 seconds;
a larger requested wait fails explicitly rather than retrying before the server's
minimum. Waits observe cancellation. Terminal validation errors cannot be replaced
by a later transient error in the same wave. This is per-asset coordination, not
a provider/account-wide rate limiter or an unlimited reconnect policy.
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

Architecture overview: [media storage and serving](../../../../docs/architecture/media.md).

## Opt-in diagnostics

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
