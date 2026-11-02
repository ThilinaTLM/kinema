# Open decisions

The [architecture](architecture.md) is the single summary of agreed scope. The
[catalog schema](catalog-schema.md#target-catalog-schema) is the single authoritative
approved target specification for the normalized catalog. Schema v6 keeps twelve
normalized `STRICT` tables and retires seven obsolete provider-cache tables under
the explicit disposal decision below. No actual user-database upgrade is claimed.
Other implementation notes, proposed file layout, and open policies remain distinct.
Discuss questions **one at a time**; don't turn this list into an implementation backlog.

## 2026-10-24 — catalog simplification and obsolete-cache disposal (approved)

After a read-only review of `data/catalog.sqlite3`, the user authorized retiring
all seven legacy provider-cache tables, explicitly selecting **Discard obsolete
legacy caches** when informed that some records cannot be faithfully transferred.
Old-only saved results may become unavailable offline and require refetching.
Normalized catalog entities, identifiers, ratings, provenance, saved pages, and
indexer snapshots must survive unchanged. V6 retires the legacy tables and advances
the schema version transactionally; fresh databases create only normalized tables.
Tests use synthetic databases; this work does not directly migrate `data/`.

Identifier consolidation was conditional on genuine simplification without weaker
integrity. Review favors retaining the three tables: registry collision domains,
owner FKs, and namespace constraints differ, and application import code already
shares identifier handling. No generic entity table or archive subsystem is added.
See [v6 migration details](catalog-schema.md#legacy-cache-retirement-schema-v6).

## 2026-10-21 — metadata priority and enrichment (approved)

The user replaces selected-provider-only fetching with an ordered provider list
(default Cinemeta, TMDB; reset the old single setting without migration). The
first enabled, active provider owns search/browse; others enrich through a common
capability-declaring interface and normalized catalog observations. Missing
credentials make a provider inactive and skipped, not an error.

Per-field provider priority wins, lower providers fill gaps, and newer observations
win within one provider without summary replacing detail. Explicit external IDs
alone establish identity; ratings stay separate by source/metric/supplier.
Details show the primary result then enrich fully; visible cards enrich only
missing card fields with bounded, cancellable, generation-checked work. Details
attribute actual contributors in a “Data from TMDB and Cinemeta” footer, without
per-field provenance UI. See the [full contract](architecture.md#agreed-metadata-provider-contract).
Implementation is in progress in this change; no live-provider verification is
claimed. Automatic TTLs and full-catalog crawling are not part of this decision.

## 2026-10-20 — desktop v3 visual-language revision (approved)

The user supersedes v3's violet accent, zero-radius geometry, flat/no-shadow
rule and Detail-only backdrop restriction with the [v3 visual language](design/desktop-v3.md#2-visual-language):
warm amber/coral brand (`#FF7A45` Dark, AA-safe darker Light), artwork-derived
Detail washes/secondary accents without recoloring brand focus or primary actions,
4/6/8/12 px radii, subtle borders and soft shadows only for raised layers.
Compact desktop density stays. Smooth hover lift, crossfades and overshoot-free
sheet/popup settle must honor reduced motion. Browse targets a blurred, darkened
title-responsive crossfading backdrop; Detail targets a full-bleed hero.

The token/motion foundation and contrast tests are implemented in `ui/theme` and `ui/motion` (see [design system](design/design-system.md));
backdrop blur/crossfade integration and the broader widget/screen reskin are in
progress, not a claim of accepted rendering, OS support or measured performance.
GPUI currently lacks an OS reduced-motion signal; Settings Always remains the
implemented instant-motion path. This decision changes presentation only, not
provider/session policy. Older Graphite/v2 notes below are historical and are
not current navigation or visual-language authority; v3 is the sole/default UI.

## Historical Graphite/v2 review (superseded by desktop v3)

The user approved the reviewed catalog ERD/details and authorized implementation.
Backend implementation and shell search/provider-detail cancellation fixes are
reported. Current post-backfill/quality-fix root fmt, strict locked workspace
Clippy, locked tests, and diff checks passed. Run history is in
[development](development.md#current-normalized-catalog-batch). Preserve the
[conservative backfill limits](catalog-schema.md#conservative-v3-preservation-and-backfill-limits).
[Desktop v3](design/desktop-v3.md) and its approved revisions now govern the
sole/default UI. Earlier Graphite palette, fixed-rail and Kit proposals are
superseded, not open choices. Full rich/provenance UI presentation and runtime
acceptance are not established by design approval.

The user additionally approved [narrow automatic transactional reconciliation](catalog-schema.md#approved-narrow-identity-reconciliation)
when explicit provider-supplied IMDb mapping proves duplicates with no contradictory
IDs or season/episode coordinates. Preserve metadata/references and abort conflicts;
this is identity reconciliation, not cache cleanup. Backend reports graph
reconciliation and rollback coverage, with current root checks passing. Post-v4
authenticated-provider/debrid and new GUI acceptance remain unverified.

Provider policy is superseded by the 2026-10-21 priority/enrichment decision;
automatic TTLs and full-catalog crawling remain outside that decision.

## Other open media storage behavior

The quota default, manual inactive-only cleanup, unknown-size admission, and
limit-lowering safety rules are agreed in [storage policy](storage-layout.md#limits-and-cleanup).
A custom cache location and checkpoint intervals remain open. Discuss one question
at a time; no additional behavior is resolved here.

## Remaining choices

| Area | Still open |
| --- | --- |
| Media storage | A custom cache location and checkpoint intervals |
| UI design | Implement and visually validate the approved desktop v3 revision; backdrop pipeline and screen redesign remain in progress |
| Metadata | Automatic refresh intervals, multi-locale behavior, and additional providers with suitable terms; ordered priority and bounded enrichment are [decided, implementation in progress](architecture.md#agreed-metadata-provider-contract) |
| Catalog retention | Retention/deletion policy is separately open; response/page, indexer, and media eviction must preserve canonical catalog identities, ratings, and provenance under the [cleanup boundary](cache-policy.md#target-cache-coverage) and authoritative schema rules |
| Response caches | Exact TTLs, retained-entry limits, and offline use of stale results; not authority to delete canonical catalog records |
| Provider integrations | Rollout order for debrid providers, required capabilities, and whether built-in adapters are sufficient |
| External players | Discovery/configuration and reconnect grace; quit/close confirmation and server lifetime are [agreed](architecture.md#agreed-session-lifetime-policy) |
| Desktop releases | Minimum OS versions, architectures, packaging, accessibility, and optional OS integrations |
| Performance | Representative workloads and numerical responsiveness/resource budgets |

## Not MVP work

Personal library/progress, embedded playback, and a built-in torrent engine are planned later. Android/iOS, explicit retained downloads, runtime plugins, transcoding, and rolling caching are not current commitments. LAN sharing is excluded; the MVP server is loopback-only. Leave useful boundaries for future work; do not implement unused subsystems.

## Recording an answer

Update the relevant document with the decision and its reason, then remove the resolved question here. Keep one description of each rule rather than copying it into every page. No migration of the old configuration, database, or keyring is planned.
