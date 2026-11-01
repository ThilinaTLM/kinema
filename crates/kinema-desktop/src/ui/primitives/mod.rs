//! Generic, accessible UI primitives built directly on GPUI (`gpui-pre`).
//!
//! Primitives depend only on [`crate::ui::theme`] and [`crate::ui::motion`];
//! they carry no screen, controller or media-domain types. Callers pass slots
//! (`AnyElement`), plain values and callbacks.

pub mod action_button;
pub mod assets;
pub mod button;
pub mod chip;
pub mod choice_card;
pub mod icon;
pub mod icon_button;
pub mod menu;
pub mod notice;
pub mod popover;
pub mod progress;
pub mod scroll_area;
pub mod search_field;
pub mod segmented;
pub mod sheet;
pub mod switch;
pub mod tag_list;
pub mod text_input;
pub mod tooltip;

mod badge;
mod card;
mod carousel;
mod list_row;
mod menu_item;
mod section;
mod skeleton;

pub use badge::{Badge, BadgeTone};
pub use card::Card;
pub use carousel::Carousel;
pub use icon::{Icon, IconName};
pub use list_row::ListRow;
pub use menu_item::{menu_item, menu_item_icon};
pub use section::Section;
pub use skeleton::{Skeleton, SkeletonShape, SkeletonText};
