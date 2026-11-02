//! Safe native visibility stays in the durable GPUI platform patch.
use gpui::*;
use std::rc::Rc;

pub(super) fn install(_: &Window, cx: &mut App) {
    super::set_main_restorer(
        Rc::new(|window| {
            let restored = window.show_window();
            if !restored {
                // Activation still restores minimized windows on hosts without hide.
                tracing::debug!(
                    category = "main_restore",
                    "Native per-window show unavailable"
                );
            }
            restored
        }),
        cx,
    );
    super::set_main_hider(Some(Rc::new(|window| window.hide_window())), cx);
}
