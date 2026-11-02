//! App compositions built from primitives: media cards, stream rows, toggles
//! and state views.
//!
//! Blocks may use `kinema-core` domain types and `crate::artwork`, but never
//! `crate::controllers` or view types.

pub mod ambient_background;
pub mod filter_panel;
pub mod library_toggle;
pub mod media_card;
mod media_hero;
pub mod overlay;
pub mod poster_card;
pub mod quality_tile;
pub mod skeleton_card;
pub mod stat_tile;
pub mod state_toggle;
mod state_view;
mod stream_row;
pub mod thumb;
pub mod toolbar;
pub mod watch_mark;
pub mod watched_toggle;

pub use media_hero::{MediaHero, MetaItem, POSTER_ASPECT, uses_over_art};
pub use state_toggle::{StateToggle, ToggleKind, ToggleStatus};
#[cfg_attr(not(feature = "ui-probe"), allow(unused_imports))]
pub(crate) use state_view::ResourceView;
pub use state_view::{StateView, ViewSelection, ViewState, select_view};
pub use stream_row::{HEALTHY_SEEDERS, StreamRow};
