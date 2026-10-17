> **Historical v2 evidence — superseded.** Use [desktop v3](desktop-v3.md)
> and the [native lab](ui-probe.md). Kit APIs and old probe commands below are
> retained only as historical evidence; they are no longer supported.

# Native Graphite implementation

## Status and evidence boundaries

The user **approved native Graphite implementation** and lead-owned commits.
This is an implemented native increment, not completion of the MVP. Rust remains
**1.97.1**, edition 2024; presentation uses official **gpui-kit =0.7.3** reexports,
components, assets, `init` and `open_window`, not a second GPUI engine.

- [Design system](design-system.md): visual/state contract and remaining acceptance
  targets. Approval to implement is no longer an outstanding gate.
- [Frozen HTML reference](README.md#frozen-v2-evidence): unchanged design-only
  artifact; its 159 browser checks and mock performance counters are not native
  implementation or performance evidence.
- [UI probe](ui-probe.md): isolated native input/pixel feedback. Stable WASM remains
  blocked by the upstream unstable feature; no browser runtime pass is claimed.
- Backend checkpoint **`f849f9b`** retains the approved
  [v4 catalog contract](../catalog-schema.md#target-catalog-schema). Historical
  **295 passed / 3 ignored** root evidence predates this native increment and must
  not be presented as its final test total.

Final serialized checks and lead-reviewed offline captures passed on 2026-10-17;
see [final validation](#final-validation-evidence). Commits so far: **`7939f10`**
(shared foundation/artwork), **`a0197da`** (failed-art retry), **`8f620c3`**
(production UI/metadata), and **`acc6439`** (modal scrim/mono fixes). Gallery
verification tooling and documentation are checkpointed with these milestones. Earlier
`/tmp/kinema-graphite-native-{dark,light,hover}` captures are superseded by the final
captures below; the first-state “IMDb linked” strip was removed before those.

## Component boundaries and implemented behavior

| Boundary | Current responsibility |
| --- | --- |
| `kinema-core/src/catalog.rs` | Pure `CatalogDisplay` facts shared through `Arc`; validated optional IMDb rating, including valid `0.0`. No GUI/provider DTO dependency. |
| `kinema-desktop/src/design/` | Dark/light semantic tokens mapped into Kit's theme, one installed native font family (or Kit fallback), compact Kit button presets, IMDb badge, fixed-geometry `MediaCard`, pure grid sizing. Modal scrims use semantic `Tokens.overlay` (dark 60%, light 40%); native Kit monospace defaults are retained except for a controlled fixture override. |
| `shell.rs`, `ui/catalog_view.rs` | Permanent 56 px icon rail (Browse, Transfers, Settings, About), compact toolbar/session indicator, Cinemeta grid/list and detail/source composition; row-virtualized grid with collection-owned navigation. |
| `tmdb_view.rs` | Selected-provider discovery/search/detail/episodes, shared cards/grid, retained optional display facts and explicit IMDb-identity gating for source lookup. TMDB vote average is never substituted for IMDb rating. |
| `ui/detail_view.rs`, `ui/episodes.rs`, `ui/source_picker.rs`, `ui/sources.rs`, `ui/strip.rs` | Compact detail/episode/source views and truthful loading, cached/error, missing-mapping and empty states; source filtering/sorting and prepare intents. |
| `settings_view.rs`, `playback_view.rs` | Kit-themed forms and task panels retain settings/keyring, preparation, explicit file choice, local-file entry, sessions and manual cleanup behavior. Styling is not new provider/player acceptance. |
| `artwork.rs` | Application-owned bounded fetch/decode tasks, generations, decoded LRU and renderer image-drop requests; no SQL or provider credentials. |
| `ui_probe.rs` | Native offline gallery using the actual shared theme/buttons/badge/cards/grid metrics, synthetic titles and generated art; separate original counter specimen. Not a second app shell or complete workflow-screen harness. |

Cards keep a 2:3 poster and fixed footer with title, year/type and IMDb score at
rest. Unknown score is `—`, not zero; rating availability and IMDb mapping are
independent. Hover/focus reveals bounded opaque-neutral facts/actions inside the
poster without layout growth or hover-driven metadata enrichment. Missing runtime,
season count, genres or synopsis are not invented. Series actions open episodes;
movie source actions require IMDb identity. Canonical display reads use resolved
metadata payloads and shared display DTOs; retaining a field does not imply a
per-field provenance inspector exists.

### Artwork bounds are implemented constants, not RAM measurements

- **4 global active jobs**, **128 queued jobs**, up to **64 stable owners** with
  owner names at most 128 bytes. Invalidated queued work is removed; obsolete
  running jobs retain their slots until completion and stale results are rejected.
  Explicit route/refresh generation boundaries clear negative cache entries
  globally, permitting retry while preserving ready artwork; no automatic retry
  timer or backoff policy is implemented.
- HTTP(S) artwork keys reject credentials, queries and fragments; URL length is
  capped at 2048 bytes. No redirects or OS proxy credentials; 5 s connect and 15 s
  total timeouts. Encoded response limit: **4 MiB**, enforced while reading.
- Static JPEG/PNG/WebP only; animated PNG/WebP and other formats are rejected.
  Header checks cap each edge at **8192** and total pixels at **8 × 1024 × 1024**;
  decoder allocation limit is **64 MiB**. Decode/thumbnail work is off the UI thread.
- Thumbnail bounds: poster **384 × 576**, backdrop **1024 × 576**, no upscaling.
  Decoded LRU quotas: **48 MiB posters + 16 MiB backdrops**, **128 entries** total,
  including negative results. Evicted images are passed to `cx.drop_image`.

These bound store accounting and work admission, not total process RSS, transient
buffers, mounted `Arc` references or GPU residency. Backdrop support in the store
is not proof of rendered detail backdrops. Source limits and tests are not native
memory-plateau, frame-latency or security-audit evidence. No artwork disk cache,
new TTL, automatic media eviction or new media-quota policy is implied.

## Running normally versus safe fixture verification

**Normal app — user action only, not a safe verification fixture:** from the
repository root:

```sh
cargo run -p kinema-desktop --bin kinema-desktop --locked
```

This uses the user's normal development `./data/` and configured services/OS
secure storage; release mode uses platform data roots. Opening an existing
application database can perform the already-authorized atomic **v4 migration**.
Legacy rows are retained with [conservative backfill limits](../catalog-schema.md#conservative-v3-preservation-and-backfill-limits).
Do not run this command for screenshot automation or inspect private databases,
keyring entries, media or token files. This documentation pass did not run it.

**Safe offline native gallery** — Linux, isolated Xvfb fixture only:

```sh
cargo build -p kinema-desktop --bin ui-probe --features ui-probe --locked
# Optional: public runtime SwiftShader ICD, applied only to the fixture child.
export KINEMA_UI_PROBE_VULKAN_ICD=/path/to/public/runtime/vk_swiftshader_icd.json
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-dark --gallery
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-light --gallery --light
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-hover --gallery --hover 150,200
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-focus --gallery --focus 140,210
```

Use a new artifact directory for each candidate; hover coordinates are fixture
window coordinates, not a general semantic input API. The runner owns its Xvfb,
temporary HOME/XDG roots and fixture window; it must not capture the real app or
private data. See [runner prerequisites and isolation](ui-probe.md#linux-native-pixel-feasibility-fallback).
The gallery uses no database, keyring, network, media or player. Without
`--gallery`, the existing counter specimen and its `--input` checks remain the
default. Direct interactive fixture launch also supports `--gallery --light`:

```sh
cargo run -p kinema-desktop --bin ui-probe --features ui-probe --locked -- --gallery --light
```

## Validation and remaining native acceptance

Root checks remain mandatory; CI also checks/tests the explicit Kit test feature:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo clippy -p kinema-desktop --lib --bin ui-probe --features ui-probe-test --locked -- -D warnings
cargo test -p kinema-desktop --lib --features ui-probe-test --locked
node --test scripts/ui-probe/pixels.test.cjs
```

CI's native matrix targets Linux/macOS/Windows; headless interaction/layout tests
are not screenshot tests or proof that each OS workflow works. Pixel capture is
an additional local Linux fixture check, not an existing cross-platform CI visual
baseline. The final evidence below is separate from historical checkpoints.

### Final validation evidence

Serialized root fmt, strict locked workspace Clippy/tests, explicit fixture Clippy
and tests, and Node PNG/comparator checks **all passed**. Logs:
`/tmp/kinema-graphite-final-validation/{fmt,clippy,tests,probe-clippy,probe-tests,tool-tests}.log`.
The documentation owner inspected log summaries; the lead reports successful exits
again after the modal-scrim regression fix. The extra semantic theme regression
accounts for the final library-test increment.

| Scope | Result |
| --- | --- |
| Locked workspace tests | **331 passed, 3 ignored**: core 26 + desktop library 17 + desktop binary 288 |
| Explicit `--lib --features ui-probe-test` | **20 passed**: 17 overlap workspace tests + 3 actual GPUI interaction/layout tests |
| Node PNG/comparator tests | **5 passed** |

Do **not** sum the explicit feature run into “351 unique” tests. These results
supersede the historical 295-test checkpoint for this increment, not its history.

The lead visually inspected final `native-first-frame.png` captures under
`/tmp/kinema-graphite-final-{dark,light,hover}/` and
`/tmp/kinema-graphite-final-hover/native-hover.png`. A separate lead-reviewed
`/tmp/kinema-graphite-final-focus/native-focused.png` shows the outer focus ring
and disclosure remaining visible after the pointer moves outside all cards.
That capture establishes focused appearance, not initial keyboard-only entry. They show the four-destination
rail, no obsolete IMDb-identity strip, valid `0.0`, unknown scores and failed-art
labels. The opaque hover panel shows supplied runtime/season count, first genres
and summary without height growth. These are isolated Linux Xvfb synthetic
shared-component captures, not screenshots of real application/private data or
complete workflow-screen acceptance, and not portable approved pixel baselines.

The captures remain valid after the modal-scrim fix: shared card rendering was
unchanged, and the corrected modal was not pictured. They do not validate that
modal visually.

The default counter `--input` run in `/tmp/kinema-graphite-final-counter` passed:
stable repeated frames, disabled action unchanged, counter **0 → 1 → 2**, and the
corrupted-pixel negative control. No real app, private data or provider was probed;
no native performance or Windows/macOS GUI testing is claimed. Stable WASM remains
blocked and unchanged.

Remaining gaps/acceptance work in the inspected increment:

- Browse IMDb sorting, density and poster-size controls from the HTML are not implemented;
  Cinemeta's grid/list switch is not parity with every TMDB collection mode.
- Full source-release inspector/provenance UI, cast, all rich ratings and artwork
  backdrops are not supplied by the current narrow detail projections.
- The gallery exercises shared actual components, **not entire workflow screens**:
  settings, source selection, preparation, transfers, dialogs, local-file choice,
  quit protection and provider failures still need integrated native acceptance.
- Full keyboard/focus-return/reconciliation behavior, screen-reader semantics,
  all contrast states, narrow windows, DPI/text scaling, native OS fonts, persisted
  appearance preferences and reduced-motion/system integration need verification
  or further implementation; no full design-system conformance is claimed.
- Profile real scrolling, retained pages/cards, decode/RSS/GPU plateau and repeated
  navigation; no native 60 Hz, p95 latency or total-memory guarantee exists.
- Authenticated providers/debrid, actual external playback and OS support remain
  separate user-authorized acceptance. Do not perform real-app/provider/private-data
  probes as part of this fixture/documentation work. No library/history/resume,
  embedded player, new provider, torrent engine or LAN scope is introduced.
