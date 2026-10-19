# Open decisions

The [architecture](architecture.md) is the single summary of agreed scope. The
[catalog schema](catalog-schema.md#target-catalog-schema) is the single authoritative
approved target specification for the normalized catalog. Backend evidence reports
v4 source implementation and atomic conservative upgrade: twelve normalized
`STRICT` tables plus seven retained legacy tables. Current post-fix root checks
passed (295 tests passed, 3 ignored); no actual user-database upgrade is claimed. Other implementation notes, proposed
file layout, and open policies remain distinct. Discuss questions **one at a time**;
don't turn this list into an implementation backlog.

## 2026-10-20 — desktop v3 visual-language revision (approved)

The user supersedes v3's violet accent, zero-radius geometry, flat/no-shadow
rule and Detail-only backdrop restriction with the [v3 visual language](design/desktop-v3.md#2-visual-language):
warm amber/coral brand (`#FF7A45` Dark, AA-safe darker Light), artwork-derived
Detail washes/secondary accents without recoloring brand focus or primary actions,
4/6/8/12 px radii, subtle borders and soft shadows only for raised layers.
Compact desktop density stays. Smooth hover lift, crossfades and overshoot-free
sheet/popup settle must honor reduced motion. Browse targets a blurred, darkened
title-responsive crossfading backdrop; Detail targets a full-bleed hero.

The token/motion foundation and contrast tests are implemented in `widgets`;
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
The user settled navigation: a **permanent left icon-only rail**, with no expanded
or top-navigation mode. The professional neutral Graphite/blue direction replaces
amber and calls for compact desktop spacing, native fonts, polished primitive
controls/composable GPUI components, visible IMDb card ratings, hover/focus states,
purposeful motion, and lazy loading/performance. These are design requirements,
not implemented styling or measured performance.

The [design system](design/design-system.md) is the authoritative **design proposal**.
Review its detailed controls, cards, and motion together with the aligned prototype
before GPUI visual implementation. Navigation is no longer an open choice; broader
visual details and full rich/provenance UI presentation are not yet accepted.

The user additionally approved [narrow automatic transactional reconciliation](catalog-schema.md#approved-narrow-identity-reconciliation)
when explicit selected-provider IMDb mapping proves duplicates with no contradictory
IDs or season/episode coordinates. Preserve metadata/references and abort conflicts;
this is identity reconciliation, not cache cleanup. Backend reports graph
reconciliation and rollback coverage, with current root checks passing. Post-v4
authenticated-provider/debrid and new GUI acceptance remain unverified.

The **Graphite v2** [HTML artifact](design/desktop-wireframes.html) is frozen and
review-ready; [review guidance](design/README.md) separates its browser checks
from unmeasured/unimplemented native GPUI performance. The design-system proposal
and guide are frozen after token/alias alignment and API peer review. User review
of the proposal, cards, and motion remains pending, not navigation selection. Selected-provider-only fetching remains
required: approval does not add both-provider enrichment, automatic fallback,
automatic TTLs, or a full-catalog crawler.

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
| Metadata | Automatic refresh intervals, multi-locale behavior, and additional free providers with suitable terms; selected-provider-only fetching with no automatic fallback or both-provider enrichment is [agreed](architecture.md#agreed-metadata-provider-contract) |
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
