# Playback startup and safe logging investigation

## Scope and evidence (2026-10-25)

This integrated investigation covers source inspection, pure credential/domain
checks, synthetic local HTTP fixtures and authorized isolated CPU-native startup
and interruption fixtures, plus four isolated Linux GPU-native backend tests.
The user reported **startup delay and a nonterminal native diagnostic; playback
succeeded**. These fixtures are not real-media/provider acceptance or a
reproduction of that reported delay/diagnostic. Final code validation passed;
root UI runtime validation is **blocked by the current headless environment**,
with failed attempts retained below.

Changes span credential severity and optional TMDB enrichment lookup;
fixed-category desktop startup/icon logging; media reader priority/disconnection
fixes and bounded stage observations; backend native diagnostic classification,
lifecycle timing and generation-safe qualification; and Linux EGL device UUID
prefiltering/discovery summaries. No user credentials, configuration, database,
media or provider were accessed. No OS-keyring probe was run, and no manifests,
lockfiles or vendor files were changed. Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`,
Cargo 1.97.1.

## Credential lookup severity

### Source evidence

- `screens/coordinator.rs`, `AppCoordinator::new`: queues `LoadSettings` during
  initialization, not only when the user opens Settings.
- `app.rs`, `Backend::execute_checked`: `LoadSettings` clears cached settings and
  calls `snapshot`; `snapshot_warning` queries both debrid providers' `status`
  and TMDB `status`. These are optional presence checks on the application
  worker, not credential validation or evidence of playback failure.
- Previously both stores' `status` called `get`, whose shared `diagnose` logged
  every `Missing` as WARN before converting it to `Ok(false)`. Thus a clean
  installation could emit three credential warnings at startup. This is a
  source-derived expectation, not an observed native run.
- Required operations use `get`: debrid client construction through the injected
  `AppIo` credential closure and Settings Test connection (including TMDB).
  Set/delete are explicit mutations. Errors there must not become success.
- Previously `catalog_metadata.rs` also called required TMDB `get` for optional
  enrichment. In `prepare_network`, the `Enrich` branch already mapped
  `TmdbMissingCredential` to an inactive provider while distinguishing service
  failures. This caller now uses `get_optional`. Direct TMDB refresh and
  `A::Tmdb` work retain required `get` semantics.

### Implemented correction

Use a closed operation enum rather than string-based severity policy. Both
`status` methods diagnose their own vault lookup as `Status`, without calling
required `get`:

| Result | Optional `status` / TMDB `get_optional` | Required `get`, `set`, `delete` |
| --- | --- | --- |
| Present/success | DEBUG; presence true / Some(secret) | DEBUG; unchanged success |
| Missing | DEBUG; presence false / None | WARN; unchanged Missing error |
| AccessDenied / Unavailable | WARN; unchanged error | WARN; unchanged error |

Only fixed provider labels, operation labels and typed credential categories are
logged. Secrets and native backend text are never formatted. Existing error
mapping/zeroization and metadata-generation behavior are preserved. This does
not suppress secure-storage service failures or change when startup contacts it.

TMDB enrichment now uses explicit `MetadataCredentialStore::get_optional`,
which returns `Ok(None)` with DEBUG for absence and preserves service errors.
The enrichment caller translates absence back to its existing inactive-provider
category. Direct required TMDB operations retain `get` and WARN/Missing. No
credential caching, provider ordering, focus or acquisition policy is changed.

### Pure regression evidence

`FailedVault` supplies typed Missing/Unavailable/AccessDenied failures without
contacting the OS. A captured-log matrix asserts all three providers' optional
status and optional lookup results, DEBUG versus WARN counts, and fixed operation
labels; Missing must be `Ok(None)`, not `Err(Missing)`, for optional lookup. The
roundtrip checks optional presence before set, after set, and after deletion. All six
required get/set/delete failure paths still WARN. Fake secrets are excluded.
Existing fake roundtrip, metadata generation/namespace, safe backend-error
mapping and diagnostic payload-redaction tests remain.

Final focused credential rerun from the repository root, after the optional
lookup and reader-window corrections:

```sh
cargo test -p kinema-desktop --lib --locked credentials::tests
```

**5 passed, 0 failed, 0 ignored** (488 other tests filtered out). Fake vaults and
temporary settings only; no OS keyring access. Earlier focused strict desktop
Clippy and owned-file rustfmt/diff checks passed. The final combined results
below include all subsequent code corrections. Pure checks are not native/keyring/
provider acceptance.

## Combined validation

**Final lead-reviewed code validation passed after all code changes**, including
the redacted-error classification and test-only accessor corrections:

| Check | Final result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | 42 core + 489 desktop + 108 mpv = **639 passed, 25 ignored** |
| Locked desktop build | PASS |
| Vendor verification | PASS, **307 files** |
| `git diff --check` | PASS |

Each feature row below includes strict all-targets Clippy (`--locked --
-D warnings`) and locked tests:

| Scope | Final test result |
| --- | --- |
| `kinema-mpv --no-default-features` | **63 passed, 6 ignored** |
| `kinema-desktop --no-default-features` | **489 passed, 4 ignored** |
| `kinema-desktop --features ui-probe-test` | **569 library + 5 binary = 574 passed, 4 ignored** |

The default-feature `ui-probe` binary build also passed. Results are attributed
to the lead's final combined runs, not independently rerun by this documentation
reviewer. **All six isolated CPU/GPU-native backend fixtures passed again on the
final current tree**, including the numeric vendor-context and failure-category
corrections. All owned children exited; the native slot was released.
**The isolated root UI probe is blocked, not passed**. Compilation, pure fixtures
and backend native tests do not establish new GPUI-window/runtime qualification.

## Linux activation: separate application and window APIs

Read-only inspection of the actual pinned upstream source
`gpui-pre-linux-0.3.9/src/linux/platform.rs` shows
`LinuxPlatform::activate` logs the fixed INFO message
`activate is not implemented on Linux, ignoring the call` and does nothing.
Vendored `gpui-pre/src/app.rs`, `App::activate`, delegates to this platform API.
Kinema calls `cx.activate(true)` after opening the main window (`main.rs`), in
the lab (`design_lab.rs`), quit UI (`screens/quit.rs`), and player probe
(`screens/player_window.rs`). The message alone does **not** establish a player
startup failure, failed decoder or failed window focus.

The separate `Window::activate_window` delegates to platform-window activation.
Upstream X11 `src/linux/x11/window.rs` sends `_NET_ACTIVE_WINDOW` to the window
manager. Upstream Wayland `src/linux/wayland/window.rs` requests an xdg activation
token when activation support and app ID exist; its comments explicitly allow
compositor rejection/attention-only behavior. A no-window-manager Xvfb result
cannot validate normal desktop activation. No focus or activation call was
changed merely to silence this informational message. Native focus/replay
behavior remains unverified by this investigation.

`main.rs` no longer formats arbitrary logging-initialization, icon-decode or
window-open errors. These now report only fixed `logging_initialization`,
`icon_decode` and `window_open` categories. No focus/activation calls changed.
This removes a disclosure risk, not proof that previous errors contained secrets.
Don't broaden logging filters or dump native/backend text to diagnose activation.

## ui-probe isolation and documentation gaps

`docs/design/ui-probe.md` correctly describes the offline design lab and the
Linux capture runner. `scripts/ui-probe/native-verify.cjs` allocates its own Xvfb
display, refuses display `:0`, redirects HOME and all XDG app/runtime roots to
an owned mode-0700 temporary directory, removes inherited Wayland/X authority
and D-Bus session variables, and targets only the fixture window. It cleans up
its own child process groups and directory. It still inherits other environment
variables and records fixture stdout/stderr in `report.json`: this is not a
sandbox, secure-store interception or permission to attach unreviewed logs.
The runner is for lab captures, not the native player media command.

`src/bin/ui-probe.rs` routes `player-native` directly to the offline PlayerWindow
probe rather than production startup. It checks for a generated local media file
and installs a stderr tracing subscriber without ordinary app/file-log setup.
The `player` scene is synthetic chrome only. Neither proves managed acquisition,
loopback lease handling or real providers. Prefer the documented current
`player-native` name, not the historical experimental alias.

Relevant probe documentation limits:

- The guide's opening claim that no real player opens describes the lab, not
  `--scene player-native`; that scene intentionally opens native libmpv/GPU.
- The current CLI accepts only **1–15** seconds and PlayerWindow clamps to 15.
  Development's performance instructions now reflect this bound instead of
  recommending an out-of-range 20-second probe. A bounded smoke is not a soak;
  longer validation needs a separately approved workflow. The harness was not
  widened.

## Media startup, interruption and observations

Source findings and focused fixture evidence are recorded in the
[media integration contract](../../crates/kinema-desktop/src/media/README.md).
A fast loopback response-header return is not first playable data: acquisition
still validates the complete upstream range (up to 4 MiB), syncs payload, checkpoints
the manifest, then publishes durable availability. Even a 64 KiB body reader can
wait for this full cycle. Header timing combines connection and upstream response
wait; it cannot independently identify DNS, TLS, provider processing or network
scheduling. No partial publication, parallel acquisition or relaxed durability
was introduced.

A stale-priority scheduling defect was corrected: once a reader's priority byte
is durable, acquisition returns to the earliest missing gap rather than scanning
forward into a later gap from the stale hint. Actually missing priority bytes
still win at the next cycle; in-flight I/O is not interrupted. Priority remains
one last-writer-wins hint, not a fair demand queue. EOF-tail cases without later
gaps already fell back correctly, so this is not a universal startup-speedup
claim. Reader-disconnection detection was also added. Follow-up QA found and corrected
a partially durable 64 KiB reader-window edge: `storage::reader_priority` now
selects the first missing byte of the full reader window, not an already-durable
segment start. Exact overlapping-64-KiB-boundary scheduler and real stream-worker
regressions cover this correction. These are proven synthetic scheduling fixes,
not measured improvements to real-provider or native startup latency.

Release wakes readers independently of blocked upstream headers/body. Synchronous
upstream I/O remains bounded by existing 10-second connect/20-second total
request deadlines, not force-canceled; caller renewal closures must bound their
own I/O. Missing-buffer waits remain 45 seconds with 200 ms polling, and a body
consumer stall remains 30 seconds. Error after response headers aborts the body
rather than rewriting its HTTP status. Bytes already queued may still reach a
socket. A delay near 30 seconds alone does not establish which timeout or phase
was involved.

Exact `KINEMA_PLAYER_DIAGNOSTICS=1` adds bounded startup observations on
`kinema_desktop::media_performance`:

- Process-local numeric `asset_id` correlates acquisition/readers;
  `operation_id` identifies an acquisition invocation or reader; `request_id`
  numbers upstream attempts (readers use zero). IDs are not capability tokens,
  hashes, pointers or persistent identities.
- Numeric half-open `range_start`/`range_end` and boolean `near_tail` describe
  shape. Near-tail means within the last 4 MiB, not proven demuxer behavior.
- Latest pending phase/age is sampled every five seconds, at most 12 pending
  events per operation. Fast phases may be absent; no per-chunk INFO is added.
- One first-durable event measures acquisition worker elapsed and first successful
  header/body/payload-sync/checkpoint timing. One first-queued event measures
  reader handoff to the bounded queue, not decoder receipt or presentation.
- Terminal reader `range_wait_count`, `range_wait_total_ms`, and
  `range_wait_max_ms` aggregate unavailable 64 KiB buffer waits, including failed
  waits, not HTTP request counts. `phase_ms` is just the latest phase. Open waits
  enter totals only after exit and remain visible as pending phase age meanwhile.
- One terminal event reports typed outcome/category and elapsed/latest-phase
  time. Existing five-second completed-durable-range aggregates are separate;
  failed, retried or incomplete work is absent from their completed samples.

Disabled diagnostics create no startup timer owners; opt-in uses at most one
constant-size timer snapshot/thread per acquisition or body reader (8 + 8 = 16
live threads). Blocking workers own teardown; UI/service Drop never joins these
timers. Spawn failure disables observation rather than playback. Only numeric
fields, booleans and fixed enums/categories are emitted, not raw errors, URLs,
paths, headers or native text.

The media report describes gated synthetic HTTP fixtures for blocked headers and
partial bodies, no sparse/partial visibility before durability, normal range
recovery, cancellation and unsupported-range failure. Final focused media results
are **41 tests passed with diagnostics disabled and 41 enabled**, including the
partially durable reader-window correction. Earlier 39-test runs and strict
desktop all-targets Clippy/owned-file rustfmt/diff passes preceded that correction;
final combined checks passed as recorded above. Tests
use gates/request shape, not host-sensitive throughput thresholds. Direct
local-file PlayerWindow probes bypass acquisition and cannot qualify these wait
metrics or reproduce provider startup.

## Native diagnostics and scoped CPU fixture evidence

Backend changes in `mpv/diagnostics.rs`, `player.rs` and `render.rs` classify
FFmpeg messages into fixed categories, including MKV `metadata-unseekable`, and
record numeric end-file context without forwarding raw native text. Diagnostic
severity is preserved. `player_terminal=false` means the diagnostic alone does
not decide player state; typed worker failure/end-file handling still does.
Cancellation is not a blanket reason to suppress FFmpeg warnings/errors.

Lifecycle timing meanings, cross-checked against the source:

- `media_sequence` advances at each observed START_FILE.
- `elapsed_ms` measures backend/player lifetime, not just the current file.
- `start_file_elapsed_ms` measures from START_FILE and is retained through the
  END_FILE event, then cleared. `open_duration_ms` measures START_FILE to
  FILE_LOADED only, not load intent to displayed video.
- First qualification and `first-mailbox-publication` milestones describe backend
  evidence/publication, **not UI acceptance or physical presentation**.

Dimensions and decoder evidence are captured coherently. Qualification failure
is applied atomically only for its matching generation, preventing a stale HDR
failure from replacing newer-source state. The backend report describes focused
regressions; the final combined code validation passed.

The authorized native agent reports **two passing CPU-native fixtures**, each
using synthetic inputs with isolated temporary CWD, HOME and XDG roots. The agent
personally reviewed `target/mpv-safe-native.vXKjZW/startup.log` and
`interruption.log`; these results are attributed, not independently rerun here:

| Fixture | Observed outcome | Limit |
| --- | --- | --- |
| Malformed source | End-file error -17, recoverable `media-load-failed`; final rerun: media error true, terminal false, first frame false, four unclassified native diagnostics including one other/error. | Intentional malformed-input handling, not the user's successful playback or reported delay. |
| Interrupted transport | Final rerun: first frame true, `truncated-stream` ERROR before Drop and `metadata-unseekable` WARN; ended true, media error false, fatal false. Earlier run placed ERROR about 1.18 seconds before shutdown. | Truncation was not proven expected cancellation. Preserve native severity; this is not evidence for suppressing the user's FFmpeg diagnostic. |

No provider/media-account or physical presentation acceptance follows from these
CPU fixtures. Separate GPU backend evidence is recorded below.
A test-only diagnostic accessor is gated with `cfg(test)` for no-default-feature
builds; the final feature rerun passed as recorded above.

## GPU startup discovery and qualification

The GPU implementation report, cross-checked read-only against
`gpu_surface/linux.rs`, describes a narrower device-discovery path:

- Optional `EGL_EXT_device_persistent_id` UUIDs prioritize known exact matches
  and reject known mismatches **before display/context initialization**. If the
  extension is absent, discovery retains the previous context-probing path.
  Malformed advertised UUIDs are rejected rather than treated as unknown.
- This prefilter does not replace the final GL/Vulkan UUID match or verified
  render-node file descriptor used by the decoder. Device identity and decoder
  qualification remain required; no automatic renderer fallback is added.
- Each discovery attempt emits one summary: `enumerated`,
  `advertised_mismatches`, `invalid_identities`, `unknown_identities`,
  `context_probes`, `context_failures`, `gl_uuid_rejections`,
  `render_node_failures`, and boolean `matched`. No device UUIDs, paths, native
  strings or arbitrary candidate errors are logged.
- Enumeration itself can still emit Mesa warnings. Avoiding known mismatched
  context initialization does **not** establish that all Mesa warnings disappear
  or that such warnings caused the reported startup problem.

`gpu_surface/mod.rs` adds `qualification_wait_ms`, measured from the first
qualification-evidence check in that source generation, not from process launch,
media acquisition or first presentation. The GPU report describes pure tests for
same-generation SDR-to-HDR evidence changes and new-generation timer reset.
The isolated backend results below provide scoped Linux evidence for these
changes, not full desktop/provider or HDR-playback acceptance.

### Isolated Linux GPU backend results

Four exact ignored backend tests passed, sequentially in separate processes,
with isolated HOME/XDG/TMP/CWD and the validation layer enabled:

1. H.264 8-bit 1080p playback.
2. Source switching 1080p → 4K → 1080p, retained frames, backpressure and lifetime.
3. PQ metadata rejected as `hdr-unsupported`, with no frame publication.
4. Mixed EGL coexistence and close-survivor behavior.

For each surface discovery: `enumerated=4`, `context_probes=1`, `matched=true`,
`advertised_mismatches=1`, `invalid_identities=1`, `unknown_identities=1`.
Source/render dimensions matched exactly; direct VAAPI was observed, with no
playback readback/upload. These are backend observations, not measured physical
presentation or proof of all decoder/platform support.

Residual NVIDIA PCI driver-null discovery lines numbered **4, 4, 4 and 8** in
test order. MESA-EGL/DRI-screen warnings were zero, as were VUID, validation,
threading-error and panic matches. **Stderr was not entirely quiet**: do not claim
that UUID filtering eliminates all driver enumeration diagnostics.

Initial artifacts: `target/gpu-startup-review-20261024T221754Z-1110936/`; the lead
personally reviewed H.264 stderr/safe summary and the source-switch summary.
The **final unchanged-tree rerun passed all six CPU/GPU fixtures**, with evidence
at `target/gpu-startup-final-20261024T223006Z-1135334/`. The lead read the entire
`safe-summary-all.json` (Rust 1.97.1; recorded binary SHA prefix `59f1acc9`). The
four GPU reruns confirmed direct VAAPI 1080p/4K, exact dimensions, no fallback or
playback readback/upload, fail-closed HDR rejection with no GPU publication, and
mixed-EGL close-survivor behavior. Discovery remained four devices/one context;
residual PCI lines remained 4/4/4/8, with zero EGL/DRI, VUID, validation/threading
error or panic matches. All owned fixture children exited.

This is attributed evidence, not a rerun by the documentation reviewer. No
ordinary app startup, account, provider, user database or user media was opened.
PQ rejection validates fail-closed behavior, not HDR playback support.

### Root UI probe: failed unconstrained attempt retained

The lead's root `ui-probe` uses generated H.264 1080p@24 media lasting 20 seconds,
but only an **8-second smoke budget**, with stress resize, validation and
diagnostics under private Xvfb, `env -i`, owned app roots and CWD. The first
attempt, with no ICD override, **failed before EGL initialization** with fixed
`gpu-device-unsupported`: zero frames, one GPU-error-log count and zero panics.
Presentation ownership released and no-late-frame checks passed. The shutdown
signal was false because initialization failed before a Player existed; it is
not evidence of successful player teardown or a demonstrated teardown leak.

Failed artifacts are preserved at `target/playback-ui-review-RSL2p1/`. A later
unconstrained run with the new numeric gate diagnostic confirms GPUI selected
vendor **4318 (0x10de, NVIDIA)** with `vendor_supported=false`, followed by
`gpu-device-unsupported` before EGL. The lead read
`target/playback-ui-final-20MC1h/default/probe.stderr`. The diagnostic does not
change device selection. This is an observed headless-selection/capability
failure, **not the user's Intel startup-delay issue**.

The comparison uses a **process-scoped Intel ICD override only within this
isolated fixture**, not a global configuration change or normal-user-launch
recommendation. This Intel comparison **failed to open the window before backend
startup**: Mesa reported no DRI3 presentation support, and the GPU-error count
was one. After `window-open-failed`/`cx.quit`, the process did not exit; the lead
stopped only the owned process tree using `async_bash stop`. Evidence remains
under `target/playback-ui-final-20MC1h/`; no successful or fabricated exit code
is claimed. The GUI harness's failure-exit behavior is a separate limitation,
not evidence for the user's playback delay.

No isolated Weston/cage compositor was installed. The lead did not use the user
compositor or change global driver configuration. **Root UI runtime validation
is blocked in this headless environment**, with no new UI qualification. Both
unconstrained and Intel-scoped failures remain visible; backend test success
does not override them.

Review also found an error-classification regression after EGL errors were
redacted. `failure.rs` fixed-category matching is corrected for the redacted
errors with adversarial tests; final combined code validation passed with this
correction.
A 20-second input file does not make the 8-second smoke a 20-second probe or
soak. No ordinary app startup/accounts/providers/user database were opened.

## Issue conclusions

| Issue | Conclusion / action |
| --- | --- |
| Missing optional startup credentials | Fixed here: DEBUG/false, not WARN; service failures still WARN/error. |
| Missing required credential or failed mutation | Preserve WARN/error; no silent success. |
| Optional TMDB enrichment using required lookup | Fixed with explicit optional lookup; required calls and service failures retain errors. |
| Linux application activation INFO | Expected upstream no-op; no focus changes justified by this message. |
| Optimization banner | No change warranted: root `[profile.dev.package."*"]` uses `opt-level=2`, as does explicit `kinema-mpv` development configuration. Generic optimization advice is not evidence that this banner caused the startup delay; no profile/vendor change made. |
| Unrelated EGL device context initialization | Four isolated GPU backend tests pass; each discovery probes one context among four enumerated devices while retaining final identity checks. |
| Mesa/discovery warnings | No MESA-EGL/DRI-screen warnings in these GPU tests, but NVIDIA PCI driver-null discovery lines remain; no all-warnings-eliminated or startup-cause claim. |
| Qualification delay | Generation-scoped `qualification_wait_ms` improves measurement; does not itself reduce startup latency. |
| Raw startup/icon error logging | Fixed to three closed categories. Backend native messages use fixed categories plus numeric context; severity preserved and no cancellation-wide suppression. Final combined code checks pass. |
| FFmpeg truncated-stream diagnostic | Synthetic interruption observed ERROR before shutdown; not established as expected cancellation or proof of the real startup cause. |
| Stale qualification failure / evidence race | Coherent snapshot and atomic generation checks preserve current-source state; feature checks and source-switch/PQ-rejection backend tests pass. Root UI runtime qualification remains blocked by headless capability/presentation failures. |
| Stale satisfied reader priority | Corrected scheduling; no universal latency claim. |
| Reader disconnect / partially durable window | Fixed disconnected-reader detection and first-missing-byte priority; overlapping-boundary scheduler and real stream-worker regressions pass in the 41-test off/on media runs. |
| First playable bytes / roughly 30-second delay | Whole-range durability can delay readers; new pending/first/end timings distinguish phases, not automatic causal attribution. Real-media startup/acquisition timing remains unverified; use the manual diagnostic workflow below. |
| Reported startup delay / nonterminal diagnostic | User playback succeeded. No fixture establishes the cause or duration of the reported real-media delay; native diagnostics alone do not mean playback failed. |
| Intentional malformed-input failure | CPU fixture fails recoverably with no frame; separate from the user's successful playback. |
| Unconstrained headless UI capability failure | Numeric gate confirms NVIDIA vendor 4318, unsupported before EGL; not the user's Intel issue. Intel ICD comparison fails window creation without DRI3 presentation; UI qualification blocked. |
| Redacted EGL failure classification | Stale matching prefixes corrected with adversarial tests; final combined checks pass. |
| Native prerequisites/vendor integrity | Lead reports vendor verification PASS (307 files), and ffmpeg, Xvfb, xvfb-run, xdotool, libmpv.so.2, EGL and renderD128/129 available. Not independently probed for this record; availability is not successful startup/interop qualification. |

## Normal manual launch — user operated, not part of fixture validation

For startup investigation, use this command from the repository root. The exact
`KINEMA_PLAYER_DIAGNOSTICS=1` enables media-stage evidence and cadence/performance
summaries; INFO and noisy-dependency exclusions remain, adding DEBUG **only**
for the media module (not global desktop/HTTP/native DEBUG):

```sh
KINEMA_PLAYER_DIAGNOSTICS=1 \
RUST_LOG='info,kinema_desktop=info,kinema_core=info,wgpu_hal=warn,naga=warn,zbus=warn,reqwest=warn,hyper=warn,hyper_util=warn,kinema_desktop::media=debug' \
  cargo run -p kinema-desktop --locked
```

For a non-diagnostic baseline, omit `KINEMA_PLAYER_DIAGNOSTICS=1` while retaining
the same RUST_LOG value. Unset the variable after investigation. INFO keeps
`kinema_desktop::player_performance` cadence visible without enabling all player
DEBUG. Cadence measures UI-accepted updates, not physical GPU/display FPS.
Ordinary launch intentionally uses normal application state and may access the
user's settings/database/keyring; **it is not an agent-safe isolation command**.
Only the user or explicitly authorized lead should run this workflow. Review
logs locally for fixed-category/numeric fields before sharing; don't attach raw
native text, URLs, paths, tokens or arbitrary errors. Media DEBUG is not itself
a redaction guarantee: still inspect every line before sharing.

### What to share after a user-operated retry

Do **not** send raw logs. Review locally and share only these structured lines,
with only fixed categories, booleans and numeric fields (remove any unexpected
URL, path, token, raw native text or arbitrary error before sharing):

- Exact media messages: `Media operation still pending`, `Media first durable
  range`, `Media reader first queued bytes`, and `Media operation ended`.
  Include numeric operation correlations, phases, elapsed times and reader wait
  totals, including failed/unfinished operations—not just completed chunk rates.
- `mpv lifecycle` with `event="file-loaded"`, `open_duration_ms`, `media_sequence`
  and the explicitly named lifetime/current-file elapsed fields.
- `mpv native diagnostic` with fixed category, severity and `player_terminal`;
  `mpv media ended` with fixed `reason` and numeric `mpv_error`. Do not paste the
  original FFmpeg message. End-file reason is categorized, not arbitrary text.
- `EGL device discovery summary` numeric counters/boolean result;
  `mpv GPU surface metadata qualification` with `qualification_wait_ms`;
  `mpv GPU surface startup timing` with `milestone="first-mailbox-publication"`.
- `mpv GPU surface native cleanup` with `native_cleanup="complete"` or
  `"quarantined"` and numeric context. Playback stopping alone does not establish
  clean release.

Also state whether a first frame appeared, what action occurred (open/seek/close),
and relative action timing without title, media path, session URL or provider
credentials. Correlation helps separate stages; it does not automatically explain
a roughly 30-second delay or establish a GPU/provider root cause.

## Isolated reproduction commands — explicit authorization required

First have the lead authorize the exact platform/display scope. Use generated
media; do not open ordinary app startup, real providers, user mpv config,
keyring or a production DB.
The Linux lab runner is suitable for synthetic chrome/focus capture:

```sh
cargo build -p kinema-desktop --bin ui-probe --features ui-probe --locked
node scripts/ui-probe/native-verify.cjs target/ui-probe/startup-chrome --lab --scene browse --focus 115,190 --tabs 8
```

For an authorized Linux offline player test, use an owned temporary directory,
empty environment apart from explicit runtime necessities, and private Xvfb.
Reproduction recipe (review prerequisites/driver policy with the lead first;
this does not bypass the headless UI blockers recorded above):

```sh
fixture=$(mktemp -d)
chmod 700 "$fixture"
mkdir "$fixture/config" "$fixture/cache" "$fixture/data" "$fixture/runtime"
chmod 700 "$fixture/runtime"
ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=30 -t 15 -c:v mpeg4 -q:v 5 -an "$fixture/generated.mp4"
binary="$PWD/target/debug/ui-probe"
xvfb-run -a -s '-screen 0 1200x760x24' sh -c '
  cd "$1" || exit 1
  env -i PATH="$PATH" DISPLAY="$DISPLAY" XAUTHORITY="$XAUTHORITY" \
    HOME="$1" XDG_CONFIG_HOME="$1/config" XDG_CACHE_HOME="$1/cache" \
    XDG_DATA_HOME="$1/data" XDG_RUNTIME_DIR="$1/runtime" XDG_SESSION_TYPE=x11 \
    RUST_LOG=kinema_mpv=info,kinema_desktop=info \
    "$2" --scene player-native --media "$1/generated.mp4" \
    --smoke-seconds 12 --stress-resize
' sh "$fixture" "$binary"
```

Retain/review only fixed-category/numeric output and synthetic captures, then
remove only that owned temporary directory. Do not copy raw native output into
shared logs. Xvfb may lack GPU interop capabilities: record a categorized
capability failure, do not weaken policy or imply desktop failure. Real Wayland
activation testing requires separately authorized compositor/runtime access;
private Xvfb cannot cover it. Review actual first frame, controls, resize,
close/worker teardown, error/panic counters and secret-safe output. This probe
has no audio and cannot establish audible playback, provider startup, physical
presentation, sustained performance, Windows/macOS or HDR qualification.
