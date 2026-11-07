//! Notification panel: the session's toast history behind the header bell.
//!
//! Opening the panel marks every entry read; entries that were unread at that
//! moment carry a "New" marker while it stays open. Entries keep no actions:
//! a Retry or Undo belongs to the live toast, whose owner may have moved on.
use std::time::{Duration, Instant};

use gpui::{prelude::*, *};

use crate::{
    ui::primitives::button::{Button, ButtonVariant},
    ui::primitives::toast::{LogEntry, ToastHost, tone_mark_for},
    ui::primitives::{Icon, IconName},
    ui::theme::layout::controls::NOTIFICATION_LIST_MAX_HEIGHT,
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{self, radius, size, space as s},
};

/// Short relative age: "Just now", "5 min ago", "2 h ago", "3 d ago".
pub(crate) fn age(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match secs {
        0..60 => "Just now".to_owned(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

fn row(entry: &LogEntry, new: bool, now: Instant, cx: &App) -> AnyElement {
    let p = theme::palette(cx);
    let when = age(now.saturating_duration_since(entry.at));
    let name: SharedString = format!(
        "{}{}{}, {when}",
        if new { "New: " } else { "" },
        entry.title,
        entry
            .body
            .as_ref()
            .map(|b| format!(". {b}"))
            .unwrap_or_default(),
    )
    .into();
    div()
        .id(("notification", entry.id))
        .role(Role::ListItem)
        .aria_label(name)
        .w_full()
        .flex()
        .items_start()
        .gap(s::S12)
        .px(s::S8)
        .py(s::S8)
        .rounded(radius::MD)
        .when(new, |el| el.bg(p.accent))
        .child(tone_mark_for(entry.tone, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(s::S2)
                .pt(s::S4)
                .child(
                    div()
                        .text(TextSize::Base)
                        .weight(Weight::Semibold)
                        .text_color(p.foreground)
                        .child(entry.title.clone()),
                )
                .when_some(entry.body.clone(), |el, body| {
                    el.child(
                        div()
                            .text(TextSize::Sm)
                            .text_color(p.muted_foreground)
                            .child(body),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(s::S6)
                        .text(TextSize::Xs)
                        .text_color(p.subtle_foreground)
                        .when(new, |el| {
                            el.child(
                                div()
                                    .text_color(p.primary_text)
                                    .weight(Weight::Semibold)
                                    .child("New"),
                            )
                            .child("·")
                        })
                        .child(when),
                ),
        )
        .into_any_element()
}

/// Panel content for the header Popover.
pub(crate) fn panel(host: Entity<ToastHost>, cx: &mut App) -> AnyElement {
    let p = *theme::palette(cx);
    let now = Instant::now();
    let (rows, empty) = {
        let host = host.read(cx);
        let rows: Vec<AnyElement> = host
            .log()
            .map(|entry| row(entry, host.is_new(entry), now, cx))
            .collect();
        let empty = rows.is_empty();
        (rows, empty)
    };
    let clear = host.downgrade();
    div()
        .w(size::POPOVER_WIDTH)
        .flex_none()
        .p(s::S4)
        .flex()
        .flex_col()
        .gap(s::S8)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text(TextSize::Lg)
                        .weight(Weight::Semibold)
                        .child("Notifications"),
                )
                .when(!empty, |el| {
                    el.child(
                        Button::new("notifications-clear", "Clear all")
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, _, cx| {
                                let _ = clear.update(cx, |h, cx| h.clear_log(cx));
                            }),
                    )
                }),
        )
        .when(empty, |el| {
            el.child(
                div()
                    .py(s::S24)
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(s::S8)
                    .text_color(p.muted_foreground)
                    .child(Icon::new(IconName::Bell).color(p.subtle_foreground))
                    .child(div().text(TextSize::Base).child("No notifications"))
                    .child(
                        div()
                            .text(TextSize::Sm)
                            .text_color(p.subtle_foreground)
                            .child("Messages stay here after they close, until you quit."),
                    ),
            )
        })
        .when(!empty, |el| {
            el.child(
                div()
                    .id("notifications-list")
                    .role(Role::List)
                    .aria_label(SharedString::from("Notifications"))
                    .max_h(NOTIFICATION_LIST_MAX_HEIGHT)
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(s::S2)
                    .children(rows),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::age;
    use std::time::Duration;

    #[test]
    fn ages_are_short_and_rounded_down() {
        assert_eq!(age(Duration::from_secs(59)), "Just now");
        assert_eq!(age(Duration::from_secs(60)), "1 min ago");
        assert_eq!(age(Duration::from_secs(3599)), "59 min ago");
        assert_eq!(age(Duration::from_secs(7200)), "2 h ago");
        assert_eq!(age(Duration::from_secs(3 * 86_400)), "3 d ago");
    }
}
