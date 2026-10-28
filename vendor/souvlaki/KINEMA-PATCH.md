# Kinema patch to souvlaki 0.9.3

Base: crates.io souvlaki **0.9.3**, upstream license retained. Root Cargo.toml
pins that release and patches it to this directory. Registry sources are not
modified. The application's Linux backend is exclusively `use_zbus`.

## Changes

- Public `ActionCapabilities` (all disabled by default) and native setters for
  zbus MPRIS, Windows SMTC, and macOS MPRemoteCommandCenter. The app publishes
  play/pause/stop only with a selected player, always disables Next/Previous, and
  enables seek only when that selected player is observed seekable.
- zbus stores capabilities in its actual property state and emits property change
  notifications. Windows uses the native action-enabled setters. macOS enables
  the corresponding command objects and adds the previously absent Stop handler.
- zbus attach waits for actual bus/name registration and reports failure instead
  of spawning a thread that panics after reporting success. Publications are
  acknowledged after state/property notification, so the app's callback target
  gate is not released before the new metadata is applied. Internal requests use
  a bounded channel. These waits happen only on the app's Linux adapter worker.
- zbus duration overflow is omitted, not unwrapped. macOS rejects invalid absolute
  seek seconds rather than panicking. Windows detaches both button and absolute
  position subscriptions and clears their tokens.

Source evidence: compare upstream 0.9.3 `platform/mpris/zbus.rs` methods
`can_go_next`, `can_go_previous`, `can_seek` (formerly unconditional true);
Windows `attach` (formerly unconditional SetIsNextEnabled/Previous/FastForward/
Rewind true); macOS `attach_command_handlers` (formerly all enabled and no Stop).
The patch changes those actual native/property paths, not just application event
filtering. The unused libdbus backend is upstream, not qualified or selected by
Kinema; it does not implement this patch's capability API.

## Tests and limits

zbus unit tests cover default and dynamic property values, pre-attach capability
configuration, duration overflow, and real registration failure on a deliberately
nonexistent bus in an isolated child process (no process-global environment race).
The workspace adapter suite also tests selected-player seek/queue capabilities,
transport mapping, generation checks, bounded/revoked callbacks and host health.

`cargo check -p souvlaki --locked` checks the selected Linux backend. Dependency
unit tests are not run by `cargo test --workspace`; their standalone validation
uses an identical temporary source copy with unused upstream dev-only
manifest entries removed (upstream's winit dev version is not locally resolvable).
No native Windows/macOS compile/run qualification or real Linux shell/media-key
qualification follows from these tests. Windows absolute-seek visibility remains
shell-dependent; SMTC only exposes fast-forward/rewind enable flags. Artwork is
not published. Root native playback/HDR qualification is unchanged.
