# Real-Kinema GPU regression workspace

GPU embedded mpv is now the normal/default embedded path. Ordinary builds use
`cargo run -p kinema-desktop --locked` from root, with no preparation. The public
feature is `gpu-surface`, default-on in desktop/backend; there is one Embedded
choice, not a fourth experimental player. This wrapper remains for regression
work only and uses the same durable [vendor sources](../../vendor/README.md).
It is not a second mpv client or this standalone harness's `Producer`.

Linux Intel Mesa qualification evidence for the standalone bridge is recorded in
[the qualification report](../gpui-mpv-gpu/QUALIFICATION.md). That does **not** by
itself qualify this real-app integration, multiwindow behavior, or every driver.
HDR remains deferred. Windows ANGLE/D3D11On12/D3D12 and macOS legacy
CGL/IOSurface/Metal paths are experimental and **native-runtime untested**, not
qualified platform backends; see [platform probes](../mpv-platform-probes/README.md).
No rendered-frame CPU map/readback/re-upload fallback is permitted in embedded
playback. GPU copies are allowed; software decoder uploads are a separate issue.
Missing capabilities must fail closed rather than silently selecting another
player/backend. Cancellation, generation rejection, bounded GPU mailbox ownership
and shutdown belong to the real player integration, not a launcher-side producer.

## Measured real-app GUI evidence — 2026-10-25

Lead-run offline `ui-probe` tests passed under the Vulkan validation layer:

| Synthetic SDR fixture | GPU surfaces | UI frame acceptances |
| --- | ---: | ---: |
| H.264 8-bit 1080p | 167 | 165 |
| HEVC 10-bit 4K | 166 | 164 |
| AV1 10-bit 4K | 165 | 163 |

Each probe had an **8-second total budget: 7 seconds playback plus 1 second
shutdown**, not 8 seconds of steady playback. All three retained source-sized
render dimensions, reached verified control phase 5, observed both requested
resizes, and exercised two auxiliary windows through paint/resize/close/reopen.
Presentation release and shutdown completed with no late frames, panics, GPU
errors, validation errors, threading errors or VUIDs. Reported readback bytes and
upload-byte estimates were zero; frame acceptances do **not** measure physical
presents. Fixtures had no audio, so audible playback was not tested. No accounts,
ordinary user configuration or providers were opened; the ordinary main-app
launcher was not run during validation.

Review artifacts: [GUI smoke report](../../target/kinema-gpu-app/gui-smoke-report.json)
and [raw lead probe log](/home/tlm/.nerve/data/conversations/conv_01M5P40MMVT3FR6J02EER8DRFV/tool-calls/tool_01M5QS7ZHBW2RTC47G5EMRK4R9/files/output.txt).
These are local generated evidence, not portable checked-in artifacts. This is
scoped Linux GUI evidence, not general platform qualification. Native Windows
and macOS linking/runtime and HDR remain user-deferred. Device-loss recovery and
restart are unsupported; audio pause remains best-effort.

## Prepare and build

From the repository root, using its Rust **1.97.1** toolchain:

```sh
# Safe policy checks: no root dependency resolution, accounts, services or UI.
python3 experiments/kinema-gpu-app/prepare.py --self-test
python3 experiments/gpui-mpv-gpu/prepare.py --self-test

# Optional staging while the lead coordinates root manifests/Cargo.lock:
experiments/kinema-gpu-app/run.sh prepare --manifests-only

# Only after root manifests, root lockfile and the selected stable patch set has settled:
experiments/kinema-gpu-app/run.sh prepare --update-lock
experiments/kinema-gpu-app/run.sh check
experiments/kinema-gpu-app/run.sh build
experiments/kinema-gpu-app/run.sh test
```

`--update-lock` intentionally writes **only** this directory's `Cargo.lock` and
the generated app lock. On first creation it seeds from the lead-owned root lock
and performs a workspace-only offline update in the generated directory, retaining
locked upstream versions rather than freshly selecting a new dependency graph.
Normal prepare/build/check/test/run use `--locked`; root metadata also uses
`--locked`. The lead alone updates the root `Cargo.lock`.
Do not run an unlocked root build to work around pending manifest coordination.
The generated workspace root is `target/kinema-gpu-app/`; `CARGO_TARGET_DIR` points
to root `target/` to reuse matching profiles/artifacts, not to mix lockfiles.

Prerequisites for this wrapper: Python 3.11+, Git, Cargo/toolchain, and normal
native GPUI/libmpv platform dependencies. There is no Linux-only guard in this
preparation script. On Windows, use a Bash environment for `run.sh`, or invoke Python/Cargo
commands directly. The `patch` command is needed only for optional vendor source
reproduction, not normal builds or regression preparation. Native/cross-build SDK requirements still apply. The macOS generated
catalog enables `gpui_apple/runtime_shaders` (as does the root catalog) so Rust cross-checking need not invoke
the Metal SDK compiler at build time: actual native Metal shader compilation is
still required at runtime. A cross-check is **not** a successful runtime test.

## Explicit user launch versus agent validation

For a **user-requested normal app launch**:

```sh
experiments/kinema-gpu-app/run.sh run [app arguments]
```

This launches the actual `kinema-desktop` binary, not the fixture probe, and
passes arguments directly without shell evaluation. It deliberately uses ordinary
application paths: debug builds use the repository's `data/` directory; ordinary
OS credential storage may also be accessed. It is **not** an account/settings/DB
sandbox. This wrapper uses the same default GPU embedded path as root builds. The `build` command
prints Cargo's actual executable path (including a custom target/profile or
Windows `.exe`), so users can also launch that binary deliberately.

**Agents must never invoke ordinary `run` or the main binary for testing.** Agent
UI/native validation is limited to lead-controlled offline fixture launches:

```sh
# All input paths must be absolute/local and synthetic.
experiments/kinema-gpu-app/run.sh run --probe --scene player
# Lead-controlled GPU embedded fixture, with generated local media:
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  experiments/kinema-gpu-app/run.sh run --probe --scene player-experimental \
  --media /absolute/generated/testsrc-h264.mkv --smoke-seconds 6 \
  --size 1100x680 --stress-resize
```

`run --probe` instead launches the existing offline `ui-probe` binary from a
fresh owned session CWD, so debug-build `data/` paths do not resolve to ordinary
user/repository data. Its available GPU scenes are controlled by the desktop
integration, not invented by this wrapper. Filesystem isolation alone does not
isolate OS secure storage: only the offline fixture contract makes this suitable
for agent validation. Do not use release builds, ordinary user DBs, external
services or stored credentials for validation. Probe session files are preserved
under the owned generated directory rather than deleting user/generated files.

### Native Python/Cargo commands (no Bash launcher required)

For example, in Windows PowerShell with native Python and Rust on PATH:

```powershell
python experiments/kinema-gpu-app/prepare.py --update-lock
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo build --manifest-path target/kinema-gpu-app/Cargo.toml --locked -p kinema-desktop --features gpu-surface --bin kinema-desktop
# USER launch only; never an agent test:
cargo run --manifest-path target/kinema-gpu-app/Cargo.toml --locked -p kinema-desktop --features gpu-surface --bin kinema-desktop
```

Git Bash is one way to run `run.sh`; it is not an implicit dependency of the
compiled app at runtime.
Cross-compilation remains unqualified runtime evidence, not a native platform pass.

## Source, manifest and patch isolation

`prepare.py` reuses the standalone harness's safety helpers, **not its playback
pipeline**. Root `cargo metadata --locked` must resolve all five exact package
releases to the promoted `vendor/` paths. The vendor inventory and identical
historical patch inputs are verified; neither script regenerates target patches
or modifies registry sources. Authentication/reproduction provenance is documented
in [`vendor/README.md`](../../vendor/README.md).

Missing/unknown patches, unreviewed root patch entries, unknown preexisting build
directories, symlinks, and checksum failures fail closed. Distinct ownership
markers prevent the app and standalone preparers from replacing each other's
generated trees. Existing generated patch-copy directories are left untouched.

The generated root copies root workspace package metadata, lints, profiles and
shared dependency versions, rebasing only the reviewed vendor paths to absolute
paths. Root and generated builds enable the same patched GPUI API features and
Apple runtime shaders; the production desktop owns Windows dependency forwarding.
No global patch or registry change is involved.

Three generated package manifests inherit from that generated root. Absolute
`[lib]`, default `[[bin]]`, `ui-probe`, integration test, example and benchmark
paths reference the **original** checked-in sources. Consequently relative
`include_bytes!`/`include_str!` paths resolve against original assets. Explicit
required-feature target gates are preserved. Only Git-tracked `assets/`,
`fixtures/` and `tests/fixtures/` support files are copied when preparing package
directories, supporting `CARGO_MANIFEST_DIR`-based tests without copying ignored or
generated user files. The ordinary source tree, `reference/cpp`, root lockfile,
account settings and secure-store entries are not migrated or rewritten.
