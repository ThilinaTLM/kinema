# Current Rust implementation

Rust is Kinema's sole implementation. This is a source-based overview,
not a completion report, runtime qualification or C++ parity checklist. The
[architecture](architecture.md) separates approved scope from deferred work;
[development](development.md) records prerequisites and scoped validation limits.

## Desktop and ownership

- Direct GPUI (`gpui-pre =0.3.9`) and `gpui_platform` bootstrap **v3 only**, with
  in-house widgets; no GPUI Kit, Graphite default or UI-version selector.
- Browse, detail/episodes and Streams pages compose with a separate Settings
  window, preparation sheet, transfers popover, local-file opening and quit
  confirmation. The coordinator owns background workers and rejects stale
  results by generation; blocking provider/storage work stays outside rendering.
- Pure values and policies live in `kinema-core`; desktop adapters and GPUI
  presentation live in `kinema-desktop`; native playback lives in `kinema-mpv`.

**Source:** [workspace](../Cargo.toml), [bootstrap](../crates/kinema-desktop/src/main.rs),
[widgets](../crates/kinema-desktop/src/widgets/),
[coordinator](../crates/kinema-desktop/src/screens/coordinator.rs).
**Contract:** [desktop v3](design/desktop-v3.md). Source composition does not establish
keyboard/accessibility, visual, performance or cross-platform acceptance.

## Metadata and catalog

- Cinemeta and token-enabled TMDB adapters provide browse/search, title details
  and series episodes. Settings hold provider priority; background enrichment
  fills missing metadata. Domain/display data includes optional artwork, ratings,
  identifiers and facts; this is not a full per-field provenance inspector.
- SQLite stores canonical metadata, provider-specific query/page membership and
  indexer snapshots. Saved exact queries/pages can be reopened; they are not
  arbitrary offline full-text discovery over all cached titles.
- **V6** is the normalized-cache retirement baseline (twelve `STRICT` tables;
  seven obsolete provider-cache tables removed). **Current source is v7** with
  fifteen `STRICT` tables, including personal records. Supported Rust schema
  upgrades are distinct from migration of the historical C++ application.

**Source:** [metadata adapters](../crates/kinema-desktop/src/metadata_providers/),
[catalog worker](../crates/kinema-desktop/src/catalog_worker.rs),
[version/migrations](../crates/kinema-desktop/src/catalog.rs),
[DDL](../crates/kinema-desktop/src/catalog_schema.rs).
**Limits:** [schema and migration boundaries](catalog-schema.md),
[cache policy](cache-policy.md). Live authenticated-provider acceptance and automatic
TTL-refresh policy must not be inferred from cached fixtures or source wiring.

## Sources and debrid

- Torrentio or Peerflix is selected explicitly, not federated. IMDb movie/episode
  targets drive public noncredentialed indexer requests. Parsed quality, size and
  seed counts are unverified hints; unsupported direct-URL-only rows are omitted.
- Real-Debrid and AllDebrid adapters implement account checks, preparation,
  provider file catalogs and link resolution with bounded requests/polling and
  cancellation. Playback requires an unambiguous file selection; it does not
  guess the largest file or silently fall back to local torrenting.
- Service, transport, malformed-response and cancellation failures are distinct
  from successful empty results. Instant availability is not established by
  account presence or a successful credential check.

**Source/evidence:** [indexers and parsing tests](../crates/kinema-desktop/src/sources.rs),
[selection policies](../crates/kinema-core/src/sources.rs),
[debrid adapters and endpoint contract](../crates/kinema-desktop/src/providers/README.md),
[preparation coordinator](../crates/kinema-desktop/src/app.rs).
Fixture/public-endpoint evidence is not authenticated debrid/playback acceptance.

## Managed media and playback

- Managed remote acquisition persists validated byte ranges and stable file
  identity, not authorized upstream URLs. A loopback-only HTTP server serves
  leased remote assets and existing local files, including single byte ranges.
  Local originals are opened in place, outside managed quota and deletion.
- Cache admission uses a configurable budget (20 GiB default); manual cleanup
  protects active assets. Streaming is not proof of complete offline retention.
  Upstream acquisition requires exact range responses; no full-download HTTP 200
  fallback, LAN serving or active eviction is implemented.
- Embedded MPV is the default, in a separate managed GPUI window with controls
  and lease ownership. The default `gpu-surface` feature selects its sole renderer;
  there is no automatic renderer/external-player fallback. Native libmpv must be
  supplied separately. External mpv/VLC remain explicit process hand-offs, not
  tracked playback completion.

**Source/evidence:** [media contract and tests](../crates/kinema-desktop/src/media/README.md),
[player choice/launch](../crates/kinema-desktop/src/player.rs),
[player window](../crates/kinema-desktop/src/screens/player_window.rs),
[native backend](../crates/kinema-mpv/README.md),
[default features](../crates/kinema-desktop/Cargo.toml).
**Limits:** [player contract](design/player.md), [storage policy](storage-layout.md).
Default-on and scoped Linux evidence do not qualify HDR, Windows/macOS runtime
support, sustained performance or complete live-provider workflows.

## Personal state and credentials

- Personal Library membership is independent of manual movie/episode watched
  state. Local Library queries and conservative series summaries use canonical
  records; missing/partial episode listings never imply a fully watched show.
  Removing membership does not erase watch state. Personal rows and referenced
  entities are protected from cache cleanup, but deleting the database loses them.
- Settings and credential actions use OS secure storage, with missing,
  unavailable and denied states; secrets do not belong in settings, catalog rows,
  media manifests or logs. No historical C++ settings/database/keyring migration.

**Source/evidence:** [personal storage](../crates/kinema-desktop/src/catalog_library.rs),
[queries](../crates/kinema-desktop/src/catalog_library_query.rs),
[storage tests](../crates/kinema-desktop/src/catalog_library_tests.rs),
[shared personal state](../crates/kinema-desktop/src/screens/coordinator/personal.rs),
[credentials](../crates/kinema-desktop/src/credentials.rs),
[settings](../crates/kinema-desktop/src/settings.rs).
**Contract:** [Library/manual watch tracking](design/library.md).

## Not established or deferred

No full MVP, end-to-end live-provider playback or all-desktop runtime acceptance
is claimed here. Manual watched state is not viewing history, persistent playback
resume or automatic completion tracking. Built-in torrenting, download
pause/prefetch suspension and subtitle/Up Next features
must not be inferred from this overview. See [architecture](architecture.md) for
scope and [validation status](development.md#validation-status-and-limits) for
evidence boundaries.
