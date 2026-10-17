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

Each active remote asset has one acquisition owner. It fetches 256KiB chunks,
prioritizing missing reader offsets over sequential background progress. HTTP
bodies use 64KiB buffers and a two-message queue. Limits: 8 acquisition workers,
8 HTTP workers, 32 accepted sockets and 32 session leases. Socket inactivity is
60s; a missing chunk waits at most 45s; a stalled body consumer at most 30s.
Upstream connect/whole-request deadlines are 5s/20s. Drop only sets cancellation
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
