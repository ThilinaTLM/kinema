# Kinema Next

Design notes for a Rust + GPUI Kit rewrite. The Rust workspace implements a first
bounded Cinemeta search increment, not a complete local catalog or MVP. The [archived C++ source](../reference/cpp/README.md) remains a reference, not a template to port line by line.

**The goal:** browse movies and series, find sources through Torrentio/Peerflix, acquire media through debrid, and serve it from local storage to external VLC/mpv.

## Start here

| Document | What it covers |
| --- | --- |
| [Development](development.md) | Toolchain, OS prerequisites, running, and validation |
| [Architecture](architecture.md) | Agreed scope, system diagram, responsibilities, and desktop support |
| [Catalog and database](catalog-schema.md) | Actual three-table search cache versus proposed seven-table catalog/sync |
| [Storage and media server](storage-layout.md) | File locations, streaming, cache limits, and recovery |
| [Caching](cache-policy.md) | What is cached, expiry, timestamps, and force-refresh |
| [Existing features](features.md) | Useful behavior and limitations in the C++ application |
| [Open decisions](decisions.md) | The remaining choices, discussed one at a time |

## Reading the design

**Implemented** describes observed source, not verified tests, GUI runtime, or
comprehensive live-provider support. **Agreed** means a direction confirmed in
discussion. **Proposed** means an implementation choice still open to change.
The architecture's MVP scope is agreed; the actual cache/schema and build-mode
data locations are labeled separately from the fuller proposed schema and file layout.

The current cache reopens saved membership for the exact Cinemeta search query.
It is not arbitrary offline text search across all cached titles, discovery/browse
collections, title-detail sync, or episode sync.

Keep the design small. Add structures when a feature needs them, rather than building a framework for possible future work.

Next starts with fresh application data. Do not migrate the existing `~/.config/kinemarc`, `kinema.db`, or keyring entries.
