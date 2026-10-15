# Kinema Next

Design notes for a Rust + GPUI Kit rewrite. The Rust workspace implements a first
bounded catalog foundation with additional MVP source flows now composed; full
GUI/live-provider acceptance is not complete, although final locked root checks passed. The [archived C++ source](../reference/cpp/README.md) remains a reference, not a template to port line by line.

**The goal:** browse movies and series, find sources through Torrentio/Peerflix, acquire media through debrid, and serve it from local storage to external VLC/mpv.

## Start here

| Document | What it covers |
| --- | --- |
| [Development](development.md) | Quickstart, secure-token setup, prerequisites, and validation matrix |
| [Next-lead handoff](handoff-next-lead.md) | Frozen-batch scope, code map, evidence, and finalization risks |
| [Architecture](architecture.md) | Agreed scope, system diagram, responsibilities, and desktop support |
| [Catalog and database](catalog-schema.md) | Actual version-3 provider caches versus proposed normalized catalog/sync |
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

The cache reopens saved exact-query/page membership and normalized detail/episode
and source snapshots. It is not arbitrary offline full-text search across all
cached titles or the proposed normalized sync engine. Start with the
[quickstart](development.md#quickstart-and-setup) and
[validation matrix](development.md#validation-status-and-limits).

Keep the design small. Add structures when a feature needs them, rather than building a framework for possible future work.

Next starts with fresh application data. Do not migrate the existing `~/.config/kinemarc`, `kinema.db`, or keyring entries.
