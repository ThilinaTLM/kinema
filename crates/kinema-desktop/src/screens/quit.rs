//! Quit confirmation shared by the action and native window close path.
use super::coordinator::AppCoordinator;
use crate::widgets::{button::Button, sheet::Sheet};
use gpui::*;
pub(crate) fn confirmation(
    coordinator: Entity<AppCoordinator>,
    cancel_focus: FocusHandle,
) -> Sheet {
    let cancel = coordinator.clone();
    let dismiss = coordinator;
    Sheet::new("quit-sheet","Quit Kinema?")
        .body("Sessions are active or launches are pending. Quitting stops downloads and the local server; your player may lose media.")
        .initial_focus(cancel_focus.clone())
        .on_dismiss(move|_,cx|dismiss.update(cx,|c,cx|{c.quit_requested=false;cx.notify();}))
        .footer(Button::new("cancel-quit","Cancel").focus_handle(cancel_focus).on_click(move|_,_,cx|cancel.update(cx,|c,cx|{c.quit_requested=false;cx.notify();})))
        .footer(Button::new("confirm-quit","Quit").on_click(|_,_,cx|cx.quit()))
}
