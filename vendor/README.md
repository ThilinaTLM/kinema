# Repository-owned GPU build inputs

Root Cargo uses these exact crates.io releases through `[patch.crates-io]`.
`cargo run -p kinema-desktop --locked` requires no preparation, target-directory
sources, registry modification, or global Cargo configuration. The desktop/backend
feature is `gpu-surface` (default-on); the patched upstream API feature retains
its internal name `gpu-video-experiment`. That spelling does not make the normal
embedded path an experimental app setting.

| Directory | Release | Upstream license |
| --- | --- | --- |
| gpui-pre | 0.3.9 | Apache-2.0 |
| gpui-pre-wgpu | 0.3.9 | Apache-2.0 |
| gpui-pre-apple | 0.3.9 | Apache-2.0 |
| gpui-pre-windows | 0.3.9 | Apache-2.0 |
| wgpu-hal | 29.0.4 | MIT OR Apache-2.0 |

Sources were extracted directly from cached `.crate` archives after authenticating
SHA-256 against the pre-promotion root Cargo.lock, then patched with zero fuzz.
`provenance.json` records original archive identities/checksums and patch hashes;
`sources.sha256.json` records every resulting source file. All archive assets,
examples, manifests, license texts, and platform build inputs are retained. Cargo
registry extraction markers, build outputs, and caches are not included.

GPUI upstream: <https://github.com/zed-industries/zed>, revision
`93d745668805029476e6d7fbb19652116fed51de`. HAL upstream:
<https://github.com/gfx-rs/wgpu>, wgpu-hal 29.0.4. The five `patches/` inputs were
promoted unchanged from the GPU experiment. Their headers preserve provenance and
license notices; modifications retain upstream licensing terms. Each crate keeps
its original license files.

## Verification and maintenance

From the repository root:

```sh
python3 -B vendor/verify.py
# Optional full offline reproduction; destination MUST NOT already exist:
python3 -B vendor/verify.py --reproduce /path/to/new-directory
```

Reproduction reads only authenticated Cargo archive-cache inputs and writes only
the explicitly new destination. It does not modify this directory or the registry.
Normal Cargo builds do not run either command. Updating patches requires reviewing
the patch hashes, exact patched sources, source inventory, and root lock together.
Do not update these inventory files merely to silence an integrity failure.

## Behavior and limits

The patches add GPUI GPU-surface ownership/presentation APIs, shared WGPU context
and queue synchronization, the Windows HWND WGPU adapter, Apple surface metadata
validation/runtime shader build inputs, and optional Vulkan external-semaphore
extension enablement in HAL. Root catalog enables the relevant GPUI features and
Apple `runtime_shaders`; the latter stitches shader sources at build time and
compiles Metal shaders on the actual Apple runtime, not on a Linux build host.

Promotion changes the supported build entry point, not native qualification.
Windows/macOS runtime/linking/GPU behavior and HDR remain unqualified. Keep the
detailed [API/synchronization constraints](patches/README.md) with these patches.
The experimental harnesses are retired; their
[historical qualification record](../docs/validation/gpu-surface-qualification.md)
is not current root validation. Use the [root validation instructions](../docs/development.md#run-and-validate)
and offline `ui-probe` fixture path for new checks, not retired wrapper commands.
No desktop CPU readback fallback is introduced here.
