# Isolated experimental GPU-surface patches

The canonical production build inputs now live in [`vendor/`](../../../vendor/README.md),
including identical copies of these five patch inputs. Root `[patch.crates-io]`
is intentional and uses authenticated exact-release sources. Ordinary root builds
need no preparation; the regression scripts consume those same sources. Never
modify registry sources or global Cargo configuration. For optional reproduction,
use `vendor/verify.py --reproduce NEW_DIRECTORY` (zero-fuzz patching).
Resolve one **wgpu 29.0.4**.

| Patch | Copied crate | License |
| --- | --- | --- |
| `0001-gpui-pre-0.3.9.patch` | gpui-pre 0.3.9 | Apache-2.0 |
| `0002-gpui-pre-wgpu-0.3.9.patch` | gpui-pre-wgpu 0.3.9 | Apache-2.0 |
| `0003-wgpu-hal-29.0.4.patch` | wgpu-hal 29.0.4 | MIT OR Apache-2.0 |
| `0004-gpui-pre-apple-0.3.9.patch` | gpui-pre-apple 0.3.9 | Apache-2.0 |
| `0005-gpui-pre-windows-0.3.9.patch` | gpui-pre-windows 0.3.9 | Apache-2.0 |

GPUI provenance: https://github.com/zed-industries/zed, snapshot revision
`93d745668805029476e6d7fbb19652116fed51de`. HAL provenance:
https://github.com/gfx-rs/wgpu, wgpu-hal 29.0.4 release. Retain the copied upstream
license files; modifications retain upstream licensing terms. Patch headers also
carry provenance/license notices.

Original `.crate` archive SHA-256:

```text
34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0  gpui-pre-0.3.9
5b1cdb26c1503bf9662ee8fbc5ab0b44b2deeee4b5c12dc67491ba6fb5fc544b  gpui-pre-wgpu-0.3.9
97ace1c17727311c22a46e4e3faf56ea6de81af99dcc839bdfb54857b94d448d  wgpu-hal-29.0.4
b52f58893fe1a0836464428dfad2b5eaf42c9c303a9638f4c2aced5a69072a15  gpui-pre-apple-0.3.9
513d4f0fc7d596d3bdef2a73a524ffb83dabc95bf484f2855a698e7f431290a9  gpui-pre-windows-0.3.9
```

Reviewed Windows adapter patch SHA-256 (distinct from the upstream archive
checksum above):

```text
28b74a24c6f438a2addae9a6332bc92f2f479031416e58ef285810117b7be810  0005-gpui-pre-windows-0.3.9.patch
```

## Features and dependencies

Enable `gpu-video-experiment` on core and the active renderer crate. Both WGPU
and Apple renderer features forward it to core. On Windows, also enable
`gpui_windows/gpu-video-experiment`: patch 0005 activates the HWND WGPU adapter and
forwards the core/WGPU features. The root desktop manifest has an optional Windows-target `gpui_windows`
dependency forwarded by its default-on `gpu-surface` feature; the root catalog
enables the underlying patched API features. Feature-off builds do not expose the frame/context
APIs or allocate experimental pipelines/registrations, and retain the legacy
Windows scene renderer.

Core's added dependencies are optional, platform-specific:

- Linux/Windows: `wgpu = "=29.0.4"`, defaults disabled, `std` enabled.
- macOS: `objc2-io-surface = "0.3.2"`, defaults disabled, `std`, `IOSurfaceRef`,
  and `objc2-core-foundation` enabled. CoreVideo's `CVPixelBufferIOSurface` and
  `objc2-io-surface` features enable safe metadata-only backing validation.

Apple retains its existing `cbindgen 0.28` version as a host build dependency
rather than a target-gated dependency. With `runtime_shaders`, its target-aware
build script generates a real header and stitches the real shader source on a
Linux cross-build host. Native Apple builds without `runtime_shaders` retain the
normal `xcrun metal`/`metallib` path. Runtime shader source stitching is **not**
Metal shader compilation or validation. The root catalog enables `runtime_shaders`; actual Metal compilation then occurs
on the Apple device.

HAL's optional extension-enable patch has no feature switch: root and regression
workspaces select the same repository-owned HAL source.

## Public API

Linux/Windows:

```rust,ignore
gpui::GpuSurfaceFrame::new(
    view: wgpu::TextureView,
    size: [u32; 2],
    owner: Arc<dyn Send + Sync>,
) -> Result<gpui::GpuSurfaceFrame, &'static str>;
gpui_wgpu::experimental_gpu_context() -> Option<gpui_wgpu::WgpuContext>;
// Canonical atomic device/queue/gate capture from the same context:
context.experimental_gpu_queue_gate() -> Arc<std::sync::Mutex<()>>;
// Unique-global diagnostic accessor also remains available:
gpui_wgpu::experimental_gpu_queue_gate() -> Option<Arc<std::sync::Mutex<()>>>;
```

macOS:

```rust,ignore
// MacGpuSurfaceFrame is a macOS-only type alias of GpuSurfaceFrame.
gpui::MacGpuSurfaceFrame::new(
    buffer: CFRetained<CVPixelBuffer>,
    size: [u32; 2],
    owner: Arc<dyn Send + Sync>,
) -> Result<gpui::MacGpuSurfaceFrame, &'static str>;
gpui_apple::experimental_gpu_registry_id() -> Option<u64>;
```

All three platforms: `gpui::gpu_surface(Arc<gpui::GpuSurfaceFrame>) -> gpui::Surface`.
The usual element styling and object fit apply. No iOS experimental API is added.

### WGPU context and multiwindow contract

`WgpuContext` already exposes the actual `device: Arc<wgpu::Device>`,
`queue: Arc<wgpu::Queue>`, and `adapter: wgpu::Adapter`. With the feature enabled
on Linux/Windows, the `GpuContext` slot is
`Rc<RefCell<Option<Arc<WgpuContext>>>>`; feature-off retains upstream's by-value
slot. Manual context insertion must use an `Arc`; ordinary `None` initialization
is unchanged. A private gate is owned by the context and shared by every context clone, including producers retaining a
context after the final window closes. Weak, per-actual-device renderer records
share one `Arc<WgpuContext>` and gate across windows; wgpu's dispatch identity,
not adapter name or a guessed GPU, is compared. Independent empty platform context
slots adopt the unique live context before creating a surface/instance; lost
slots also adopt a healthy replacement already recovered by another window.
Recovery checks this registry before creating a replacement device, so independent
slots do not fork the device after loss. A same-device/different-queue context is
rejected.

Multiple windows are supported. Accessors return `None` before renderer creation,
after all renderer registrations are released/lost, or when live renderers have
multiple distinct devices. Returned context/gate clones do **not** keep a live
renderer registration alive. Headless devices are not registered as window
contexts. Query the context once, then obtain its gate with
`context.experimental_gpu_queue_gate()`; do not issue two global queries that can
race across window lifecycle changes. Reacquire a new context and its gate after
recovery. Reject obsolete device identities/generations and never
publish old-device frames to a replacement renderer.

The enabled Linux WGPU instance is **Vulkan-only**, with no GLES/EGL enumeration
or GL fallback (the producer owns its EGL lifecycle). The enabled Windows WGPU
instance is **DX12-only**. Feature-off selection is unchanged. **Platform wiring
matters:** upstream `gpui-pre-windows 0.3.9` directly constructs `DirectXRenderer`.
Patch 0005 is therefore required, not optional, for experimental Windows app
presentation. Never substitute a second/private WGPU device to conceal a missing
platform adapter.

### Windows HWND adapter (patch 0005)

The feature switches the platform's window renderer alias to a thin
`WgpuWindowRenderer`. Scene drawing/acquisition/submission/presentation use WGPU
Dx12 exactly once; the existing D3D11 device remains for font rasterization/vsync,
not a second video or scene renderer. A UI-thread `GpuContext` slot is shared
across HWNDs and uses patch 0002's actual context/gate factory. The adapter rejects
CPU adapters and non-Dx12 backends; legacy D3D11 device notifications never replace
the video renderer's device. Actual WGPU loss is terminal/restart-required rather
than in-place replacement with retained video frames.

The feature disables `WS_EX_NOREDIRECTIONBITMAP`, since this is an HWND swapchain,
not a DirectComposition visual. Renderer destroy runs before explicit
`DestroyWindow` and at `WM_DESTROY` (before HWND invalidation at `WM_NCDESTROY`);
the window state is retained across deferred native destruction. Patch 0002 owns
all configured-surface teardown/configure gate scopes. Draw-false conditions
request a forced redraw on the next native frame rather than invoking a second
renderer. Dimensions are device pixels; negative/over-limit extents are rejected,
and zero-size draws are skipped without configuring a 1×1 video presentation.

**Read-only review findings resolved in the patch hash recorded above:** the
feature-only minimize path records zero drawable size without configuring or
presenting it, and repeated minimize messages preserve the saved frame callback.
The first restore queries the physical client extent and resizes WGPU before
resuming drawing. Maximized DPI fallback likewise uses `GetClientRect`, not monitor
work-area/outer dimensions; ordinary resize also avoids WM_SIZE's packed 16-bit
extent. Legacy message handling is unchanged. The patch owner reports feature-on/
off Windows cross-checks, strict Clippy, and zero-fuzz application passing. Native
Windows linking/GPU runtime, including message-pump/reentrant destruction behavior,
remain explicitly deferred; compilation and source review are not qualification.

### WGPU frame and queue contract

- Use a default 2D view of a single-layer/mip/sample `Rgba8Unorm` **or**
  `Bgra8Unorm` texture with `TEXTURE_BINDING` usage on the renderer's actual device.
  Metadata/dimensions are checked by the constructor; view format/subresource
  compatibility is a caller contract, and wgpu validates device binding.
- Opaque SDR RGB only: source alpha is ignored, element opacity is honored. No
  HDR/YUV conversion, alternate views, or CPU pixel map/readback is added.
- Complete producer writes and cross-API ownership/visibility synchronization
  before publishing. Published textures are immutable and must not be reused.
  External import and normal-texture GPU copy belong to the native backend.
- Every raw producer queue submit and normal wgpu submit must use the exact
  returned gate. Submission and presentation hold it only for their queue calls.
  Producer recording, decoding, fence waits, and polling remain outside it.
- **Existing framework wait exception:** configure and configured-surface teardown
  invoke HAL device-idle waits. They must hold the same gate for their entire
  call, including existing UI-thread resize/recovery/drop waits. This is **not**
  a UI-wait-free renderer. Treat gate poisoning as fatal; never bypass it.
- Every queued external wait must already have its matching ready signal flushed.
  No queued work may require a future producer queue submit needing the gate
  while configure waits for idle. The gate alone is not a GPU memory dependency.
- Linux handoff still requires explicit GL_EXT_semaphore_fd ↔
  VK_KHR_external_semaphore_fd dependencies and proper ownership/barriers.
  `glFinish` or CPU fence waits alone are insufficient. The HAL patch only enables
  the supported optional extension; it does not implement or prove handoff.
- Owners remain alive through element/scene drawing; wgpu tracks submitted native
  resources. Owner release is **not** a presentation fence or permission to reuse
  external inputs. Retire native work/resources explicitly on workers.

The feature's private surface owner covers all configure/drop paths, not just
explicit resize. Replacement surfaces inherit the current core's exact gate.

| Operation | Gate coverage |
| --- | --- |
| Scene submit, staged-write error flush, headless readback submit | Core submit helper |
| Surface texture present | Same-core present guard |
| Initial configure, resize, alpha change, suboptimal/lost/outdated recovery, replacement configure | Private surface configure guard |
| Unconfigure, old-surface replacement, destroy, device recovery state release, automatic Drop/unwind | Private surface Drop guard; underlying surface is destroyed before unlocking |

The earlier missing configure exclusion was reproduced with owned synthetic H.264
and controlled window resizes: `UNASSIGNED-Threading-MultipleThreads-Write` reported
worker `vkQueueSubmit` concurrent with UI `vkDeviceWaitIdle`. No VUID is present;
it remains a fatal GPU error, with no suppression/logger relaxation. Raw-message
logging was scratch-only, never an option in the checked-in app.

### Apple Metal frame and lifetime contract

Registry lookup uses IDs from **actual live window Metal renderers**. Registrations
are weak and renderer-owned; destroy/drop removes ownership. No default-device
guess or headless device is used. Multiple renderers with one registry ID return
that ID; zero IDs, no live renderer, or different live IDs return `None`.

Publish a fresh immutable, nonplanar, IOSurface-backed **BGRA32 SDR** CVPixelBuffer
and its native frame owner, after producer GPU work is complete. Constructor
validation uses only safe metadata queries. The actual renderer's existing
CVMetalTextureCache imports a BGRA8Unorm texture. A separate opaque-RGB fragment
pipeline uses existing surface geometry/clip distances and element opacity; the
NV12/CoreVideo path and its shader remain unchanged. Import failure logs an error
and draws no experimental surface; it does not fall back to NV12 or CPU pixels.

The optional frame reference travels in PaintSurface. The command-buffer
completion handler retains the frame/owner, pixel buffers, and CVMetalTexture
wrappers until completion, not merely scene drop or command encoding. Completion
still does **not** guarantee actual presentation time. Existing rounded-surface
corners are not extended on any backend.

## Validation limits and hooks

Root app/player controls, generations, bounded mailbox, cancellation, audio,
shutdown, and provider lifecycle belong to the integration/native backend, not
these patches. HDR is deferred. **Windows/macOS native runtime and Metal shader
execution are unqualified**; cross-compilation alone must not be reported as
playback or OS support. Linux qualification/stress is serialized by the lead.

Core includes a pure dimension guard test; Apple includes a pure weak-registry
uniqueness/lifetime policy test; the Windows adapter includes drawable-bounds
policy tests. The backend's ignored
`synthetic_rgba_surface_clips_and_blends` test explicitly uses **test-only** CPU
upload/readback, never playback, and requires a Vulkan device. It checks clipping,
opacity, opaque-source alpha, dimensions, and owner lifetime.

Published upstream backend unit tests reference omitted Zed font assets and an
undeclared naga dev dependency. Earlier focused scratch validation disabled only
those unrelated upstream `#[cfg(test)]` modules, then restored them; those
exclusions are not part of these patches. Full upstream unit-test targets retain
those pre-existing packaging limitations.
