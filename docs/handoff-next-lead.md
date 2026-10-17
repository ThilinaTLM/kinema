# Next-lead handoff

<a id="current-next-action-html-prototype-navigation-review"></a>

<a id="current-next-action-design-system-cards-and-motion-review"></a>

<a id="current-next-action-finalize-native-graphite-evidence-and-acceptance"></a>

<a id="current-next-action-lead-owned-native-commit-and-remaining-acceptance"></a>

## Current next action: native workflow and performance acceptance

The user **approved native Graphite implementation and lead-owned commits**.
Navigation remains the permanent 56 px left icon-only rail. Rust **1.97.1** and
official **gpui-kit =0.7.3** remain mandatory; no second GPUI engine. The backend
checkpoint is **`f849f9b`**. Parallel native implementation is being integrated;
see [native implementation status](design/native-implementation.md) for the code
map, remaining functional gaps, normal-app warning and safe fixture commands.

The [design system](design/design-system.md) is the approved direction/acceptance
contract. The [frozen HTML](design/desktop-wireframes.html) and
[review guide](design/README.md) remain design-reference evidence, not the runtime.
Shared native themes, compact controls, cards and bounded artwork are implemented;
not every planned workflow/inspector, appearance control or accessibility behavior
is complete. Approval to implement is **not** an outstanding gate.

**Final evidence frozen:** serialized fmt, strict root/scoped fixture Clippy and
locked tests passed. Workspace: **331 passed / 3 ignored** (core 26 + desktop
library 17 + desktop binary 288). Explicit feature: **20 passed**, consisting of
17 overlapping library tests and 3 actual GPUI tests; **not 351 unique**. Node:
**5 passed**. The serialized checks passed again after the semantic modal-scrim
fix (dark 60% / light 40%) and preservation of native Kit monospace defaults
except the controlled fixture override. Existing card captures remain valid;
the corrected modal is not pictured. Logs: `/tmp/kinema-graphite-final-validation/{fmt,clippy,tests,probe-clippy,probe-tests,tool-tests}.log`.

The lead visually inspected final `native-first-frame.png` in
`/tmp/kinema-graphite-final-{dark,light,hover}` and hover `native-hover.png`:
no old IMDb-identity strip, four rail destinations, genuine zero/unknown/failed-art
states and opaque runtime/season/genre/summary hover without height growth.
`/tmp/kinema-graphite-final-counter` default `--input` passed stable frames,
disabled stability, **0 → 1 → 2** and the negative control. Earlier gallery
captures are superseded; the **295 passed / 3 ignored** backend checkpoint below
remains historical. No real-app/private-data/provider, native performance or
Windows/macOS GUI testing occurred; stable WASM remains blocked.

Commits so far: **`7939f10`** foundation/artwork and **`a0197da`** failed-art retry.
Production UI/metadata is committed as **`8f620c3`** and modal scrim/mono fixes
as **`acc6439`**. Gallery tooling and documentation are checkpointed alongside
these source milestones. Remaining gaps include
Browse sorting/density, full source inspector, cast and actual native workflow
acceptance; they are not silently promised by fixture success. Full details:
[final native evidence](design/native-implementation.md#final-validation-evidence).

The lead has checkpointed the source and documentation in scoped commits.
Keep subsequent work similarly scoped. Do not inspect user
databases/keyring/media/token files or run real-app/provider probes. The safe
native gallery uses actual shared components with synthetic data and no SQL, not
entire workflow screens. Existing authorized v4 migration occurs on normal user
app database open; no real user migration is claimed here.

### Historical backend and design checkpoint evidence

The following freeze/results describe earlier checkpoints, not the final native
Graphite increment. Their former approval-to-implement gate is superseded above.
The reviewed [catalog specification](catalog-schema.md#target-catalog-schema) and
approved schema/reconciliation behavior remain unchanged.

### Graphite v2 HTML freeze evidence (design only)

Reported frozen HTML SHA-256:
`c686a3f290c40faad42ebbc1402dcea95c295c553dd1d22ff93846e80b875dd7`.
The HTML owner reports six browser scripts with **159 checks** passing and no
errors. The lead independently reran an offline Chromium subset on this final
hash, confirming fixed-rail behavior, card/focus/rating states, and bounded mock
stress behavior without page errors. The system author froze the spec/guide after
**48 color assignments**, **64 CSS aliases**, and API peer approval. Detailed browser
metrics and limits live in the [review guide](design/README.md); they do not measure
native GPUI timing, memory, decoding, image caching, or production cancellation.
At that HTML checkpoint, no native GPUI performance or theme implementation was
claimed. Native implementation was subsequently approved as recorded above;
HTML evidence remains historical. Superseded layout variants are not choices.

The user additionally approved narrow automatic transactional identity
reconciliation when explicit selected-provider IMDb mapping proves duplicates
without contradictory IDs or season/episode coordinates. Follow the
[authoritative exception](catalog-schema.md#approved-narrow-identity-reconciliation):
preserve metadata, page/count/FK/provenance references and abort contradictions.
This is not name/year merging or cache cleanup. Backend-owner evidence now reports
graph reconciliation and contradiction rollback coverage; current post-fix root
checks passed. Post-v4 authenticated-provider/debrid and new GUI acceptance remain
unverified, regardless of historical user-reported success on earlier versions.

**Current source status (backend-owner evidence): `SCHEMA_VERSION = 4`.** The
atomic upgrade creates twelve normalized `STRICT` tables, conservatively backfills
supported data, retains seven legacy tables (nineteen application tables total),
and advances the version. Metadata/indexer paths read/write canonical rows; the
rich domain API includes artwork, ratings, IDs, seasons/episodes, and provenance.
Existing GPUI projections remain narrow and do not render per-field origins;
normalized source implementation is not full target UI acceptance.

Reported targeted results: catalog/worker **97 passed**, core **3 passed**, TMDB
**8 passed**, SQL **8 passed**. These are separate reported scopes, not a workspace
total. Shell search/provider-detail cancellation fixes are done, with **6 shell
and 10 state tests** plus targeted strict Clippy passing. GUI acceptance is separate.

### Intermediate validation history (superseded by final rerun)

A **pre-streaming-fix** root run passed fmt, strict locked workspace Clippy, and
locked tests (**291 passed, 3 ignored**: 25 core, 266 desktop). Actual-client
Cinemeta smoke in a temporary v4 database passed browse (48), search (27), movie
and Spider-Man movie details, and series (128 episodes); a no-auth TMDB probe
returned expected HTTP 401 in 5.67 seconds. The lead inspected Clippy/test results
and live-probe evidence in `/tmp/kinema-final-root-lk7f16/`.

**This run was not final:** the lead then found whole-legacy-table payload
`Vec` materialization and authorized a bounded-memory streaming-backfill fix.
The correction is now **frozen**: single-row keyset batches bound legacy-backfill
memory while preserving TMDB dependency order and savepoint rollback. Four focused
regressions passed, the catalog scope passed **63 tests, 1 ignored**, and strict
package Clippy/fmt passed. The lead also compared all twelve ERD entity field sets
with actual DDL in fresh in-memory SQLite: **zero column mismatches**. No user
database was accessed. Those intermediate results did not validate later changes;
the post-correction root rerun below is the final evidence.

### Final current-source evidence and freeze

After bounded streaming backfill and the field-quality fix, root fmt, strict
locked workspace Clippy, locked workspace tests, and diff checks **all passed**:
**295 passed, 3 ignored** (25 core, 270 desktop). Actual-client public Cinemeta
smoke in a temporary v4 database passed browse (48), search (27), Matrix details,
Spider-Man details without `videos`, and series (128 episodes). The no-auth TMDB
probe returned expected HTTP 401 in **5.7815 seconds**; this is transport evidence,
not authenticated-provider acceptance.

Logs: `/tmp/kinema-final-root-rerun-aqcCLD/outcomes.txt` and
`{fmt,clippy,test,diff,live-cinemeta,live-tmdb}.log` in that directory. The lead
inspected outcomes, Clippy, test-result lines, and both live logs; the documentation
owner also inspected those bounded results. No user database or tokens were
accessed; no actual user-database migration is claimed. No commits were made.

The source/documentation batch is frozen, not a declaration of complete MVP.
At that historical checkpoint, search/detail/sources cancellation wiring changed,
but GPUI layout did not. Native layout work is now tracked separately.
The three-navigation-mode prototype/browser evidence belonged to the preceding
design comparison, now **superseded by the permanent left icon-only rail decision**.
It is not validation of the new Graphite/blue proposal. Native Graphite implementation is now approved; use the
[native record](design/native-implementation.md) for remaining acceptance. Post-v4 authenticated-provider, debrid, and new GUI acceptance
remain unverified.

### Preservation and documentation scope

Preservation limits: nonempty legacy Cinemeta exact-query searches remain in old
tables but are not backfilled because shared title-field provenance is ambiguous;
the first canonical query needs refresh/fetch. Known legacy detail fields retain
`Detail` provenance quality; only fetch-resource completeness stays `Summary` to
force richer refetch on demand. Do not label all backfilled values summary-quality. Source targets without verified canonical
mappings stay legacy/unmapped. No user database or credentials were accessed; actual
upgrade occurs only when the user next opens their database with the new build.
See [backfill details](catalog-schema.md#conservative-v3-preservation-and-backfill-limits).

The former design-only alignment assignment is historical. Current native
documentation/implementation scopes and lead-owned finalization are recorded at
the top of this handoff. Preserve other agents' work and existing private data.

The agreed direction is provider-agnostic shared movie/series, season, and episode
metadata with rich descriptions, artwork, external IDs, separate IMDb/TMDB/Rotten
Tomatoes critic/audience ratings, nullable unknowns, and provenance-safe merging.
The local catalog becomes the primary read model, with owned, bounded,
demand-driven selected-provider fetches of missing/new details. Keep metadata
`updated_at` distinct from successful-resource `fetched_at`. This is not automatic
both-provider enrichment, fallback, TTL refresh, or a full-catalog crawler. No
history/library scope is added. ERD approval is complete; the historical
continuation evidence below remains historical and does not establish completion
of the newly authorized normalized catalog or the UI prototype.

## Committed snapshot and continuation scope

The outgoing code batch is frozen, passed final root validation, and is committed
as `107317ab15f31e022891987213e50d64e332a7db`. Documentation is handed off separately.
The next lead should continue the full agreed MVP from that snapshot, including
remaining implementation and acceptance, without expanding beyond agreed scope.
The outgoing freeze does **not** prohibit that continuation. This is not a
declaration of complete MVP or verified desktop/provider support.

Read root `AGENTS.md`, [architecture](architecture.md), and
[development/setup](development.md). For coordinating agents, read the team-lead
skill at `/home/tlm/Projects/nerve/packages/skills/dist/builtin/team-lead/SKILL.md`
(or its installed equivalent). Preserve teammates' edits; assign conflict-free
scopes. Do not recursively spawn agents. Serialize final workspace checks under
one assigned owner when teammates are editing.

Known commits:

- `c7400bb`: cached Cinemeta search and development data-path baseline.
- `65ad31e`: pure core identities, file selection, ranges, and quota policies.
- `107317ab15f31e022891987213e50d64e332a7db` (`107317a`): metadata, debrid,
  settings/keyring, UI and local-playback integration; 45-file feature batch.

The outgoing snapshot sections below describe code revision `107317a` and remain
historical. The historical documentation HEAD was `f1beeb9`; subsequent backend/native
checkpoints are recorded above. The continuation section below is historical. Preserve ignored data
and require explicit lead assignment for subsequent commits.

## Confirmed decisions (not all fully accepted in runtime)

- Both **Cinemeta and TMDB** are required now. Default Cinemeta; explicit metadata
  selection, no automatic provider fallback. TMDB bearer token is user-supplied,
  stored securely; fixed adapter locale `en-US`. Map TMDB titles to IMDb before
  source lookup; missing mapping means clearly unavailable, never guessed.
- Both **Torrentio/Peerflix** and **Real-Debrid/AllDebrid** are required adapters.
  Indexer and debrid selection are separate; source availability is not playback.
- Media server is **loopback only**, no LAN or daemon. External VLC/mpv decode
  media; Kinema serves local URLs, not upstream signed URLs.
- Managed-media quota defaults to **20 GiB**, editable. Manual inactive-only
  cleanup is sufficient; never evict active assets or delete original local files.
  Unknown size waits/refuses until verified; reject lowering quota below reserved
  usage rather than silently deleting files.
- Navigation does not stop sessions. Quit/window-close with active sessions or
  pending playback requires confirmation that the server stops. Player remains
  open; it loses the stream. Verify both close paths in the real UI.
- `debug_assertions` selects development `./data/` relative to **process cwd** for
  all ordinary data. Run Cargo from the repository root. Production uses native
  `ProjectDirs::from("", "", "kinema")` data/config/cache roots. Modes use separate
  ordinary databases; existing root/historical data is not migrated/moved/deleted.
  Root `/data/` is ignored; do not add it or clean user/generated files.
- No history/personal library, embedded playback, torrent engine, or LAN work.
  No automatic TTL adopted. Other open decisions remain in [decisions](decisions.md).

## Code map and integration boundaries

`kinema-core` is pure std/domain code: catalog/details/episodes, source/provider
identities, IMDb and canonical torrent hashes, explicit file selection,
half-open byte ranges and normalized availability, quota admission. Keep GUI,
provider DTOs, credentials, settings, SQL and platform I/O out of core.

`kinema-desktop/src/main.rs` composes modules and GPUI Kit initialization.
Use pinned official GPUI Kit/reexports; do not add another UI framework.

| Modules | Role |
| --- | --- |
| `catalog.rs`, `catalog_metadata.rs`, `catalog_worker.rs` | Owned SQL/catalog worker; local-first saved membership/snapshots; manual refresh; bounded mailboxes and stale-generation rejection |
| `metadata_bridge.rs`, `catalog_state.rs`, `tmdb.rs`, `tmdb_view.rs` | Metadata operations, independent screen state, explicit TMDB numeric identity/IMDb mapping, fixed-locale adapter |
| `sources.rs`, `providers/` | Public indexers, sanitized snapshots, RD/AD preparation, file catalogs and expiring resolved access |
| `app.rs`, `app_tracker.rs`, `app_state.rs` | Bounded application worker, command/event generations, settings/account/preparation/session coordination |
| `media/` | Owned loopback runtime, local-file leases, validated range acquisition, reservation, manifest recovery, inactive cleanup |
| `paths.rs`, `settings.rs`, `credentials.rs`, `player.rs` | Native/dev paths, versioned nonsecret preferences, OS keyring, argument-list native player launch |
| `shell.rs`, `ui/`, `settings_view.rs`, `playback_view.rs` | Reusable themed UI, navigation/search/details/sources, settings actions, preparation and session controls, quit warning |
| `app_assets.rs`, `assets/tmdb-logo.svg` | Official unmodified TMDB attribution asset; provenance/terms in root `NOTICE` |

Blocking SQL/network/filesystem/keyring/process work belongs on owned workers,
not rendering. Request replacement and generation rejection do not cancel an
already blocking HTTP operation; timeout/lifetime limits still matter.
Current backend evidence reports SQLite v4 canonical rows under the
[approved normalized catalog target](catalog-schema.md#target-catalog-schema),
with all seven legacy tables retained and conservative backfill. Current post-fix
root checks passed; this does not establish migration of actual user databases.
No signed access URLs or credentials belong in source snapshots/manifests/logs.

## Outgoing snapshot: setup and historical evidence

Use the [quickstart](development.md#quickstart-and-setup) for metadata, both debrid
providers, player/quota, manual cleanup and quit behavior. Install external players
separately. Linux secure storage needs accessible session D-Bus/Secret Service;
macOS/Windows need their native stores. Missing, denied and unavailable are distinct.
Keyring initialization failures can require restoring access and restarting Kinema.
Development/release data roots do not imply separate keyring accounts.

Settings wires credential presence, **Save token**, **Test connection**, and
confirmed deletion for TMDB/RD/AD. Test checks the saved value, not typed text;
ordinary preference Save is separate. TMDB needs the long API Read Access Token,
not the short API Key. No embedded project key; never send tokens in chat/logs,
command arguments, screenshots or commits. Do not request credential disclosure
as part of handing off. TMDB About/Credits needs the exact notice and approved
less-prominent unmodified logo; inspect runtime rendering, not just asset presence.

Evidence reported to this docs owner (not independently rerun here):

- Final serialized root checks **all passed** on Rust 1.97.1:
  `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`, and `git diff --check`.
  Tests: **244 passed, 2 ignored** (22 core, 222 desktop). The two ignored tests
  are opt-in public-network probes; a machine-specific saved-snapshot probe was
  removed rather than retained as an ignored machine-dependent test.
  Logs: `/tmp/kinema-final-validation.m3MRpY/{tests,clippy,format-check,diff-check}.log`.
  Rust is frozen; these results do not remove the acceptance gaps below.
- Actual `MediaService` local-file HEAD, full-body byte equality and `206` smoke
  passed; revoked lease returned `404`; temporary files were cleaned up.
- Actual `player::launch` with isolated no-config mpv decoded/rendered a synthetic
  two-second MP4 to EOF, exit 0. This is not authenticated debrid playback.
- VLC intentionally not run, to avoid disturbing a user instance.
- Media fixture tests cover recovery, headers, root aliases and quota; accepted
  review fixes landed. Not universal crash/OS proof.
- Actual public indexer fetches returned 97 Torrentio and 21 Peerflix candidates;
  these are point-in-time results, not support guarantees.
- Cinemeta actual-client failure was resolved: measured OS DNS lookup took
  5014 ms, exceeding the former 5-second connect timeout. Connect timeout is now
  10 seconds; total request timeout remains 15 seconds. The actual desktop-client
  live smoke passed browse (48), search (27), movie details, and series details
  (128 episodes). Catalog and worker fixtures passed; no TLS bypass, tokens or
  user-data changes. Subsequent Linux X11 GUI checks exercised popular-page
  scrolling/Load more and Ted Lasso genres/seasons/episodes.
- **Historical bug at the outgoing snapshot (addressed in the uncommitted continuation below):** `Spider-Man: Brand New Day` movie details
  reproducibly show “Cinemeta returned an invalid catalog response” in the latest
  rebuilt GUI, while Torrentio sources for that title work and the Matrix movie
  smoke passes. This is a title-specific malformed-details/parse investigation,
  **not** the resolved DNS timeout. Read-only diagnosis confirmed HTTP 200 from
  `https://v3-cinemeta.strem.io/meta/movie/tt22084616.json` omits `videos`, while
  `DetailsDocument` requires `videos: Vec<EpisodeDocument>`; Serde rejects before
  validation. Matrix supplies `videos: []`. Keep the freeze: the focused next-lead
  fix should accept omitted videos as no episodes **only for movies**, with a
  regression fixture, without weakening malformed-series validation.
- Linux X11 GUI loaded 304 Torrentio sources with sorting/quality/seeder controls;
  Use selected without a debrid key reached a clear Transfers credential-missing
  error. Settings quota `0` disabled Save with validation; fake token input was
  masked and not saved. About showed the original blue-green TMDB logo and exact
  required notice visually. Only the smoke process's own PID was cleaned up.
  Screenshots: `/tmp/kinema-smoke/{t1,t3,t5,t6,s4,s9,u3}.png`; failing movie detail
  screenshots: `/tmp/kinema-smoke/t4.png`, `/tmp/kinema-smoke/u1.png`.
- Still unverified in the UI: TMDB missing-token routing/real-provider flow,
  native file chooser, actual quit dialog and OS-close path, detail cancellation,
  dark/light appearance, and non-Linux runtime. Source/detail cancellation wiring
  exists; cancellation when leaving Cinemeta browse is not forwarded.
- No live TMDB/RD/AD credentials: authenticated providers remain unverified.
  Windows/macOS runtime and full integrated live UI acceptance are not established.

Keep the [validation matrix](development.md#validation-status-and-limits) aligned
with new evidence. Do not convert fixture or subprocess evidence into a claim of
GUI acceptance, paid-provider support, or complete MVP.

## Frozen snapshot status

Code snapshot `107317a` passed final locked root validation and is committed.
The outgoing batch is closed to further Rust edits. The title-specific parse bug
and unverified acceptance are intentional next-lead items, not claims of full MVP
completion. Begin with the priorities below rather than repeating outgoing
finalization or reimplementing already-wired APIs.
Temporary screenshots/logs above are outside Git and may not survive cleanup or
transfer; the recorded observations are the handoff evidence, not portable fixtures.

## Historical outgoing starting priorities

These were the priorities at `107317a`, not the current task list. See the
continuation below for reviewed fixes and remaining acceptance.

1. Read the committed snapshot, final validator/owner reports, and this handoff;
   distinguish already-wired APIs from genuine remaining implementation gaps.
   Do not reimplement settings/app/shell APIs based on stale agent TODOs.
2. **First investigate the title-specific Cinemeta parse bug** for `Spider-Man:
   Brand New Day` (`tt22084616`) details: its HTTP 200 movie payload omits `videos`,
   but `DetailsDocument` requires the vector. Add a movie-without-videos fixture
   and treat omission as no episodes only for movies, preserving strict malformed
   series validation. This is an identified next-lead fix, not a network timeout.
   Then review cancellation on leaving Cinemeta browse and
   verify remaining UI acceptance: native-keyring Settings, TMDB routing, chooser,
   detail cancellation, theme changes, and quit/OS-close session lifetime. Preserve
   users' running applications and private data.
3. Obtain explicit user participation for live TMDB/RD/AD acceptance without
   requesting tokens in chat or logs. Verify paid acquisition/range playback and
   account-change/error safety rather than extrapolating from fixtures/local mpv.
4. Continue remaining work needed for the full **agreed MVP**, including genuine
   implementation gaps found during review, then validate integrated workflows and
   Windows/macOS/Linux runtime behavior. Keep deferred history/library, embedded
   playback, torrent engine and LAN out of scope; do not broaden the product.
5. Keep architecture/status/setup documentation and the validation matrix tied to
   observed code and evidence. Passing checks or a feature commit alone does not
   establish complete MVP acceptance.

## Uncommitted continuation after `f1beeb9`

Historical note: this continuation was originally uncommitted and does not
change the frozen `107317a` evidence above. The user subsequently authorized
commits; these changes were checkpointed as **`f849f9b`** before native UI work.
The lead reported the following Rust changes reviewed:

- Cinemeta movie details may omit `videos`, interpreted as no episodes. Explicit
  null, malformed values, and missing series videos remain rejected. Regression
  fixture and roundtrip tests cover the change; the existing opt-in public smoke
  now includes `tt22084616`. Extending that smoke is not evidence of running it.
- Shell browse work is cancelled on navigation, search, detail entry, and refresh,
  preserving cached values and invalidating the generation. Missing pages reload
  on return. Replaced detail/source work is cancelled. Four targeted shell tests
  passed; this does not establish live GUI cancellation acceptance.

### Historical continuation validation and acceptance priorities (before catalog ERD approval)

1. Corrected continuation validation is reported passing: root fmt, strict locked
   workspace Clippy, locked workspace tests **247 passed, 2 ignored** (22 core,
   225 desktop), and diff check. The lead inspected matching results, Clippy,
   tests, and live-smoke logs in `/tmp/kinema-validation-corrected.kvGbz0/` and
   reran root `git diff --check` successfully after the documentation updates. Initial test-only Clippy `unwrap_used` failures were
   fixed using `?`; initial logs `/tmp/kinema-validation.RohF4U/`. The public
   `live_cinemeta` actual-client smoke passed (1 test): browse (48), search (27), Matrix,
   `tt22084616` with no episodes, and series (128 episodes), using a temporary DB.
   The reported Xvfb/D-Bus GUI smoke used the **old frozen-snapshot binary**,
   so it cannot establish post-fix GUI acceptance. The outgoing 244-pass result
   still applies only to `107317a`.
2. User-assisted post-fix GUI evidence now confirms corrected-title detail
   rendering: the user launched the updated app using the Cargo run instructions
   and reported “Looks okay”. The lead inspected
   `/home/tlm/Projects/nerve/data/storage-4/data/images/image-20261016T172106Z.png`,
   showing Spider-Man: Brand New Day details with synopsis, 2026, Action/Adventure,
   green Fetched status, and last successful fetch 2026-10-16 17:20 UTC, without
   invalid catalog response. The outgoing parse diagnosis is historical, not a
   current known unfixed bug. This establishes corrected-title GUI detail rendering
   only: browse cancellation/navigation and return behavior still need acceptance;
   the user's general report does not establish other provider or GUI flows.
3. Frozen-snapshot GUI smoke in private Xvfb/D-Bus with temporary cwd verified
   window/sidebar navigation and Transfers Open local file chooser opening; Cancel
   returned to unchanged playback screen. The lead viewed the chooser screenshot.
   Screenshots: `/tmp/kinema-ui-accept.dVHRhJ/{initial,settings,chooser-attempt,chooser-cancelled}.png`.
   No file selection, player launch, token UI actions, or token disclosure occurred.
   Isolation used private Xvfb, new D-Bus, redirected HOME/XDG, and temporary cwd,
   not a sandbox or secure-store interception. The portal chooser initially showed
   the repository directory, source unknown; underlying secure-store access is not
   ruled out. Theme toggle was not found, so theme runtime remains unverified. Complete chooser
   selection/error and cross-OS acceptance, native-keyring Settings, TMDB routing,
   theme, and quit/OS-close acceptance while preserving users' running applications
   and private data. This old-binary smoke is not post-fix GUI evidence. The
   follow-up using a custom no-op player fixture was stopped without a rerun
   because isolation was not guaranteed. Local-session, quit, and OS-close remain
   **unverified**; no fixture result is claimed.
4. With explicit user participation, validate authenticated TMDB/RD/AD flows and
   paid acquisition/range playback without requesting credentials in chat/logs.
5. Continue agreed-MVP gaps and integrated Windows/macOS/Linux acceptance. Neither
   this continuation nor passing checks establishes complete MVP, provider, or
   cross-OS support. Keep the development validation matrix aligned with evidence.

### Subsequent TMDB timeout correction (validated; GUI retry pending)

The lead inspected the user's TMDB GUI failure screenshot
`/home/tlm/Projects/nerve/data/storage-4/data/images/image-20261016T172347Z.png`
and Settings Test connection screenshot
`/home/tlm/Projects/nerve/data/storage-4/data/images/image-20261016T172847Z.png`.
Both show network/timeout messages, not credential rejection.

The network agent reproduced system DNS taking 5.016 seconds twice, versus the
prior production reqwest timeout at 5.002 seconds. With a corrected 10-second
connect budget, an unauthenticated TMDB request reached HTTP 401 in 5.807 seconds.
This is transport evidence, not acceptance or rejection of the user's saved token:
the agent did not access that token, and authenticated TMDB was still unverified
at that point (see subsequent user-reported success below).

`tmdb.rs` now shares its client builder between production and tests, retaining
the 15-second total timeout, TLS, and no redirects. A deterministic delayed-DNS
regression and an ignored no-credential live probe were added. Eight focused tests
and targeted Clippy passed. Final root fmt, strict locked workspace Clippy, locked
workspace tests (**248 passed, 3 ignored**: 22 core, 226 desktop), and diff check
all passed for this correction. The credential-free TMDB live probe reached HTTP
401 in 5.598 seconds. Logs: `/tmp/kinema-tmdb-validation-BujUJA/`; the lead inspected
Clippy, test result lines, and the transport log.

The user offered a token file but explicitly prohibited reading it. No agent read,
accessed, or tested that file; no agent-run authenticated acceptance was performed.
The user subsequently confirmed “Yes TMDB worked” in response to whether
post-restart Settings Test connection and Movies browse succeeded. This is
user-reported authenticated check and browse success only, not acceptance of
TMDB details/IMDb mapping or universal provider support. No credentials were
accessed by agents.

### Subsequent user AllDebrid playback failure (investigation pending)

The lead inspected user screenshot `image-20261016T173825Z.png` in the same image
directory recorded above. AllDebrid preparation file listing is visible with a
selected 861.5 MiB file; an external player launched for a local session, but the
UI shows red “Media provider request failed” and 0 B downloaded / 4.4 GiB reserved.
The reservation is aggregate; this does not establish a size mismatch.

User mpv/ffmpeg console logs report the stream ending at byte 0 rather than
903319726, retries, then format not recognized. This confirms zero delivered bytes,
not a codec/file-format root cause. At that point successful paid range playback
was unverified; the exact original stream failure cause is still not proven. Agents did not
access tokens, user data, or the live stream. Do not reproduce local capability
URLs or screenshot filenames of media assets in reports.

Reviewed media fix raises the connect budget from 5 to 10 seconds on this host
with known roughly 5-second DNS, retaining the 20-second total timeout, TLS, and
no redirects. Typed redacted errors distinguish `ConnectTimeout`, `RequestTimeout`,
`BodyRead`, and `RenewalExhausted`, preserving generic `Upstream` elsewhere.
Deterministic delayed-DNS/failure coverage passed 24 media tests; targeted strict
Clippy passed. Final root fmt, strict locked workspace Clippy, locked workspace
tests (**250 passed, 3 ignored**: 22 core, 228 desktop), and diff check all passed
on the media correction. Logs: `/tmp/kinema-media-validation.Il8jwz/`; the lead
inspected Clippy and test result lines and separately passed `git diff --check`.

These are bounded delayed-DNS/error fixtures, not live signed-URL or media
acceptance. On 2026-10-17 the user replied “all good” to the requested post-fix
restart/reprepare/play retry and whether playback starts/downloaded bytes increase.
This supersedes the pending basic AllDebrid retry with user-reported playback and
download progress success only. There is no independently observed byte comparison,
seek, interrupted acquisition, account-change safety, other provider/indexer, or
cross-OS acceptance. The correction remains hypothesis-driven, not proof of the
original stream's cause. No credentials were accessed by agents. TMDB's separate
user-reported check/browse success above does not establish media playback.
The preceding documentation/backend freeze is historical. Native Graphite was
subsequently approved and implemented as recorded at the top of this handoff;
its final isolated evidence is frozen separately from these older provider
reports. The lead owns commits. Actual native workflow and post-v4 authenticated
provider/debrid acceptance remain unverified by the fixture checks.
