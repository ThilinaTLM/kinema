# Native UI verification before Graphite implementation

**Implementation follow-up:** the [isolated GPUI probe](ui-probe.md) now records
actual spike results and runnable checks. This document preserves the research
proposal and its evidence; it is not the current harness execution report.

**Research proposal, 2026-10-17. Not an implemented harness or approval to restyle
production.** Start with a tiny fixture-only feasibility milestone, review its
evidence, then extend it. Keep official `gpui-kit = 0.7.3`, its reexports, assets,
`init`, and `open_window`; add no second GPUI dependency.

**Recommended three layers:** native headless Kit behavior/layout assertions;
optional shared-Rust WASM gallery with Playwright screenshots and agent review;
separate native actual-pixel, performance, and OS verification. Lead-forwarded
pinned findings and the official [testing](https://gpui-kit.com/docs/test/) and
[WebAssembly](https://gpui-kit.com/docs/webassembly/) guides support this strategy.
The testing URL is `/docs/test`, not `/docs/testing`. The live
[component gallery](https://gpui-kit.com/gallery/) uses Rust views rendered to a
WebGPU/WebGL2 canvas, not an HTML reimplementation. It is a maintained component
preview, not evidence of mature full-application browser distribution. Current
online guides may advance, but native-agent/lead inspection of pinned 0.7.3
`TESTING.md`, `src/test.rs` and `tests/window.rs` confirms the core APIs below.
Do not substitute a newer checkout, patch, or second GPUI dependency.

## Evidence collected (not runtime verification)

| Evidence | Observed consequence |
| --- | --- |
| Root `Cargo.toml`, desktop manifest, `Cargo.lock` | Two-crate workspace; pinned Kit 0.7.3; native platform packages in the lockfile use GPUI pre 0.3.9. Desktop also depends on SQL, keyring, HTTP, Tokio/Axum, and filesystem adapters. |
| `crates/kinema-desktop/src/main.rs` | Uses Kit application/assets/init/open-window wrapper and constructs `Shell::new`. It has no inspected fixture-only entry path. |
| `shell.rs`, `Shell::new` | Starts `CatalogWorker::spawn_default` and `AppWorker::spawn`, then submits `LoadSettings`. Merely launching the existing app is not a safe isolated gallery. |
| `app.rs`, `Application::settings` / `snapshot_warning` | Resolves ordinary paths/settings and queries both debrid and metadata credential stores. A temporary HOME alone is not a secure-store isolation boundary. |
| `catalog_worker.rs`, `spawn_default` | Constructs `CatalogService::open_default` on demand. Do not reuse this default backend in visual fixtures. |
| `.github/workflows/ci.yml` | Ubuntu 24.04, macOS 15, Windows 2025 compile/Clippy/test matrix; Rust 1.97.1 and locked dependency checks. No GUI launch, screenshot comparison, visual artifact upload, or native performance job is configured. Linux installs Vulkan loader/validation packages, not proof of a usable headless renderer. |
| `docs/architecture.md`, `docs/development.md` | Native GUI/provider/OS acceptance is distinct from source/check evidence. Linux needs a working Vulkan driver, not just the loader. Debug startup can use repository `data/catalog.sqlite3`; release uses platform paths. Neither is permitted here. |
| `design-system.md`, `README.md`, `desktop-wireframes.html` | Frozen Graphite HTML is a design review artifact; permanent 56 px icon rail is settled. Browser fixture/performance results are not native rendering or timing evidence. Production visual approval remains required. |

Read-only inspection found extensive existing tracked and untracked work. Nothing
in that work was reverted, formatted, compiled, or launched for this research.
No credentials, user database, provider, player, or desktop screenshot was used.

Official-guide evidence (also confirmed in the lead-forwarded research):

- Source-verified in pinned 0.7.3: `#[gpui_kit::test]`, `TestAppContext`,
  `TestWindowExt::{find, click, hover, press, input, render_frame}`,
  `TestAppContextExt::wait_for`, and `TestSupportExt` observation of snapshot
  bounds, accessible labels, visibility and focus. Apply `.test_support()`
  **before** `.track_focus(&handle)`. These APIs are source-verified, not yet compiled
  or exercised in Kinema. Proposed desktop dev dependency (not added):
  ```toml
  [dev-dependencies]
  gpui-kit = { workspace = true, features = ["test-support"] }
  ```
  Proposed root command: `cargo test -p kinema-desktop --locked` (**not run**).
- Native offscreen pixel APIs (`HeadlessAppContext::with_platform`,
  `Window::render_to_image`, `capture_screenshot`) currently have a **macOS Metal
  renderer only**. Linux/Windows headless interaction/layout tests do not produce
  real rendered pixels. The upstream explicit rendering target is not a command
  that can simply be run against Kinema's workspace.
- WASM uses `wasm32-unknown-unknown`, `gpui_kit::web`, a JS loader and retained
  application handle; graphics initialize asynchronously. `Auto` tries WebGPU,
  then WebGL2. One canvas/top-level window is supported; use in-window dialogs.
- The web font database starts empty. Bundle appropriately licensed initial font
  files, including the default/system alias and theme families, and register them
  before opening the first window. CSS/native OS fonts are not an adequate default.
- Upstream story-web selects nightly and documents wasm-bindgen CLI 0.2.121 for
  its lockfile. Kinema requires Rust 1.97.1 and locks wasm-bindgen 0.2.129. This is
  an explicit compatibility gate, not permission to copy upstream install commands.

Sources above are linked guides, summarized/adapted for Kinema; GPUI Kit prose is
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). No guide code is copied.

### Local tool inventory

`command -v` confirmed `Xvfb`, `xvfb-run`, `xdotool`, `grim`, `import`,
`vulkaninfo`, `node`, `npm`, `cargo`, `rustc`, `ffmpeg`, `compare`, `magick`, and
`python3`. `wmctrl` and `weston` were absent. Presence does not verify versions,
renderer availability, image-diff behavior, or a working window manager.
Lead inventory additionally reports no installed `wasm32-unknown-unknown` target
or wasm-bindgen CLI; installed targets are native Linux/macOS/Windows only. Node
availability does not establish Playwright/browser installation or usable WebGPU.
Only relevant display variables were printed: `DISPLAY=:0`,
`WAYLAND_DISPLAY=wayland-0`, `XDG_SESSION_TYPE=wayland`. These identify an existing
personal session, **not test targets**. Do not use `grim`, root-window capture, or
an implicit display connection against it. No `vulkaninfo` runtime probe was run.

**Current hard blockers:** no safe fixture-only bootstrap; no demonstrated
Vulkan-capable renderer under an isolated display; source-verified native test
APIs not yet compiled/exercised in Kinema; missing WASM target/CLI and unverified Rust 1.97.1
compatibility; no approved baselines. Xvfb supplies an X server, not a GPU or
Vulkan presentation guarantee. Missing Weston blocks a local Weston-based fallback
without a separately approved environment change. Xvfb/X11 remains a candidate,
not proven support. This research authorizes no installations or app starts.

### Lead-reported public browser probe (actual remote-gallery evidence)

The lead ran an isolated Playwright 1.62.1 Chromium headless probe with
`--enable-unsafe-swiftshader` against `https://gpui-kit.com/gallery/`. The deployed
footer reported **v0.7.3**; this was the public deployment, **not a locally built,
locked Kinema/pinned-source artifact**.

- The WASM response was HTTP 200 and one 1280 × 900 canvas was present.
  `body.innerText` was empty: DOM text assertions are not a sufficient oracle.
- WebGPU reported no adapter; `Auto` fell back to WebGL2 via ANGLE SwiftShader.
  The gallery actually painted, rather than merely dismissing its loader.
- Clicking the canvas Checkbox navigation item changed the displayed view.
  Clicking the first Checkbox at `(614, 266)` changed local pixels from white to
  dark with a checkmark. This establishes pointer-driven visual change in this
  public browser deployment, not a semantic-ID driver or native OS input check.
- Pressing Space after that click **did not visibly untoggle** the first checkbox;
  its crop remained dark. Keyboard activation is **unproven** and needs focus/event
  investigation. Do not count that step as successful keyboard coverage.
- There were zero page errors, but the console was **not clean**: font/asset
  loading errors/warnings, GPU fallback and timeout messages were recorded.
  A visible success does not excuse those diagnostics or prove complete assets.

Lead-owned artifacts under `/tmp/kinema-gpui-research-20261017/`:
`probe-gallery.cjs`, `gallery-results.json`, `upstream-gallery.png`,
`upstream-checkbox.png`, `upstream-checkbox-click.png`, and
`upstream-checkbox-space.png`. These are temporary remote-gallery probe artifacts,
not personal-desktop captures, approved Kinema baselines, or durable CI uploads.
The document owner did not rerun the probe. Lead source research also confirms
native Kit helpers; the milestone still must exercise them in this project.

This is positive evidence for an optional shared-Rust browser visual loop, with
keyboard, fonts/assets, local build compatibility and native pixels still open.
No project Rust/Cargo/WASM build, native application start, or installation was
performed for this probe. It does not remove the fixture/bootstrap isolation gate.

## Minimal proposed fixture boundary

After approval for a separate implementation task, prefer a small native gallery
entry point in `kinema-desktop` using the same Kit and eventual production
components, not a new framework or duplicated whole app. If the WASM spike passes,
share only the small Rust view/state specimen with a target-specific web entry;
do not compile the existing desktop bootstrap or port its DB/backend to the browser. Select fixture mode **before**
constructing `Shell`, workers, paths, credential stores, HTTP clients, or media.
A separate fixture executable is preferable if that makes the boundary auditable;
exact Cargo/entry-point changes require their own approval and are not made here.

- Construct only in-memory view data and an explicit fixture action recorder.
  Never use an in-memory SQL database as a shortcut to real backend composition.
- Permit bundled synthetic artwork/fonts/icons and a new owned artifact directory
  only; no URLs, user media, chooser access, provider tokens, database, keyring,
  media server, subprocess player, or production services. Display a fixture badge.
- Use fixed IDs, seed, locale, timestamps, sort order, logical viewport, scale,
  theme, and reduced-motion policy. Freeze animation/time for screenshots; advance
  fake events explicitly rather than sleeping for invented network delays.
- Model loading, empty, success, stale, unavailable, and failed states separately.
  A fake Play action records an intent; it cannot invoke playback. Credential
  fields contain status fixtures only, never real or plausible secret values.
- Generation/cancellation scenarios can inject obsolete results in memory and
  assert rejection. They exercise UI presentation, not real worker cancellation.
- Keep pure rules in `kinema-core`, presentation/test composition in desktop.
  Add only the seam needed by the first specimen; no service-container, full
  scenario language, generalized screenshot service, or provider emulator upfront.

Start with a shared primitive and one media-card specimen: known IMDb rating,
unknown rating, valid `0.0`, long title, missing artwork, and missing IMDb mapping.
The first spike needs only one card plus a focusable action to prove the loop;
expand specimens only after the loop works. Later scenarios can cover gallery,
browse/detail, sources, simulated prepare, transfers, settings, and quit/cancel,
without implying end-to-end backend acceptance.

## Proposed input, assertions, and visual loop

Use stable semantic IDs such as `gallery.card.fixture-zero`, `card.details`, and
`dialog.cancel`, independent of text, screen coordinates, or list index. Associate
role, label, selected/disabled/focus state with each where the pinned API supports
it. An element ID is not automatically an accessibility tree or OS-automation
selector. Use the source-verified Kit helpers for the headless driver.

Three complementary layers are recommended:

1. **Native headless component/application assertions:** fixture state, focus
   target, emitted intent, disabled-action behavior, missing-vs-zero rating,
   bounded disclosure, and stale-result rejection. Exercise documented Kit test
   facilities whose pinned exports are source-verified; use element-ID snapshots/bounds
   and simulated input. These are behavior/layout checks, not Linux/Windows pixel
   captures or OS-input coverage. Direct action dispatch alone is weaker coverage.
2. **Optional WASM visual loop:** render the same Rust specimen in a browser
   canvas, use Playwright key/mouse input, and inspect canvas screenshots/diffs.
   Do not expect DOM selectors for individual painted controls: use canvas-relative
   bounds from the test adapter and independently assert resulting Rust state.
   Wait for fonts/assets, graphics readiness and an actual frame, not just a hidden
   loader. Record the chosen WebGPU/WebGL2 backend; treat them as separate baseline
   environments. Agent visual review complements automated assertions.
3. **Real native input and pixels:** send Tab/Enter/Escape and pointer events to
   the isolated fixture window; assert expected focus/state and capture its client
   area after a render-ready acknowledgment. On isolated X11, `xdotool` is a
   candidate fallback. It does not establish Wayland, macOS, or Windows input
   automation. Avoid coordinate-only scripts where semantic resolution is possible.

The minimal proposed adapter reports readiness, scenario/step, logical bounds,
focused ID, and a small allowlisted semantic snapshot to an artifact file; a
second proposed operation resolves a semantic ID to current bounds for pointer
input. This is test-build-only instrumentation, not an assumed upstream API.
Use bounded timeouts and an explicit frame/asset-settled signal; never accept a
blank image simply because a process exists. Require a visible fixture marker,
expected dimensions, and a changed image after the driven state transition.

### Milestone 1: smallest useful proof (future, not executed)

Before production styling or a full gallery, approve one Rust specimen (a
focusable button or card, a disabled action, and a visible state change):

1. **Native headless:** compile/run one real Kit test against published 0.7.3;
   assert element identity/bounds, mouse/key behavior, focus, and disabled action.
   Deliberately invert an expected result once to demonstrate that the test fails.
   Preserve results; do not mislabel test-platform output as rendered pixels.
2. **WASM compatibility decision:** check whether that same view can compile with
   Rust 1.97.1, the locked dependency graph and a matching 0.2.129 CLI. The target
   and CLI are absent; browser tooling is unverified. Obtain approval before setup.
   Do not silently adopt nightly, downgrade the lockfile, or add another GPUI.
   If a different toolchain/dependency resolution is required, report the decision
   instead of changing policy. Time-box this feasibility work; if expensive or
   incompatible, skip WASM and continue with the native fixture harness.
3. **Optional shared-Rust browser proof:** add only a tiny WASM entry/loader and
   bundled licensed fonts/assets served locally; deny external asset/provider
   requests. Use the same fixture view, no DB/keyring/network/media composition.
   Verify WASM/JS loading, initialized graphics, visible nonblank specimen and
   completed first frame. Drive both keyboard and mouse through Playwright and
   assert Rust state/focus; save canvas images at fixed size/device scale.
4. **Diff negative control:** compare repeated clean renders for determinism,
   then intentionally change one visible specimen property (for example its
   selection fill) and require the screenshot diff to fail. Keep before/after/diff,
   browser console, failed network requests and state report. Restore only the
   spike-owned mutation. A passing no-op screenshot comparison is not proof.
5. Review the evidence and effort. Approve a native controlled-display capture
   next (below), regardless of whether optional WASM succeeded. No full browser
   application, scenario framework, or production Graphite conversion is part
   of this milestone.

### Milestone 2: native controlled-display capture

1. Review milestone 1 and approve the fixture boundary and one specimen.
   Add only the isolated entry path, ready/state report, one input scenario, and
   capture mechanism. Review the diff for accidental backend construction first.
2. Build with Rust 1.97.1 and `--locked`, using Kit reexports only. Re-run root
   fmt, strict locked workspace Clippy, and locked tests after implementation.
3. Create a fresh owned temporary run directory, distinct HOME/XDG data/config/
   cache/runtime directories, fixed fonts/locale, no credentials in arguments or
   logs, and no inherited session-bus address. Prefer a disposable CI user/container
   with network denied; environment overrides alone are not a security sandbox.
4. Start a **new isolated X server**, not `:0`, using `xvfb-run -a` with a fixed
   screen (candidate 1280×800×24). Inside its child wrapper unset
   `WAYLAND_DISPLAY`, retain only the wrapper's assigned `DISPLAY`/authentication,
   and choose X11 only through the pinned backend's verified selector. Do not
   invent a GPUI backend environment variable. Set up a private runtime directory;
   never reuse the personal compositor/socket or launch the ordinary app.
5. In that same isolated environment, record renderer diagnostics with bounded
   output and timeout. Verify the actual Vulkan device/driver and window surface
   creation. If software Vulkan is unavailable or presentation fails, **stop** and
   preserve diagnostics; do not fall back to the personal display or install tools.
6. Launch only the fixture executable as a child of the wrapper. Wait for a
   bounded ready acknowledgment. Identify the fixture window by its run-specific
   marker/PID; require exactly one match. Without a window manager, do not assume
   `_NET_ACTIVE_WINDOW` activation works: verify direct focus and input delivery.
7. Capture only that window/client area on the wrapper's explicit display and
   verified window ID (ImageMagick `import` is a candidate, pending the spike).
   Never use `-window root`. Drive one Tab/Enter transition, assert semantic state,
   capture again, and save expected/actual/diff plus the run manifest.
8. Repeat three clean runs and compare their outputs for determinism. Require a
   real nonblank render, successful input assertion, stable image dimensions, and
   either repeatable pixels or narrowly explained variation. Make no baseline
   approval automatic. On exit/timeout kill only owned child processes and remove
   only owned scratch files; retain diagnostic artifacts for review.

Pass/fail outcome is **loop feasibility**, not production Graphite approval.
If Xvfb/Vulkan fails, request a dedicated GPU-enabled isolated runner or a separately
approved headless compositor experiment. Do not build the full gallery first.

## Baselines, review, and failure artifacts

Keep native baselines separate by OS/backend, renderer class/driver, fonts, scale,
viewport, theme, fixture version, and relevant Kit/toolchain versions. Record both
logical and physical dimensions. Choose a controlled Linux CI environment first;
retain native fonts on each OS, accepting separate baselines rather than claiming
cross-platform pixel equality. The HTML mock can guide visual intent; it is not
a pixel reference for GPUI, and a future WASM renderer needs its own baselines.

Proposed pipeline: fixture → ready assertion → native input → semantic assertions
→ client-area capture → same-environment image diff → agent visual review → human
approval of intentional baseline changes. Automated diffs detect regressions;
they do not decide whether Graphite looks correct or usable.

- Start with exact same-environment comparisons; measure nondeterminism before
  approving any tolerance. Record changed pixels and error metrics. Any masks
  must be narrow, documented, and reviewed; do not hide focus rings/text/artwork
  or the whole client area to obtain green checks.
- A missing baseline is a review request, not a pass. Never regenerate approved
  references silently in CI. Submit baseline additions/updates with specimen ID,
  reason, before/after/diff montage, environment manifest, and reviewer approval.
- On assertion, timeout, render, capture, or diff failure retain available actual,
  expected and diff images, semantic snapshot, failed step, bounded stdout/stderr,
  executable revision/build identity (including dirty status), fixture/seed,
  viewport/scale/font/theme settings, renderer/OS/tool versions, and exit reason.
  A startup failure may have no image: save logs instead, without desktop fallback.
- Upload only fixture artifacts. Use a short documented CI retention period and
  always-upload-on-failure behavior; exclude environment dumps, home directories,
  tokens, databases, and arbitrary core dumps. No upload workflow exists yet.
- Agent review should inspect images for clipping, hierarchy, focus visibility,
  hover/focus parity, long-title behavior, notices, and both themes. Human design
  approval is still needed before production visual implementation.

## Native performance is a separate experiment

Do not time screenshot capture, software-rendered Xvfb, or the HTML's simulated
1,000-card preview and call it native user performance. After the correctness
spike, propose a release/profile native fixture workload on a documented dedicated
machine with real bundled image decoding (no network). Separate cold/warm runs,
fixed dataset and scripted scroll/search/navigation, reduced motion versus motion,
and GPU/software renderer results.

Measure frame-time distribution/p95/p99 and missed frames, input-to-next-present
latency where instrumentable, RSS plus image/GPU memory where available, mounted
card counts, decode queue concurrency, cancellation/stale completion counts, and
resource recovery after repeated navigation. Record instrument limitations and
raw samples. Counters alone do not prove memory reclamation; FPS alone does not
prove input responsiveness. Agree budgets after measurement, not by copying browser
numbers or turning noisy shared-runner timings into blocking thresholds.

## OS/CI limits and decisions still needed

Windows, macOS, and Linux are product targets; the current CI matrix is build/test
evidence only. The initial proposed X11 loop establishes neither native Wayland
support nor Windows/macOS GUI support, accessibility, keyring, provider, player,
or actual full-app behavior. Each needs separately authorized platform checks.
OS permission prompts and personal-desktop capture are not acceptable shortcuts.

Next decisions: approve the tiny milestone and any later tooling installation,
compile/exercise the source-verified 0.7.3 helpers in Kinema, resolve the WASM toolchain
compatibility gate, and name the isolated native runner/baseline reviewer. Optional
shared-Rust WASM offers an agent-friendly visual loop without reimplementing the
UI in HTML, but cannot establish native renderer/input/performance or OS parity.
Use separate HTML, WASM-backend and native-OS baselines, never pixel identity
between them. If WASM costs too much, use the fixture-native harness instead;
do not port backend services merely to make browser tests possible.

**Final research status:** this document incorporates the lead's native helper,
WASM implementation and actual public-gallery probe findings. Work by this document
owner was repository/tool/guide inspection, this new file and a review-guide link: no
Rust/Cargo/CI edits, installs, app starts, screenshots, or commits. The lead's
isolated public-browser screenshots are explicitly distinguished above. Preserve
all existing uncommitted source and the lead's tracked Rust hash prefix
`804a103…`; this documentation task neither changes nor recomputes that source
hash. Research is complete; the proposed harness milestones are not implemented.
