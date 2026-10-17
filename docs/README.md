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
| [Desktop design system](design/design-system.md) | Authoritative design proposal for the fixed-rail desktop; review pending, not an implemented GPUI theme |
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
with canonical metadata/indexer reads and writes. Current post-fix root fmt,
strict locked workspace Clippy, locked tests (295 passed, 3 ignored), and diff
checks passed. Post-v4 authenticated-provider/debrid and new GUI acceptance remain
unverified; earlier user-reported successes remain historical. No user database/credentials were
accessed; databases upgrade on next open with the new build. Retained legacy rows
have [conservative backfill limits](catalog-schema.md#conservative-v3-preservation-and-backfill-limits),
not a guarantee that every old query/detail/source snapshot is reusable offline.

The user chose a **permanent left icon-only rail**: expanded-sidebar and top-
navigation modes are superseded, not remaining choices. The six-area UI scope
stays unchanged. The [design system](design/design-system.md) is the authoritative
**design proposal**, pending review: professional neutral Graphite with blue
accents (not amber), compact desktop spacing, a native font stack, polished
primitive controls and composable GPUI components, visible IMDb ratings on cards,
hover/focus states, and purposeful motion. Lazy loading and performance are design
requirements, not verified runtime performance claims.

The **Graphite v2** [HTML artifact](design/desktop-wireframes.html),
[design-system proposal](design/design-system.md), and review guide are frozen for
user review, not production implementation approval. The artifact is review-ready: fixed 56 px rail, native font stack, compact controls, visible IMDb
ratings (including valid zero and unknown states), bounded hover/focus facts,
shared card footer, a components gallery, and a 1,000-fixture performance preview.
See the [review guide](design/README.md) for browser evidence and limitations;
those checks do not measure or implement native GPUI performance. Next, review the
[design-system proposal](design/design-system.md), cards, and motion before GPUI
visual implementation; navigation selection is complete. No GPUI redesign/theme is implemented. Existing GPUI projections remain
narrow and do not render per-field origins or all rich domain metadata. Existing
search/detail/sources cancellation wiring was updated; the GPUI layout is unchanged.

The cache reopens saved exact-query/page membership and normalized detail/episode
and source snapshots. It is not arbitrary offline full-text search across all
cached titles. Canonical rows are the reported current source read model, not
proof of full rich/provenance UI acceptance. Start with the
[quickstart](development.md#quickstart-and-setup) and
[validation matrix](development.md#validation-status-and-limits).

Keep the design small. Add structures when a feature needs them, rather than building a framework for possible future work.

Next starts with fresh application data. Do not migrate the existing `~/.config/kinemarc`, `kinema.db`, or keyring entries.
