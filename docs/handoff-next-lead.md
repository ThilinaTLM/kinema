# Next-lead handoff

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

This handoff describes code revision `107317a`; the separate documentation commit
need not reference its own hash. Preserve ignored data and require explicit lead
assignment for subsequent commits.

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
The current SQLite version is 3 with provider-specific caches, **not** the
proposed normalized seven-table catalog model; see [schema](catalog-schema.md).
No signed access URLs or credentials belong in source snapshots/manifests/logs.

## Setup and evidence

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
- **New high-priority next-lead bug:** `Spider-Man: Brand New Day` movie details
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

## Next lead: starting priorities

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
