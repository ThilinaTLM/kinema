# Durable playback progress and desktop integration — validation handoff

**2026-10-28 · implemented source, scoped validation; not full native/provider
qualification.** Rust 1.97.1, edition 2024. This records the implementation of
[the playback-progress design](../design/playback-progress-desktop.md), not a
claim that every proposed/native acceptance gate has passed. Final native fixture
review includes the corrected Streams layout; native OS/runtime gates remain below.

## Implemented architecture

- `kinema-core/src/watch_progress.rs` owns validated integer-millisecond
  observations, the pure 90% completion rule, and most-recent-resumable regular
  episode selection. Existing binary next-up remains the fallback; specials and
  watched replays do not displace it. Presentation formatting remains desktop-side.
- `kinema-desktop/src/catalog_schema.rs`, `catalog_playback_progress.rs`,
  `catalog_library.rs`, `catalog_library_query.rs`, and
  `catalog_normalized_merge.rs` own persistence, canonical identity resolution,
  aggregation and reconciliation. SQLite work stays on the existing background
  catalog owner, not rendering.
- `LibraryOperation::PlaybackProgress` / `LibraryResult::PlaybackProgress` use
  the existing bounded, durable local FIFO. Repository operations are `Read`,
  `BeginRun`, `Save`, `Clear`, and `CompleteRun`; results are `Applied(snapshot)`
  or `Stale(snapshot)`. Snapshots include watched state, epoch, active run,
  suppression and optional checkpoint. Personal movie/episode reads carry these
  snapshots; title summaries expose separate `has_resume`.
- `controllers/coordinator/playback.rs` owns intents, pending acknowledgements,
  one latest coalesced observation per run, sequence/generation rejection and
  retries. `controllers/playback_progress.rs` and the player view (`ui/views/player`) observe the
  independent managed player. Canonical movies/episodes are tracked; unmatched
  files are not persisted by filename/path guesses. External mpv/VLC are unchanged.
- `desktop_integration` implements bounded typed native commands, selected-player
  media routing, all-player inhibition and tray/runtime health. Application-owned
  lifecycle state retains the coordinator independently of Main visibility.
  Native callbacks do not directly call GPUI or mpv. See the
  [adapter contract and limits](../../crates/kinema-desktop/src/desktop_integration/README.md).

## Schema and conflict semantics

The normalized catalog advances from **v7 to v8**, in a migration transaction.
This is a same-catalog extension, not a legacy configuration/database/keyring
migration. Existing watched values/timestamps survive; migration fabricates no
checkpoints.

| Storage | Contract |
| --- | --- |
| `movie_playback_progress` | One row per canonical movie, composite movie-kind FK to `titles` |
| `episode_playback_progress` | One row per canonical episode, FK to `episodes` |
| Both progress tables | STRICT; `position_ms INTEGER >= 0`, nullable `duration_ms INTEGER > 0`, `updated_at`, `revision`; recency indexes; RESTRICT deletion |
| Existing watch tables | Add durable `revision`, `active_run`, and `auto_mark_suppressed`, all with zero defaults |

`BeginRun` increments the durable epoch and authorizes the new run in an
IMMEDIATE write transaction. Every save compares run/epoch in the same transaction
as checkpoint upsert and any watched update. A newer run in another process
revokes the previous writer. The epoch remains in the watch row even without a
checkpoint; a queued old write cannot resurrect explicitly cleared progress.
The checkpoint revision tags its authorization epoch, not a wall-clock winner.
Within a run the coordinator serializes/coalesces samples using sequence numbers.

Mutation timestamps advance past the maxima in **both watch and both progress
tables**, including after wall-clock rollback; overflow is an error. Canonical
alias reconciliation preserves the most recent checkpoint, advances the winner's
epoch beyond both aliases, retags retained progress and revokes old writers before
RESTRICTed loser deletion. Cache-page pruning and Library membership removal do
not remove personal progress.

## Implemented progress behavior

- Checkpoints and watched flags are independent. Observed backward positions
  replace larger positions. Zero is a valid checkpoint, but is not a Resume offer.
  Invalid/nonfinite/negative/overflowing positions are rejected; invalid/unknown
  player duration becomes unknown rather than a fabricated percentage.
- A confirmed position at **90% or later** of valid positive duration marks
  watched atomically with saving progress, unless suppressed. This is current
  position, not cumulative viewing time; confirmed seeks/resumes can qualify.
  EOF is not required. Already-watched flags are not repeatedly timestamped.
- Manual watched/unwatched changes preserve and retag checkpoints and advance
  epoch, including bulk episode mutations. Manual unwatched durably suppresses
  auto-marking for the current run. A new explicit run may qualify again.
- Clear deletes only the checkpoint and advances epoch. Continuing playback can
  save again after a fresh epoch handshake. Old observations/retries are discarded;
  run identity must still match before reauthorization.
- Start-over clears only after the player is ready and its new BeginRun commits.
  Failed/cancelled preparation preserves the old checkpoint. Ready arriving behind
  an in-flight Save is latched. A stale manual-reset response must not bypass the
  pending start-over barrier or lose that Ready intent.
- Resume loads paused and uses observed load/seek confirmation before activating
  persistence/play intent. A tracked progress-read failure **blocks that tracked
  launch with an error and Retry from Transfers**; it does not silently launch at
  zero. This is narrower than the design's optional untracked fallback.
- Unwatched positive-position checkpoints contribute to Library In progress
  counts/filtering **before pagination**, for movies and series. Watched replay
  checkpoints do not. Episode-completion totals remain binary and conservative
  about metadata completeness; elapsed time is not a fractional watched episode.
- Routine samples are throttled to about **five seconds**, with forced transition
  and close/quit samples. Failures retain one latest sample and retry at 1/2/4
  seconds with a bound; forced samples bypass ordinary backoff. Quit has a bounded
  final flush (1.8 seconds) within the shared two-second shutdown gate, with a
  fixed-category warning if incomplete. A crash can lose the latest sampling
  interval; failed writes can lose more. No unsaved retry journal is replayed on
  next startup, and zero-loss crash recovery is not claimed.

## Inspected automated evidence

The lead ran these commands from the repository root with `--locked`. Counts below
were read from the named logs after the final source/layout refinements.
Logs under `/tmp` are local handoff artifacts, not durable repository evidence.

| Command | Observed result / evidence |
| --- | --- |
| `cargo fmt --all -- --check` | Passed on the final source state |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed; inspected `/tmp/kinema-progress-final2-clippy.log` |
| `cargo test --workspace --locked` | Passed; inspected `/tmp/kinema-progress-final2-tests.log`: core **69 passed**; desktop **618 passed, 5 ignored**; mpv **109 passed, 21 ignored** |
| `cargo test -p kinema-desktop --features ui-probe-test --locked` | Passed; inspected `/tmp/kinema-progress-final2-ui-tests.log`: desktop library **724 passed, 5 ignored**; ui-probe binary **5 passed** |

Focused reproduction commands (no real provider credentials or user DB required):

```sh
cargo test -p kinema-core --locked
cargo test -p kinema-desktop --locked catalog::library::tests
cargo test -p kinema-desktop --features ui-probe-test --locked \
  controllers::coordinator::playback::tests
```

Scoped runs also passed **22 catalog-library tests** and **9 coordinator progress
tests**. Coverage includes migration flag preservation/no fabricated checkpoints,
atomic rollback on save/reset failure, cross-process latest-run ownership,
manual suppression and stale resets, backward seeks, unknown/invalid duration,
90% threshold, canonical merges, timestamp rollback/overflow, pre-pagination movie
and series filtering, stale generation/sequence rejection, Ready behind Save,
stale-epoch Ready, old-run retry cleanup, resume-presence transitions and forced
backoff bypass. These are synthetic policy/repository/coordinator fixtures, not
real-provider playback acceptance.

**Separately reported, not independently log-inspected here:** 16 adapter tests
and 4 vendored media-integration tests. Do not add these counts to the inspected
workspace totals or treat them as native OS tests. Dependency unit tests are not
implied by a root workspace test run. The
[vendored souvlaki patch notes](../../vendor/souvlaki/KINEMA-PATCH.md) describe its
isolated test setup and limits.

## UI fixture evidence

The lead personally inspected the movie, series, clear-confirmation and final
Streams progress fixture captures.
Reports/images are under these local paths:

- `target/ui-probe/progress/detail-live-local-movie-resume/`
- `target/ui-probe/progress/detail-live-local-series-resume/`
- `target/ui-probe/progress/detail-live-local-series-resume-clear-1440x800/`
- `target/ui-probe/progress/final-1440x800/`
- `target/ui-probe/progress/final-1360x800/`

Their `report.json` explicitly limits evidence to isolated Xvfb fixture-window
feasibility, not the personal desktop/full application/performance/cross-platform
acceptance. The reports do not themselves prove image review; the inspection claim
above is attributed to the lead. Movie hero Resume remains compact without the
removed movie-detail elapsed bar; series/episode placement follows the agreed
layout. Fixture progress values are synthetic, not recorded viewing data.

The final Streams images show token-based native typography, compact release
metadata and account-presence status, a filename disclosure separated from the
playback section, provider selection above a full-width primary Resume button,
and a quieter Play from beginning action below. Both 1440×800 and 1360×800 fit
without button overlap; the latter is approximately the minimum outer window size
for a 1280-pixel content drawer. Below the content-width threshold, the existing
no-drawer source-list workflow remains. Earlier side-by-side-button captures are
superseded by these final images. Light-theme and external-player-specific captions
were not visually qualified. External handoffs have policy tests preventing a
false Resume offer; they continue to start from the beginning.

Reproduce the native scenes without real accounts/media:

```sh
cargo build -p kinema-desktop --bin ui-probe --features ui-probe --locked
node scripts/ui-probe/native-verify.cjs target/ui-probe/progress/final-1440x800 \
  --lab --scene detail-live-local-series-resume-streams --size 1440x800
node scripts/ui-probe/native-verify.cjs target/ui-probe/progress/final-1360x800 \
  --lab --scene detail-live-local-series-resume-streams --size 1360x800
```

## Isolated Linux media-session protocol smoke

The lead independently ran and inspected:

```sh
cargo test -p kinema-desktop --lib --locked \
  desktop_integration::media_session::linux_smoke::isolated_mpris_session \
  -- --ignored --exact --nocapture
```

Passed in an isolated `dbus-run-session`, without using the user's session bus.
Observed actual MPRIS registration (`Identity=Kinema`), public metadata,
240-second duration, 72.5-second position, volume 0.37, Next/Previous disabled,
Seek disabled then enabled after selected-player change, and Play/Pause callbacks
routed to the expected generations. Empty selection cleared metadata/capabilities;
Drop released the bus name and rejected subsequent calls without post-detach
commands. Evidence: `/tmp/kinema-mpris-smoke.log`. This validates real D-Bus protocol
registration/dispatch, not desktop media-key UX, tray hosting, inhibition or live
video playback. The test is deliberately ignored in ordinary workspace runs.

## Qualification limits and remaining gates

- **Not qualified:** native Windows/macOS behavior; real Linux-shell MPRIS/media
  keys, tray visibility/host loss and inhibition; multiple native players through
  main-close/restore/quit; native permission denial and shell restart recovery.
  Source adapters, compilation and fake-host tests do not establish these claims.
- Wayland per-window hiding is explicitly unsupported in the GPUI visibility patch;
  close uses the visible Minimize / Quit / Cancel fallback rather than pretending
  to hide successfully. A usable fresh tray health lease and a supported hide/
  restore path are prerequisites for close-to-background. Real host-loss recovery
  remains an OS/session acceptance task.
- Automatic idle/display inhibition is distinct from preventing explicit lock,
  explicit suspend or managed power policy; none of those overrides is promised.
- Seekable DVR/live media is **not separately classified**. Do not infer complete
  live/DVR resume semantics from finite-duration/seekable checks.
- No real provider, account, OS secure-storage, remote-media, external-player,
  Windows/macOS playback or HDR acceptance follows from these results. Existing
  [cross-platform qualification limits](cross-platform.md) remain in force.
- Final root format/Clippy/tests and UI-feature suite passed after refinement.
  `python3 -B vendor/verify.py` independently verified 308 GPUI/HAL patched source
  files; the separate souvlaki patch retains its upstream license and patch notes.
  Remaining gates are native desktop/provider qualification, not pending code checks.
