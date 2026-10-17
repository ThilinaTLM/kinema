//! Desktop v3 in-house component set built directly on GPUI (`gpui-pre`).
//!
//! Spec: `docs/design/desktop-v3.md`. Square geometry (no radius), hairlines,
//! no drop shadows, tokens from [`tokens`], motion from [`motion`].
//! Code here must use `gpui::*` directly, never `gpui_kit`.

pub mod motion;
pub mod text;
pub mod tokens;

// Foundation: assets (fonts, icons), icons.
pub mod assets;
pub mod icon;

// Text entry.
pub mod search_field;
pub mod text_input;

// Menus and overlays.
pub mod menu;
pub mod popover;
pub mod tooltip;

// Controls and surfaces.
pub mod button;
pub mod chip;
pub mod icon_button;
pub mod notice;
pub mod progress;
pub mod scroll_area;
pub mod segmented;
pub mod sheet;
pub mod switch;

pub use icon::{Icon, IconName};
pub use motion::Motion;
pub use text::TypeExt;
pub use tokens::{Mode, Palette, Theme, palette, theme};
