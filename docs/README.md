# Kinema Next

Design notes for a Rust + GPUI Kit rewrite. The Rust workspace implements a first
bounded catalog foundation with additional MVP source flows now composed; full
GUI/live-provider acceptance is not complete, although prior baseline locked root
checks passed. The [archived C++ source](../reference/cpp/README.md) remains a reference, not a template to port line by line.

**The goal:** browse movies and series, find sources through Torrentio/Peerflix, acquire media through debrid, and serve it from local storage to external VLC/mpv.

## Start here

| Document | What it covers |
| --- | --- |
| [Development](development.md) | Quickstart, secure-token setup, prerequisites, and validation matrix |
| [Next-lead handoff](handoff-next-lead.md) | Frozen-batch scope, code map, evidence, and finalization risks |
| [Architecture](architecture.md) | Agreed scope, system diagram, responsibilities, and desktop support |
| [Desktop design system](design/design-system.md) | User-approved fixed-rail Graphite visual/state contract and remaining acceptance targets |
| [Native Graphite implementation](design/native-implementation.md) | Implemented components, normal-app warning, safe gallery commands and evidence limits |
| [Catalog and database](catalog-schema.md) | Approved target and reported v4 source implementation, retained legacy data, and conservative backfill limits |
| [Storage and media server](storage-layout.md) | File locations, streaming, cache limits, and recovery |
| [Caching](cache-policy.md) | What is cached, expiry, timestamps, and force-refresh |
| [Existing features](features.md) | Useful behavior and limitations in the C++ application |
| [Open decisions](decisions.md) | The remaining choices, discussed one at a time |

## Reading the design

**Implemented** describes observed source, not verified tests, GUI runtime, or
comprehensive live-provider support. **Agreed** means a direction confirmed in
discussion. **Proposed** means an implementation choice still open to change.
The architecture's MVP scope and reviewed normalized catalog target are approved;
the actual cache/schema and build-mode data locations remain labeled separately
from target behavior and proposed file layout. Backend evidence reports v4 source
implementation: twelve normalized `STRICT` tables and seven retained legacy tables,
with canonical metadata/indexer reads and writes. Historical backend post-fix root fmt,
strict locked workspace Clippy, locked tests (295 passed, 3 ignored), and diff
checks passed. Post-v4 authenticated-provider/debrid and new GUI acceptance remain
unverified; earlier user-reported successes remain historical. No user database/credentials were
accessed; databases upgrade on next open with the new build. Retained legacy rows
have [conservative backfill limits](catalog-schema.md#conservative-v3-preservation-and-backfill-limits),
not a guarantee that every old query/detail/source snapshot is reusable offline.

The user chose a **permanent left icon-only rail** and explicitly approved
native Graphite implementation and lead-owned commits. The six-area scope stays
unchanged. The [design system](design/design-system.md) defines neutral Graphite
with blue accents, compact controls/native fonts and IMDb card semantics. Shared
native themes/cards/buttons, shell and provider-view composition, and bounded
artwork now exist; see [native implementation status](design/native-implementation.md)
for actual boundaries, runnable commands and remaining gaps. Compilation does not
establish workflow, live-provider, OS, accessibility or performance acceptance.

The **Graphite v2** [HTML artifact](design/desktop-wireframes.html) remains a frozen
design reference, not the native runtime. Its 159 browser checks and simulated
1,000-fixture performance preview remain HTML-only evidence. Final offline dark/light/hover captures were visually inspected by the lead;
serialized root checks passed **331 tests / 3 ignored**. Explicit fixture tests
passed **20** (17 overlap + 3 GPUI tests), and Node tests passed **5**; these are not
351 unique tests. See [final evidence](design/native-implementation.md#final-validation-evidence).
The 295-test backend checkpoint remains historical.
No real-app/private-data or provider probes are part of this documentation pass.

The cache reopens saved exact-query/page membership and normalized detail/episode
and source snapshots. It is not arbitrary offline full-text search across all
cached titles. Canonical rows are the reported current source read model, not
proof of full rich/provenance UI acceptance. Start with the
[quickstart](development.md#quickstart-and-setup) and
[validation matrix](development.md#validation-status-and-limits).

Keep the design small. Add structures when a feature needs them, rather than building a framework for possible future work.

Next starts with fresh application data. Do not migrate the existing `~/.config/kinemarc`, `kinema.db`, or keyring entries.
