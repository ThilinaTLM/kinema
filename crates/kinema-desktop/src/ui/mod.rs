//! Desktop v3 UI: in-house components built directly on GPUI (`gpui-pre`).
//!
//! Spec: `docs/design/desktop-v3.md`. Code here must use `gpui::*` directly,
//! never `gpui_kit`.
//!
//! Layers, from lowest to highest; a layer may depend only on layers listed
//! before it:
//!
//! - [`theme`]: pure token values and functions (palette, type, space, radius,
//!   elevation). No GPUI elements.
//! - [`motion`]: durations, reduced-motion handling, tweens and interaction
//!   animation helpers. Depends on `theme` only.
//! - [`primitives`]: generic, accessible components. Depend on `theme` and
//!   `motion`; no media or domain vocabulary; no controllers.
//! - [`blocks`]: media/app compositions of primitives. May use `kinema-core`
//!   domain types and `crate::artwork`; never `crate::controllers` or
//!   [`views`].
//! - [`views`]: pages and windows. Compose blocks and primitives, own view
//!   state and talk to `crate::controllers` through entities and events.

pub mod blocks;
pub mod motion;
pub mod primitives;
pub mod theme;
#[cfg(not(target_family = "wasm"))]
pub mod views;
