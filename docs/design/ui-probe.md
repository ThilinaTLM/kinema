# Isolated shared-Rust UI probe

Isolated fixture feasibility harness, not full-app acceptance. The original
render-only `ui-probe` counter, focusable action and disabled action remain the
default native/browser specimen. Native `--gallery` additionally exercises actual
shared Graphite theme/buttons/badge/cards/grid metrics with synthetic data and art;
`--light` selects its light palette. It does **not** construct entire workflow
screens or the desktop shell, workers, settings, database, credentials, providers
or player. `ui-probe-test` adds Kit headless behavior/layout support, not pixels.

**Browser: UNRUN, stable-WASM build blocked. Native isolated Linux pixels/input:
verified on the counter; final dark/light/hover gallery captures lead-reviewed.**
See [native implementation status](native-implementation.md) for current code,
remaining acceptance and evidence separation. WASM is not required for native
feedback. To open only the fixture (not the real app):

```sh
cargo run -p kinema-desktop --bin ui-probe --features ui-probe --locked
cargo run -p kinema-desktop --bin ui-probe --features ui-probe --locked -- --gallery --light
```

## Browser build and run (currently blocked)

From the repository root, with Rust **1.97.1**, the WASM target, Node 20+ and
Playwright **1.62.1** (including its matching Chromium) installed:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked \
  --root /tmp/kinema-ui-probe-tools
bash scripts/ui-probe/build.sh
node scripts/ui-probe/verify.cjs
```

`KINEMA_WASM_BINDGEN` can select another CLI executable of exactly 0.2.129.
`KINEMA_UI_PROBE_BUILD_DIR` changes the generated bundle location (also pass
`--build PATH` to the verifier). No toolchain or lockfile changes are requested.
The default bundle/artifacts directories are under ignored `target/ui-probe/`.

Normally `require('playwright')` resolves the runner dependency. For an isolated
portable install (outside the repository):

```sh
npm install --prefix /tmp/kinema-ui-probe-node playwright@1.62.1
/tmp/kinema-ui-probe-node/node_modules/.bin/playwright install chromium
export KINEMA_PLAYWRIGHT_MODULE=/tmp/kinema-ui-probe-node/node_modules/playwright
```

This host uses an already installed matching package:

```sh
export KINEMA_PLAYWRIGHT_MODULE=/home/tlm/Projects/nerve/node_modules/.pnpm/playwright@1.62.1/node_modules/playwright
node scripts/ui-probe/verify.cjs --artifacts /tmp/kinema-ui-probe-candidate
```

No persistent server or personal browser profile is used. The runner owns an
HTTP server bound only to `127.0.0.1` on port 0 and a temporary browser context;
both close on success, failure and termination signals. It serves only the built
bundle. Browser network interception rejects all requests outside that server.
Fonts/icons must be embedded or bundled; no CDN fallback is accepted. The loader
calls `run(asset_endpoint: String)` and exposes only the read-only
`probe_snapshot() -> String` adapter. Counter changes must come from real canvas
mouse/keyboard events, never a JS state mutation or action-dispatch export.

The runner forces bounded **WebGL2 ANGLE SwiftShader** software rendering with
WebGPU disabled, 800×600 logical pixels, DPR 1, UTC, en-US and reduced motion.
This is browser fixture evidence, not native GPU performance or Linux native
pixels. Native Kit headless behavior/layout tests and macOS Metal pixel tests are
separate verification layers.

## Evidence and baselines

The report records snapshots, console diagnostics, page errors, asset responses,
blocked network attempts, browser version and pixel counts. PNGs include first
paint, repeated first paint, clicked, keyboard-focused and keyboard-activated
states, plus counter-readout ROI captures and magenta pixel-difference PNGs.
A nonblank frame and exact repeated-frame match are required. The runner clicks
the disabled action (counter unchanged), clicks increment, cycles Tab to its
reported focus ID, then presses Enter (counter changes again). Both transitions
must change actual pixels inside the observed `probe.counter` bounds, which may
not overlap either action: button hover/focus changes alone cannot pass. The
report explicitly records `visualChecks.glyphAndCounterRegionChecked`. These
readout checks prove repaint, not OCR or full typography/design approval.

Without `--baseline`, output is an **unapproved candidate**, not a visual baseline
pass. Review `first-frame.png`, then explicitly accept it:

```sh
node scripts/ui-probe/verify.cjs --baseline target/ui-probe/baselines/webgl2.png --accept-baseline
node scripts/ui-probe/verify.cjs --baseline target/ui-probe/baselines/webgl2.png
```

Acceptance is explicit and occurs only after behavior/network/error checks pass.
Baseline paths must be outside the artifact output directory, including with
`--accept-baseline`, so screenshot capture cannot overwrite its comparison oracle.
Ordinary runs never update baselines. Comparison is exact RGBA pixel equality;
keep browser/font/viewport/backend environments separate. The decoder accepts
only trusted generated 8-bit RGB/RGBA noninterlaced PNGs, with bounded positive
dimensions (≤4096 per axis, ≤4 million pixels), checked chunk/header/end structure,
exact inflated size and bounded zlib output. It is not a general PNG decoder and
does not validate CRCs. A built-in negative control flips one pixel in a copy, requires the same comparator to reject it,
and never writes the modified image or an accepted baseline. The standalone
comparator also demonstrates a real failing exit status without file changes:

```sh
node scripts/ui-probe/compare.cjs BASELINE.png ACTUAL.png
# Must exit 1 even when given the same PNG twice:
node scripts/ui-probe/compare.cjs BASELINE.png BASELINE.png --negative-control
node --test scripts/ui-probe/pixels.test.cjs
```

Runner failure returns a nonzero exit code and preserves `report.json`; PNGs
already captured remain.

## Linux native-pixel feasibility fallback

This is a separate, bounded experiment, not the browser harness or native Kit's
headless tests. It never uses an inherited display or normal app executable:

```sh
cargo build -p kinema-desktop --bin ui-probe --features ui-probe --locked
# Optional public runtime Vulkan ICD; no hardcoded default or global env change:
export KINEMA_UI_PROBE_VULKAN_ICD=/path/to/public/runtime/vk_swiftshader_icd.json
node scripts/ui-probe/native-verify.cjs /tmp/kinema-ui-probe-native-feasibility
# Reviewed fixture coordinates only; not a general semantic native input driver:
node scripts/ui-probe/native-verify.cjs /tmp/kinema-ui-probe-native-input --input
# Offline shared-component gallery, not full workflow screens:
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-dark --gallery
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-light --gallery --light
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-hover --gallery --hover 150,200
node scripts/ui-probe/native-verify.cjs /tmp/kinema-graphite-review-focus --gallery --focus 140,210
```

The Linux-only runner starts its own Xvfb with `-displayfd`, rejects display :0,
unsets Wayland variables, uses temporary HOME/XDG directories and forces X11.
It finds only the fixture-titled window on that owned display and captures only
that window with ImageMagick. Child process groups and temporary directories are
cleaned on completion/failure/signals. No packages or system configuration are
changed. Xvfb is not a software Vulkan implementation: missing a usable Vulkan
adapter is an expected blocker. `KINEMA_UI_PROBE_VULKAN_ICD` sets both
`VK_DRIVER_FILES` and `VK_ICD_FILENAMES` **only on the fixture child**. The tested
public runtime path was
`/home/tlm/.cache/ms-playwright/chromium-1234/chrome-linux64/vk_swiftshader_icd.json`;
no browser profile/user data was opened. The native selected adapter is not
independently reported, so this does not prove hardware selection/performance.

A candidate PNG still requires image review; there is no native render-ready
acknowledgment or semantic pixel/input adapter. `--input` uses the reviewed
800×600 image: actions centered at `(152,216)` and `(152,276)`, counter ROI
`(32,136,240,40)`. It injects XTEST pointer input, then focuses the owned window
and sends Tab/Return. It checks disabled counter-pixel stability, counter-only
repaint for each activation, repeated-frame equality and a negative control.
Counter values are image-reviewed, not exported Rust state or OCR.

Gallery `--focus X,Y` clicks a fixture card, gives the owned window focus, then
moves the pointer outside all cards before capture. The lead inspected
`/tmp/kinema-graphite-final-focus/native-focused.png`: the outer focus ring and
bounded disclosure remain visible without pointer hover. This is focused
appearance evidence, not proof of initial keyboard-only entry.

## Native headless checks and lead validation

### Final Graphite increment (2026-10-17)

Serialized root fmt, strict locked workspace Clippy/tests and explicit fixture
Clippy/tests passed: **331 workspace passes / 3 ignored** (core 26, desktop library
17, desktop binary 288); explicit feature tests **20 passed** (17 overlap + 3
actual GPUI tests, **not 351 unique**). Node PNG/comparator tests: **5 passed**.
Logs: `/tmp/kinema-graphite-final-validation/{fmt,clippy,tests,probe-clippy,probe-tests,tool-tests}.log`.

The lead visually inspected final offline `native-first-frame.png` captures in
`/tmp/kinema-graphite-final-{dark,light,hover}` and `native-hover.png` in the hover
directory. The obsolete IMDb-identity strip is absent; valid zero/unknown/failed
art labels and bounded opaque hover facts are visible. Default counter input in
`/tmp/kinema-graphite-final-counter` passed stable-frame, disabled stability,
**0 → 1 → 2** repaint and negative-control checks. See
[final native evidence](native-implementation.md#final-validation-evidence) for
scope, commits and limits. This is not real-app, private-data, provider, native
performance or Windows/macOS GUI evidence. Stable WASM remains blocked.

### Commands and historical checkpoint

```sh
cargo test -p kinema-desktop --lib --features ui-probe-test --locked
cargo clippy -p kinema-desktop --lib --bin ui-probe --features ui-probe-test --locked -- -D warnings
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

**Historical pre-Graphite checkpoint** — lead-reported results (2026-10-17): all checks above passed; workspace tests had
295 passes/3 ignored and the explicit fixture test had 1 pass. Logs:
`/tmp/kinema-ui-probe-validation-20261017/{fmt,clippy,tests,probe-clippy,probe-tests}.log`.
Native headless evidence covers geometry, disabled pointer behavior, pointer
activation and Tab wrapping/Enter **after a pointer click**. Initial Tab entry
is not proven. These tests do not produce Linux native pixels.

Design checkpoint is committed as `11f767b`; core/CI checkpoint as `378441b`.
The lead independently reran native capture/input in
`/tmp/kinema-ui-probe-lead-native/`, reviewed the actual counter glyphs **0 → 1 → 2**,
and confirmed an exact cross-run first-frame match (zero changed pixels). Five
Node tests passed; the standalone corrupted-pixel negative control exited 1.
These are reviewed fixture candidates, not approved product baselines. Tooling
and documentation are checkpointed separately from the native core commit.
The explicit CI fixture step uses `ui-probe-test`; default workspace all-target
checks remain in place. Lead's dependency-feature proof is under
`/tmp/kinema-ui-probe-proof-20261017`: optional inactive Fast packages in the lock
graph are not an active Fast engine. The lock adjustment resolved the initial
cocoa conflict (cocoa 0.26.0/unicode-properties 0.1.3).

## Status

- Installed isolated `wasm-bindgen-cli 0.2.129` under
  `/tmp/kinema-ui-probe-tools`; did not modify the project toolchain.
- Harness syntax checks and five PNG decode/comparator/CLI unit tests passed,
  including malformed header/chunk/length, decompression-bound rejection, and
  baseline/artifact overlap rejection with and without explicit acceptance.
  The CLI negative control exits 1 and leaves baseline bytes unchanged.
- After the parallel Rust owner's lockfile work resolved the initial optional
  dependency cocoa conflict, the actual WASM retry reached compilation and failed
  with **E0554** in
  `wasm_thread 0.3.3`: its WASM path enables unstable
  `stdarch_wasm_atomic_wait`. Command: `bash scripts/ui-probe/build.sh`;
  log: `/tmp/kinema-ui-probe-build.log` (2026-10-17).
- **Browser UNRUN:** no local WASM bundle, browser PNG/input pass or accepted
  browser baseline exists. Runner assertions are implemented but unexecuted
  against the real browser fixture. Rust 1.97.1 remains unchanged: no nightly,
  `RUSTC_BOOTSTRAP`, dependency patch or second GPUI was used.
- Native build passed; log `/tmp/kinema-ui-probe-native-build.log`. Early runs
  timed out in exact Unicode-title matching despite a titled fixture appearing
  in xdotool enumeration. This was not proof of GPU failure or an unmapped
  window. The final runner matches the ASCII fixture-title marker, can map only
  the owned fixture if needed, and records any such mapping. One obsolete
  diagnostic used missing `xwininfo`; no package was installed.
- **Actual native capture/input passed**, with the public ICD override above,
  owned Xvfb `:1`, temporary HOME/XDG directories and cleaned child processes.
  `/tmp/kinema-ui-probe-native-feasibility-swiftshader/` holds the first reviewed
  800×600 PNG (696 colors). `/tmp/kinema-ui-probe-native-input-swiftshader/` holds
  `report.json`, full screenshots, counter crops and magenta diffs. Agent image
  review confirmed the fixture badge and counter **0 → 1 → 2** after disabled
  click (still 0), increment click, then Tab/Return. Counter-only changed pixels:
  disabled **0**, pointer **84**, keyboard **56**. Repeated first frame and
  cross-run first-frame candidate comparison both had **0** changed pixels
  (`cross-run-comparison.json`). No native semantic-ID/focus oracle, initial
  Tab entry, cross-platform support or full-app behavior is claimed.
- The same comparator rejected a deliberate one-pixel mismatch on an actual
  native PNG with exit **1**, leaving its SHA-256 unchanged; see
  `negative-control.log` in the native-input artifact directory. No accepted
  baseline was read or updated. These are reviewed candidates and stability
  evidence, not production visual approval. MESA-EGL DRI3 warnings and Xvfb
  keymap/allocation diagnostics remain recorded; successful pixels do not erase
  them. No hardware/native performance claim is made.
