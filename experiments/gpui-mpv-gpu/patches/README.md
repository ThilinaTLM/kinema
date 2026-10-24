# Standalone GPU video patches

These patches apply only to copies of the exact crates.io releases below. Apply
with `patch --batch -p1 < PATCH` from each copied crate directory. Never modify the
Cargo registry. Do not add production workspace overrides. Generated manifests
must use the root shared version catalog and resolve a single **wgpu 29.0.4**.

| Patch | Copied crate | Upstream license |
| --- | --- | --- |
| `0001-gpui-pre-0.3.9.patch` | gpui-pre 0.3.9 | Apache-2.0 |
| `0002-gpui-pre-wgpu-0.3.9.patch` | gpui-pre-wgpu 0.3.9 | Apache-2.0 |
| `0003-wgpu-hal-29.0.4.patch` | wgpu-hal 29.0.4 | MIT OR Apache-2.0 |

GPUI provenance: https://github.com/zed-industries/zed, revision
`93d745668805029476e6d7fbb19652116fed51de` (gpui-pre 0.3.9 snapshot).
HAL provenance: https://github.com/gfx-rs/wgpu, wgpu-hal 29.0.4 release.
Retain upstream license files in the copied sources; modifications retain these
upstream licensing terms. Patch headers also carry provenance/license notices.

Original `.crate` archive SHA-256:

```text
34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0  gpui-pre-0.3.9
5b1cdb26c1503bf9662ee8fbc5ab0b44b2deeee4b5c12dc67491ba6fb5fc544b  gpui-pre-wgpu-0.3.9
97ace1c17727311c22a46e4e3faf56ea6de81af99dcc839bdfb54857b94d448d  wgpu-hal-29.0.4
```

## Public experiment API (Linux only)

Enable `gpu-video-experiment` on gpui and gpui_wgpu; the backend forwards the
feature to core. Feature-off builds do not expose the API or create GPU surface
resources. Core adds optional `wgpu = "=29.0.4"`, with defaults disabled and `std`
enabled, for Linux only. No other GPUI dependency is added. HAL's extension patch
has no feature switch: isolation comes from using only the copied crate in the
standalone generated workspace.

```rust,ignore
gpui::GpuSurfaceFrame::new(
    view: wgpu::TextureView,
    size: [u32; 2],
    owner: Arc<dyn Send + Sync>,
) -> Result<gpui::GpuSurfaceFrame, &'static str>;
gpui::gpu_surface(frame: Arc<gpui::GpuSurfaceFrame>) -> gpui::Surface;
gpui_wgpu::experimental_gpu_context() -> Option<gpui_wgpu::WgpuContext>;
gpui_wgpu::experimental_gpu_queue_gate() -> Option<Arc<std::sync::Mutex<()>>>;
```

`WgpuContext` already publicly exposes the actual `device: Arc<wgpu::Device>`,
`queue: Arc<wgpu::Queue>`, and `adapter: wgpu::Adapter`. No separate device is
created by the accessors. Accessors return `None` before a window renderer exists,
after release/drop, or on device loss; they are not headless-context factories.
The enabled Linux `WgpuContext::instance()` creates a **Vulkan-only** instance:
no GLES/EGL backend enumeration or GL fallback is allowed, so GPUI does not own
the producer's process-global EGL display. Feature-off instance selection stays
unchanged. The enabled window renderer also rejects non-Vulkan backends and a
second live window/context. Acquire context and gate together on the renderer's UI lifecycle
thread, and reacquire both after recovery. Producers must reject obsolete device
identities/generations; never submit old frames to a replacement renderer. wgpu
validates texture/device compatibility at bind-group creation, not in the frame
constructor. No unsafe native handle is accepted by GPUI.

## Frame, synchronization, and lifetime contract

- Fixed opaque SDR RGB probe: use a **default view** of a single-layer,
  single-mip, single-sample `Rgba8Unorm` 2D texture with `TEXTURE_BINDING` usage,
  on the renderer's device. Source alpha is ignored; element opacity is honored.
  There is no YUV/HDR conversion, color-management, or alternate-view support.
- Constructor validation checks nonzero/matching dimensions and underlying
  texture metadata. View subresource/format compatibility remains a caller
  contract and wgpu validation responsibility.
- Finish all producer writes and explicit ownership/visibility synchronization
  before publishing a frame. Published textures are immutable and must not be
  reused. GPUI only samples a normal texture; external import/copy is backend work.
- GL/Vulkan shared PBO handoff **requires explicit GL_EXT_semaphore_fd ↔
  VK_KHR_external_semaphore_fd dependencies** plus proper buffer ownership and
  barriers. `glFinish`, CPU fence waits, and the queue mutex alone are insufficient.
  The HAL patch only enables the optional Vulkan extension when supported; it
  neither proves semaphore handle compatibility nor implements synchronization.
- Every producer raw Vulkan queue submit and wgpu queue submission must hold the
  exact returned queue gate. GPUI guards its three submit paths and presentation
  with that same gate. Producer command recording, decoding, fence waits, and GPU
  polling stay outside the gate, on workers where blocking is needed. Treat mutex
  poisoning as fatal; never recover by bypassing the gate. The gate does not make
  arbitrary other wgpu queue/device operations safe for raw-Vulkan interop.
- **Existing framework wait exception:** surface configure and configured-surface
  destruction call HAL `vkDeviceWaitIdle`, which externally synchronizes all queues.
  The experiment's private surface owner holds the same queue gate for the entire
  configure/drop call, including the existing blocking wait on the UI thread during
  resize/recovery/teardown. This is not a UI-wait-free implementation. Submit and
  present scopes remain short; producer fence/poll waits never hold the gate.
  The producer must flush each GL-ready signal before submitting the corresponding
  Vulkan wait. No queued submission may depend on a future producer queue submit
  that would need this gate while framework configure waits for device idle.
- Frame owners remain alive in the element and scene through drawing; wgpu tracks
  submitted resources. Owner release is **not** a presentation fence, and there is
  no guarantee of actual presentation time. Do not use owner destruction to reuse
  external input textures. Retire producer work/resources explicitly on workers.
- Bounds, object fit, scene ordering, rectangular content masks, scissor reset,
  and normal compositor alpha modes are preserved. Rounded surface corners are
  not added (the existing surface element does not support them).

## Queue synchronization audit

The Linux feature uses a private `WgpuWindowSurface` owner (feature-off is a type
alias to the unchanged wgpu surface). Its configure method gates the entire
underlying call; Drop takes and destroys the actual surface before releasing the
gate. This also covers automatic drop and error unwinding, not just explicit
resize paths. Replacement surfaces inherit the current core's exact gate.

| Operation | Coverage |
| --- | --- |
| Normal scene submit, error-path staged-write flush, headless readback submit | `WgpuRendererCore::submit` |
| Surface texture present (`vkQueuePresentKHR`) | Existing same-core present guard |
| Initial configure, resize, alpha-mode change, suboptimal/lost/outdated recovery, replacement configure | Private surface owner's `configure` guard |
| Explicit unconfigure, replacement's old surface, destroy, device recovery state release, renderer automatic Drop | Private surface owner's Drop guard |
| Producer raw handoffs and normal wgpu copy submit | Producer's same returned gate; fence/poll waits outside it |

The original missing configure exclusion was reproduced with an owned synthetic
H.264 VAAPI fixture and controlled window resizes: Vulkan validation emitted
`UNASSIGNED-Threading-MultipleThreads-Write` for concurrent worker `vkQueueSubmit`
and UI `vkDeviceWaitIdle`. This diagnostic has no VUID; it remains a fatal GPU
error, with no suppression or logger-policy relaxation. Raw diagnostic logging
was used only in ignored scratch sources, not added to the checked-in app.

## Focused validation hooks

Core includes a pure dimension-guard unit test. The backend includes an ignored
`synthetic_rgba_surface_clips_and_blends` GPU test, enabled with
`gpu-video-experiment,test-support`; run it explicitly with `--ignored`. Its CPU
pixel upload and readback are **synthetic validation only**, never playback. It
checks opaque-source behavior, element opacity, rectangular clipping, dimensions,
and scene owner lifetime. A working Vulkan device is required. This does not
validate real mpv playback or GL/Vulkan external semaphore correctness.

The published upstream backend unit-test modules reference omitted Zed font
assets and an undeclared `naga` dev dependency. To run this focused test from
registry copies, the scratch validation disabled only those unrelated upstream
`#[cfg(test)]` modules, ran the explicit synthetic test, then restored the copied
sources. Those scratch-only exclusions are **not** part of these patches. An
unmodified upstream registry package's full unit-test target remains blocked by
those pre-existing packaging limitations.
