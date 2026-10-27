//! Desktop v3 in-house component set built directly on GPUI (`gpui-pre`).
//!
//! Spec: `docs/design/desktop-v3.md`. Warm amber accent, subtle roundness
//! (`tokens::radius` XS/SM/MD/LG), soft shadows only on raised surfaces
//! (`tokens::elevation`), hairlines for structure, smooth motion from
//! [`motion`] (reduced motion honored), cinematic artwork backdrops, compact
//! desktop density. Tokens come from [`tokens`]; reusable compositions live in
//! [`patterns`].
//! Code here must use `gpui::*` directly, never `gpui_kit`.

pub mod ambient_background;
pub mod motion;
pub mod patterns;
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
pub mod action_button;
pub mod button;
pub mod chip;
pub mod choice_card;
pub mod filter_panel;
pub mod icon_button;
pub mod library_toggle;
pub mod media_card;
pub mod notice;
pub mod overlay;
pub mod progress;
pub mod scroll_area;
pub mod segmented;
pub mod sheet;
pub mod state_toggle;
pub mod switch;
pub mod thumb;
pub mod toolbar;
pub mod watch_mark;
pub mod watched_toggle;

pub use icon::{Icon, IconName};
pub use motion::Motion;
pub use state_toggle::{StateToggle, ToggleKind, ToggleStatus};
pub use text::TypeExt;
pub use tokens::{Mode, Palette, Theme, palette, theme};

pub mod poster_card;
pub mod skeleton_card;
