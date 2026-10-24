# Isolated real-Kinema experimental GPU app

This builds the **real** `kinema-core`, `kinema-mpv` and `kinema-desktop` sources in
an isolated generated workspace. It is not the standalone experiment's
`Producer`, a second mpv client, a new playback application, or a production
default. The opt-in `experimental-gpu` desktop feature enables the fourth
experimental embedded-mpv choice; the ordinary three choices/default stay
unchanged. The backend must use the existing `kinema_mpv::Player` client and its
control/audio/session lifecycle. Native FFI stays in `kinema-mpv`; desktop/core
retain their inherited unsafe-code prohibition.

Linux Intel Mesa qualification evidence for the standalone bridge is recorded in
[the qualification report](../gpui-mpv-gpu/QUALIFICATION.md). That does **not** by
itself qualify this real-app integration, multiwindow behavior, or every driver.
HDR remains deferred. Windows ANGLE/D3D11On12/D3D12 and macOS legacy
CGL/IOSurface/Metal paths are experimental and **native-runtime untested**, not
qualified platform backends; see [platform probes](../mpv-platform-probes/README.md).
No rendered-frame CPU map/readback/re-upload fallback is permitted in experimental
playback. GPU copies are allowed; software decoder uploads are a separate issue.
Missing capabilities must fail closed rather than silently selecting another
player/backend. Cancellation, generation rejection, bounded GPU mailbox ownership
and shutdown belong to the real player integration, not a launcher-side producer.

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

Prerequisites: Python 3.11+, Git, the `patch` command, Cargo/toolchain, and normal
native GPUI/libmpv platform dependencies. There is no Linux-only guard in this
preparation script. On Windows, use a Bash environment (e.g. Git Bash) with
`python3` and `patch` available; preparation can also be invoked with Python
without Bash. `patch` must be on the native Python process's PATH (for example
from Git for Windows's Unix tools); having Git installed does not implicitly
supply a runtime shell. Native/cross-build SDK requirements still apply. The macOS generated
catalog enables `gpui_apple/runtime_shaders` so Rust cross-checking need not invoke
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
sandbox. The existing three player defaults are unchanged; only this generated
feature-enabled build exposes the opt-in fourth choice. The `build` command
prints Cargo's actual executable path (including a custom target/profile or
Windows `.exe`), so users can also launch that binary deliberately.

**Agents must never invoke ordinary `run` or the main binary for testing.** Agent
UI/native validation is limited to lead-controlled offline fixture launches:

```sh
# All input paths must be absolute/local and synthetic.
experiments/kinema-gpu-app/run.sh run --probe --scene player
# Lead-controlled actual fourth-player fixture, with generated local media:
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

For example, in Windows PowerShell with native Python, Rust and `patch` on PATH:

```powershell
python experiments/kinema-gpu-app/prepare.py --update-lock
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo build --manifest-path target/kinema-gpu-app/Cargo.toml --locked -p kinema-desktop --features experimental-gpu --bin kinema-desktop
# USER launch only; never an agent test:
cargo run --manifest-path target/kinema-gpu-app/Cargo.toml --locked -p kinema-desktop --features experimental-gpu --bin kinema-desktop
```

Git Bash is one way to supply the preparation-time `patch` executable and run
`run.sh`; it is not an implicit dependency of the compiled app at runtime.
Cross-compilation remains unqualified runtime evidence, not a native platform pass.

## Source, manifest and patch isolation

`prepare.py` reuses the standalone preparation's source-integrity policy, **not
its playback pipeline**. Root `cargo metadata --locked` identifies exact registry
sources; root locked package checksums authenticate `.cargo-checksum.json` maps
or cached `.crate` archives, and every extracted file is verified before copying.
Licenses are retained. Registry sources are never patched or rewritten.

Exactly these five checked-in patches and the lead's exact root `gpui_windows`
catalog entry are required. Unknown or missing patches fail closed; the temporary
four-patch staging mode is retired. `patch -p1` runs **inside** the corresponding
copied crate:

| Patch | Exact copied crate |
| --- | --- |
| `0001-gpui-pre-0.3.9.patch` | `gpui-pre` 0.3.9 |
| `0002-gpui-pre-wgpu-0.3.9.patch` | `gpui-pre-wgpu` 0.3.9 |
| `0003-wgpu-hal-29.0.4.patch` | `wgpu-hal` 29.0.4 |
| `0004-gpui-pre-apple-0.3.9.patch` | `gpui-pre-apple` 0.3.9 |
| `0005-gpui-pre-windows-0.3.9.patch` | `gpui-pre-windows` 0.3.9 |

The patch directory belongs to the GPUI patch owner. Missing/unknown patches,
unknown preexisting build directories, symlinks, and checksum failures fail
closed. Distinct ownership markers prevent the app and standalone preparers from
replacing each other's generated trees. No full vendor tree is checked in.

The generated root copies root workspace package metadata, lints, profiles and
shared dependency versions. Only its copied catalog adds `gpu-video-experiment`
to `gpui`, `gpui_wgpu`, and `gpui_apple` (plus Apple's `runtime_shaders`). Its generated catalog also enables
`gpui_windows/gpu-video-experiment`, and only the generated desktop manifest adds
an optional Windows `gpui_windows` dependency plus `dep:gpui_windows` to
`experimental-gpu`. No production feature forwarding is required. Only the
generated root has `[patch.crates-io]` for the selected copied crates. No root/global patch or
registry change is involved. The root desktop feature need not name unknown GPUI
features; normal root checks continue against upstream releases.

Three generated package manifests inherit from that generated root. Absolute
`[lib]`, default `[[bin]]`, `ui-probe`, integration test, example and benchmark
paths reference the **original** checked-in sources. Consequently relative
`include_bytes!`/`include_str!` paths resolve against original assets. Explicit
required-feature target gates are preserved. Only Git-tracked `assets/`,
`fixtures/` and `tests/fixtures/` support files are copied when preparing package
directories, supporting `CARGO_MANIFEST_DIR`-based tests without copying ignored or
generated user files. The ordinary source tree, `reference/cpp`, root lockfile,
account settings and secure-store entries are not migrated or rewritten.
