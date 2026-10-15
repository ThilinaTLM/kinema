# Storage and media server

**Agreed:** media is stored locally and served to external players, including while downloading. The cache has a user-configurable maximum. **Proposed:** the locations and file layout below.

## Where files live

Use the operating system's directory resolver, not hardcoded home paths. Typical defaults are:

| Root | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Data | `%LOCALAPPDATA%\Kinema\data` | `~/Library/Application Support/Kinema` | `$XDG_DATA_HOME/kinema` or `~/.local/share/kinema` |
| Config | `%LOCALAPPDATA%\Kinema\config` | Same as data | `$XDG_CONFIG_HOME/kinema` or `~/.config/kinema` |
| Cache | `%LOCALAPPDATA%\Kinema\cache` | `~/Library/Caches/Kinema` | `$XDG_CACHE_HOME/kinema` or `~/.cache/kinema` |

These illustrate native locations; resolve actual paths at runtime, including Unicode paths and valid XDG overrides. A user-selectable **media cache location** is proposed, not yet agreed. Other roots stay separate.

```text
DATA/
  catalog.sqlite
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

On macOS, settings sit beside the database because data/config share a root. SQLite manages its own WAL/SHM files. Credentials live in the OS secure store, not this tree.

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

The server delivers bytes, not decoded/transcoded video. Same-computer-only access is recommended for the MVP, but LAN scope is still [undecided](decisions.md).

## Limits and cleanup

The media maximum is separate from artwork, database, and log budgets. Count actual allocated bytes and reserved download capacity conservatively; sparse logical size is not disk usage. Check physical free space too—raising the configured limit does not create space.

If a single file exceeds the limit, explain its size and offer to increase the limit. Rolling eviction of an active file is not required. If active media fills the budget, pause/refuse further acquisition with an explanation, not silent overuse or deletion under readers.

Start with whole-asset cleanup of unused managed files, including abandoned partial downloads. Hold an active-use reference while downloading, serving, or keeping a playback session available; only delete once users of that asset release it. Count files awaiting successful deletion toward usage.

Never delete original user files. Generated asset directories must stay inside the owned cache root; cleanup must not follow symlinks/junctions outside it. If a custom cache root is added, use a dedicated managed directory rather than claiming unrelated existing files.

Cache loss is recoverable: invalidate missing media/artwork and reacquire it; keep catalog/settings intact. A cache is not guaranteed offline storage. Do not add separate torrent/progress directories or another asset database before those features need them.

## Cross-platform checks

Test interrupted writes/deletions, disk exhaustion, cache loss, file locks, Unicode paths, and player reconnects on Windows, macOS, and Linux. Do not assume POSIX open-file deletion or atomic-replace behavior everywhere. Serialize writes/cleanup for a media root; a second application instance must not corrupt it.

Default quota, cleanup order, unknown-size files, lowering the limit, and checkpoint intervals remain open. See [decisions](decisions.md); response-cache lifetimes belong in [caching](cache-policy.md).
