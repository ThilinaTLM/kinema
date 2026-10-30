# Kinema documentation

The Rust workspace is Kinema's sole implementation: direct GPUI
(`gpui-pre =0.3.9`) and `gpui_platform`, with in-house widgets. **Desktop v3 is the
sole/default UI**; GPUI Kit and Graphite v2 are historical designs, not runtime
alternatives. There is no supported C++ implementation or feature-parity promise.

## Start here

| Document | What it covers |
| --- | --- |
| [Current implementation](features.md) | Rust source overview, evidence links and limitations |
| [Development](development.md) | Rust 1.97.1 setup, native prerequisites, secure credentials, checks and scoped validation evidence |
| [Architecture](architecture.md) | Responsibilities, approved scope, implemented boundaries and deferred work |
| [Desktop v3](design/desktop-v3.md) | Direct-GPUI UI contract and subsequent approved revisions |
| [Embedded player](design/player.md) | Managed player window, controls, session ownership and acceptance limits |
| [Player shortcuts](design/player-shortcuts.md) | Keyboard/mouse controls, Settings reference, implementation contract and scoped validation |
| [Library and manual watch state](design/library.md) | Personal membership, manual tracking and conservative episode summaries |
| [Catalog and database](catalog-schema.md) | Normalized catalog, v6 cache retirement and current v7 personal records |
| [Storage and media server](storage-layout.md) | Managed media, loopback serving, quota, cleanup and recovery |
| [Caching](cache-policy.md) | Saved provider results, artwork, freshness and refresh policies |
| [Native UI probe](design/ui-probe.md) | Isolated fixture/capture tooling and its evidence limits |
| [GPU qualification record](validation/gpu-surface-qualification.md) | Retired harness evidence and its limits; not current build instructions |
| [Vendor patch documentation](../vendor/patches/README.md) | Required GPUI/HAL patch APIs, synchronization and maintenance |
| [Open decisions](decisions.md) | Unresolved choices; not implemented behavior |

## Reading status and evidence

**Implemented** means present in source, not completed MVP or verified runtime
support. **Approved** describes a design decision; **proposed** and open decisions
are not implementation promises. Use the [source overview](features.md) alongside
the [validation matrix](development.md#validation-status-and-limits), rather than
using historical test counts as evidence for the current checkout.

The catalog's **v6 baseline** retires the seven obsolete provider-cache tables and
retains the twelve normalized `STRICT` tables. Current Rust source declares
**schema v7**, adding three personal Library/watch-state tables; see the
[schema specification](catalog-schema.md) and
[version/migration source](../crates/kinema-desktop/src/catalog.rs). Provider page
membership is distinct from personal Library membership. Saved provider queries
are not arbitrary offline full-text discovery. Personal records share the catalog
database: deleting it loses those records.

Embedded MPV uses the sole/default **GPU-surface** path through the default
`gpu-surface` Cargo feature; external mpv and VLC remain alternatives. Native
libmpv is a separate prerequisite. Default-on does not qualify live-provider
playback, full GUI workflows, Windows/macOS runtime behavior or HDR. Follow the
[current root build instructions](development.md#run-and-validate), not a retired
experimental wrapper.

Superseded Graphite v2 designs and development handoffs have been removed.
Retained HTML previews are design artifacts, not runtime implementations;
validation records establish only their stated scope, not current acceptance. No legacy C++ `~/.config/kinemarc`, `kinema.db` or keyring-entry
migration is supported.
