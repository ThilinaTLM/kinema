//! Keyboard shortcut help overlay.
use super::*;

impl PlayerWindow {
    pub(super) fn open_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.help_focus.is_some() {
            return;
        }
        self.help_previous = window.focused(cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.help_focus = Some(focus);
        self.activity(cx);
    }
    pub(super) fn close_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.help_focus = None;
        self.help_previous
            .take()
            .unwrap_or_else(|| self.surface_focus.clone())
            .focus(window, cx);
        self.activity(cx);
    }
    pub(super) fn shortcut_help(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = Palette::cinema();
        let groups = shortcuts::reference_groups();
        let width = window.viewport_size().width;
        let columns = if width >= HELP_COLUMN * 3. + s::S24 * 2. + s::S48 * 2. {
            3
        } else if width >= HELP_COLUMN * 2. + s::S24 + s::S48 * 2. {
            2
        } else {
            1
        };
        // Title row + one row per entry, so column heights balance visually.
        let sizes: Vec<usize> = groups.iter().map(|g| g.entries.len() + 2).collect();
        let keycap = |text: &'static str| {
            div()
                .flex_none()
                .px(s::S6)
                .h(size::CHIP)
                .flex()
                .items_center()
                .rounded(radius::MD)
                .bg(p.card)
                .border_1()
                .border_color(p.input)
                .text(TextSize::Sm)
                .weight(Weight::Medium)
                .text_color(p.foreground)
                .whitespace_nowrap()
                .child(text)
        };
        let group_view = |group: &'static shortcuts::ReferenceGroup| {
            div()
                .flex()
                .flex_col()
                .gap(s::S6)
                .child(
                    div()
                        .pb(s::S2)
                        .text(TextSize::Xs)
                        .weight(Weight::Semibold)
                        .text_color(p.subtle_foreground)
                        .child(group.title.to_uppercase()),
                )
                .children(group.entries.iter().map(move |entry| {
                    let mouse = entry.bindings.is_empty();
                    div()
                        .flex()
                        .items_center()
                        .gap(s::S12)
                        .min_h(player::HELP_ROW_MIN_HEIGHT)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text(TextSize::Sm)
                                .text_color(p.muted_foreground)
                                .child(entry.label),
                        )
                        .child(if mouse {
                            div()
                                .flex_none()
                                .max_w(player::HELP_MOUSE_LABEL_MAX)
                                .text_right()
                                .text(TextSize::Sm)
                                .weight(Weight::Medium)
                                .text_color(p.foreground)
                                .child(entry.keys)
                        } else {
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(s::S4)
                                .children(entry.keys.split(" / ").map(keycap))
                        })
                }))
        };
        div()
            .id("player-shortcut-overlay")
            .absolute()
            .inset_0()
            .bg(p.overlay)
            .flex()
            .items_center()
            .justify_center()
            .p(s::S24)
            .track_focus(self.help_focus.as_ref().unwrap_or(&self.surface_focus))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("player-shortcut-card")
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(
                        HELP_COLUMN * columns as f32 + s::S32 * (columns as f32 - 1.) + s::S24 * 2.,
                    )
                    .max_h_full()
                    .bg(p.popover)
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::XL)
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(s::S12)
                            .pl(s::S24)
                            .pr(s::S12)
                            .py(s::S12)
                            .border_b_1()
                            .border_color(p.border)
                            .child(
                                Icon::new(IconName::Keyboard)
                                    .size(player::HELP_HEADER_ICON)
                                    .color(p.muted_foreground),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text(TextSize::Lg)
                                    .weight(Weight::Semibold)
                                    .child("Keyboard & mouse shortcuts"),
                            )
                            .child({
                                Button::new("player-shortcut-dismiss", "")
                                    .variant(ButtonVariant::Ghost)
                                    .icon(IconName::Close)
                                    .square(true)
                                    .tooltip("Close (Escape)")
                                    .focus_handle(self.help_dismiss_focus.clone())
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.close_help(window, cx)
                                    }))
                            }),
                    )
                    .child(
                        div()
                            .id("player-shortcut-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(s::S24)
                            .py(s::S16)
                            .child(div().flex().items_start().gap(s::S32).children(
                                controls::balance_columns(&sizes, columns).into_iter().map(
                                    |range| {
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .gap(s::S20)
                                            .children(groups[range].iter().map(group_view))
                                    },
                                ),
                            ))
                            // Notes scroll with the list so short windows keep it readable.
                            .child(
                                div()
                                    .mt(s::S20)
                                    .pt(s::S12)
                                    .flex()
                                    .flex_col()
                                    .gap(s::S4)
                                    .border_t_1()
                                    .border_color(p.border)
                                    .text(TextSize::Sm)
                                    .text_color(p.subtle_foreground)
                                    .child(shortcuts::FOCUS_NOTE)
                                    .child(shortcuts::MEDIA_NOTE),
                            ),
                    ),
            )
            .into_any_element()
    }
}
