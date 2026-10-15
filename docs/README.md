# Kinema Next

Design notes for a Rust + GPUI Kit rewrite. The Rust workspace is an early scaffold, not a complete MVP. The [archived C++ source](../reference/cpp/README.md) remains a reference, not a template to port line by line.

**The goal:** browse movies and series, find sources through Torrentio/Peerflix, acquire media through debrid, and serve it from local storage to external VLC/mpv.

## Start here

| Document | What it covers |
| --- | --- |
| [Development](development.md) | Toolchain, OS prerequisites, running, and validation |
| [Architecture](architecture.md) | Agreed scope, system diagram, responsibilities, and desktop support |
| [Catalog and database](catalog-schema.md) | SQLite ER diagram and local-first sync |
| [Storage and media server](storage-layout.md) | File locations, streaming, cache limits, and recovery |
| [Caching](cache-policy.md) | What is cached, expiry, timestamps, and force-refresh |
| [Existing features](features.md) | Useful behavior and limitations in the C++ application |
| [Open decisions](decisions.md) | The remaining choices, discussed one at a time |

## Reading the design

**Agreed** means a direction confirmed in discussion. **Proposed** means an implementation choice still open to change. The scope in the architecture is agreed; the schema and file layout are proposals.

Keep the design small. Add structures when a feature needs them, rather than building a framework for possible future work.

Next starts with fresh application data. Do not migrate the existing `~/.config/kinemarc`, `kinema.db`, or keyring entries.
