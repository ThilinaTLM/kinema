# Storage and media server

**Agreed target:** media is stored locally and served to external players,
including while downloading, with a user-configurable cache maximum. Only the
catalog location below originated in the first increment. Settings, managed-media,
and loopback-server modules are now wired in source; broader artwork/log and
cleanup policies not explicitly labeled implemented remain proposals.

## Implemented catalog location

The build's `debug_assertions` setting selects the catalog location:

- **Development (`debug_assertions` enabled):** `./data/catalog.sqlite3`, relative
  to the process working directory, not the executable or source directory. Run
  `cargo run -p kinema-desktop --locked` from the repository root to use the
  gitignored root `data/` directory.
- **Production (`debug_assertions` disabled):** resolve
  `ProjectDirs::from("", "", "kinema")` through the `directories` crate and use
  `data_local_dir()/catalog.sqlite3`. The default release profile selects this
  behavior: `cargo run -p kinema-desktop --release --locked`. Native directories
  are resolved at runtime, not constructed from hardcoded HOME/APP_DATA paths.

The default debug and release profiles therefore use separate databases; switching
modes does not copy or merge their data. A custom profile's `debug_assertions`
setting, rather than its name, determines the location. Existing files under root
`data/`, the previous namespace, or historical locations are not moved, merged
across locations, or deleted. This path-preservation rule does not prohibit the
authorized in-place Rust schema upgrade described below. Resolution/directory/storage failures are reported, not treated as
permission to use another database or silently reset one.

The current `AppPaths` resolver also selects `config_dir()` and `cache_dir()`
for production; in development all three roots are the same `./data/` directory.
The settings store uses `CONFIG/settings.toml`; managed storage uses
`CACHE/media/` with per-asset `payload` and `manifest.json` files. OS credentials
remain separate. Source wiring and reported recovery fixtures do not establish
cross-platform crash safety or end-to-end paid-provider playback.

The backend reports an in-place Rust **v3 → v4** schema upgrade: twelve normalized
`STRICT` tables plus all seven retained legacy tables (nineteen application tables),
with schema creation, conservative backfill, and version advance atomic. Canonical
metadata/indexer paths use the new rows under the
[approved catalog specification](catalog-schema.md#target-catalog-schema). This
changes neither database paths nor media/settings/credential locations. It is not
a migration of historical C++ data or permission to reset/move user files.

Preserved legacy data does not guarantee immediate offline reuse: nonempty
Cinemeta exact-query searches are not backfilled because shared-field provenance
is ambiguous; first canonical queries need refresh/fetch. Known legacy detail
fields retain `Detail` provenance quality, while fetch-resource completeness stays
`Summary` to force richer refetch on demand; source targets lacking verified canonical
mapping remain unmapped in legacy storage. See [backfill limits](catalog-schema.md#conservative-v3-preservation-and-backfill-limits).
Current post-fix root fmt, strict locked workspace Clippy, locked tests (295 passed,
3 ignored), and diff checks passed; full post-v4 UI/provider acceptance remains
unverified.
Agents did not access user databases or credentials. A user's database upgrades
only on next open with the new build, not as part of this documentation update.

Artwork encoded bytes now use `AppPaths.cache/artwork/<sha256-of-url>.img`:
`./data/artwork/` in development and native `cache_dir()/artwork/` in production.
The independent 256 MiB best-effort disk budget, atomic writes, validation and
mtime-based eviction are described in [cache policy](cache-policy.md#implemented-artwork-cache).
This disposable cache is not part of the managed-media quota or catalog and holds
no URL index, provider credentials or error responses.

Artwork bytes remain a separate cache; media payloads/manifests and credentials
remain outside the catalog. Cache/media cleanup must preserve canonical identities,
ratings, provenance, and referenced page/fetch stubs; identity reconciliation is a
separate proven-ID transaction, not eviction. No history/library tables are added.

## Proposed broader locations

**Agreed development root:** with `debug_assertions` enabled, all ordinary
application data (including future settings, artwork, logs, and managed media)
belongs under `./data/` relative to the process working directory. Credentials
remain in OS secure storage, never this tree. Catalog, settings, and media storage
use this rule; artwork is implemented as documented above, while log caching/
rotation in the proposed layout is not a claim of implemented behavior.

For production, use native directories in the `kinema` application namespace, not
hardcoded home paths. The following broader root/layout examples remain proposed,
not an exact resolver contract:

| Root | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Data | `%LOCALAPPDATA%\Kinema\data` | `~/Library/Application Support/Kinema` | `$XDG_DATA_HOME/kinema` or `~/.local/share/kinema` |
| Config | `%LOCALAPPDATA%\Kinema\config` | Same as data | `$XDG_CONFIG_HOME/kinema` or `~/.config/kinema` |
| Cache | `%LOCALAPPDATA%\Kinema\cache` | `~/Library/Caches/Kinema` | `$XDG_CACHE_HOME/kinema` or `~/.cache/kinema` |

These proposed broader roots illustrate native locations, not the current resolver's exact namespace. Resolve actual paths at runtime, including Unicode paths and valid XDG overrides. A user-selectable **media cache location** is proposed, not yet agreed. Other roots stay separate.

```text
DATA/
  catalog.sqlite3               # Actual catalog location follows build mode above
CONFIG/
  settings.toml                 # Non-secret settings
CACHE/
  artwork/
  logs/                         # Bounded, rotated diagnostics
  media/
    <opaque-asset-id>/
      payload                   # One file, partial or complete
      manifest.json             # Identity and downloaded ranges
```

In the proposed production layout on macOS, settings sit beside the database because data/config share a root. SQLite manages its own WAL/SHM files. Credentials live in the OS secure store, not this tree.

Keep the catalog in app data so deleting large media caches does not remove identities/settings. Cached catalog/indexer rows can be pruned transactionally; ordinary cleanup must not delete the database. Future personal records must remain separate from disposable data.

## One media file, one manifest

Each managed asset has one payload file. Seek-driven writes can fill different parts of it; no separate file per chunk or duplicate “complete” copy is needed.

The manifest contains a format version, stable upstream file identity, expected size when known, checkpointed downloaded ranges, and basic last-use information. A stable title/episode reference is optional; a local catalog row ID must not be the only media identity. Do not store credentials or expiring access URLs.

Track downloaded ranges explicitly. A sparse/preallocated file's length does not mean its holes contain video. Completion means all bytes of the known file are available.

Completed writes can be served immediately from the live range map. Periodically checkpoint it: synchronize payload bytes first, then atomically replace the manifest with the ranges known to be durable. Batch this work rather than flushing for every player read.

After restart, validate the manifest and payload and recover only checkpointed ranges; incomplete or inconsistent data is downloaded again. File identity must still match when renewing upstream access. This protects against interrupted writes, not arbitrary corruption; a checksumming system is not needed initially.

## Serving and seeking

The player receives an opaque local URL, never an arbitrary filesystem path or upstream credential. Existing user files are read in place, not copied into the managed cache.

The server should:

- Handle `GET`, `HEAD`, and byte ranges, including open-ended/suffix ranges, with correct `206`/`416`, lengths, and content types. Define multi-range behavior explicitly.
- Read available bytes from storage. For missing bytes, ask acquisition to prioritize that range and bounded read-ahead; combine overlapping requests.
- Bound buffers, concurrent requests, and waits. Cancel unnecessary reads when a player disconnects, allowing for later reconnects.
- Renew expired upstream access without mixing different file versions. Report upstream range/format limitations rather than promising every file can play before completion; some containers require tail metadata.

The server delivers bytes, not decoded/transcoded video. **Agreed:** the MVP
server binds to loopback only and serves players on the same computer. LAN access
is excluded, avoiding unintended network exposure and additional authentication
and firewall requirements. This is a requirement, not a claim of an implemented
server acceptance result; the current source binds loopback, with reported local
serving smoke evidence summarized in the [validation matrix](development.md#validation-status-and-limits).

## Limits and cleanup

**Agreed:** the default managed-media quota is **20 GiB** (20 × 1024³ bytes),
editable by the user. Manual cleanup of inactive managed assets is sufficient for
the MVP; no active eviction or silent deletion is allowed. These are agreed safety
policies, not claims of implemented quota controls or cleanup UI:

- Wait or refuse acquisition until the file size is verified; do not admit an
  unknown-size file against an assumed reservation.
- Reject lowering the limit below already reserved usage. Explain the conflict;
  do not delete media silently to make the new limit fit.

The media maximum is separate from artwork, database, and log budgets. Count actual allocated bytes and reserved download capacity conservatively; sparse logical size is not disk usage. Check physical free space too—raising the configured limit does not create space.

If a single file exceeds the limit, explain its size and offer to increase the limit. Active files must not be evicted. If active media fills the budget, pause/refuse further acquisition with an explanation, not silent overuse or deletion under readers.

Manual cleanup removes whole inactive managed assets, including abandoned partial downloads. Hold an active-use reference while downloading, serving, or keeping a playback session available; only delete once users of that asset release it. Count files awaiting successful deletion toward usage.

Never delete original user files. Generated asset directories must stay inside the owned cache root; cleanup must not follow symlinks/junctions outside it. If a custom cache root is added, use a dedicated managed directory rather than claiming unrelated existing files.

Cache loss is recoverable: invalidate missing media/artwork and reacquire it; keep catalog/settings intact. A cache is not guaranteed offline storage. Do not add separate torrent/progress directories or another asset database before those features need them.

## Cross-platform checks

Test interrupted writes/deletions, disk exhaustion, cache loss, file locks, Unicode paths, and player reconnects on Windows, macOS, and Linux. Do not assume POSIX open-file deletion or atomic-replace behavior everywhere. Serialize writes/cleanup for a media root; a second application instance must not corrupt it.

A custom cache location and checkpoint intervals remain open. See [decisions](decisions.md); response-cache lifetimes belong in [caching](cache-policy.md).
