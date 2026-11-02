//! Plain action menu entry used by context menus and overflow menus.
use std::rc::Rc;

use gpui::{App, Window};

use super::MenuItem;
use crate::ui::primitives::icon::IconName;

pub fn menu_item(
    label: &'static str,
    handler: impl Fn(&mut Window, &mut App) + 'static,
) -> MenuItem {
    MenuItem::Action {
        label: label.into(),
        shortcut: None,
        icon: None,
        disabled_reason: None,
        checked: false,
        handler: Rc::new(handler),
    }
}

/// Action entry with a leading icon.
pub fn menu_item_icon(
    label: &'static str,
    icon: IconName,
    handler: impl Fn(&mut Window, &mut App) + 'static,
) -> MenuItem {
    MenuItem::Action {
        label: label.into(),
        shortcut: None,
        icon: Some(icon),
        disabled_reason: None,
        checked: false,
        handler: Rc::new(handler),
    }
}
