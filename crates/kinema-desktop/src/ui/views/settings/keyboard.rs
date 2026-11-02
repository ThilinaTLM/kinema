//! Keyboard & mouse section.
use super::*;

impl SettingsWindow {
    pub(super) fn keyboard_mouse(&self, mut body: Div, p: theme::Palette) -> Div {
        body = body.child(keyboard_mouse_reference(&p));
        body
    }
}

pub(super) fn keyboard_mouse_reference(p: &theme::Palette) -> AnyElement {
    let mut root = div().flex().flex_col().gap(s::S24).child(text(
        "Reference for the embedded Kinema player only. These are not global shortcuts and do not apply to external mpv or VLC. Rebinding is not available yet.",
        p.muted_foreground,
    ));
    for group in crate::player_shortcuts::reference_groups() {
        let mut list = div()
            .flex()
            .flex_col()
            .border_t(size::HAIRLINE)
            .border_color(p.border);
        for entry in group.entries {
            list = list.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .justify_between()
                    .gap_x(s::S16)
                    .gap_y(s::S4)
                    .py(s::S8)
                    .border_b(size::HAIRLINE)
                    .border_color(p.border)
                    .child(div().flex_1().min_w(s::S48 * 3.).child(entry.label))
                    .child(
                        div()
                            .text(TextSize::Sm)
                            .weight(Weight::Medium)
                            .text_color(p.muted_foreground)
                            .child(entry.keys),
                    ),
            );
        }
        root = root.child(
            div()
                .flex()
                .flex_col()
                .gap(s::S8)
                .child(
                    div()
                        .text(TextSize::Lg)
                        .weight(Weight::Semibold)
                        .child(group.title),
                )
                .child(list),
        );
    }
    root.child(
        div()
            .flex()
            .flex_col()
            .gap(s::S4)
            .text(TextSize::Sm)
            .text_color(p.subtle_foreground)
            .child(crate::player_shortcuts::FOCUS_NOTE)
            .child(crate::player_shortcuts::MEDIA_NOTE),
    )
    .into_any_element()
}
