//! Controlled segmented selection, with wraparound arrow-key navigation.
//!
//! Accessibility: a `TabList` (named by [`Segmented::label`]) of `Tab`s with
//! `selected`, name, position and set size. The group has one tab stop; once
//! focused, Left/Up and Right/Down move the selection with wraparound and
//! Home/End jump to the first/last option (selection follows focus, so no
//! Enter/Space is needed). The focus ring follows the host radius. The
//! selected option is a raised pill with accent ink, not colour alone.
use crate::{
    ui::motion::{interaction::state_spring, transition::FEEDBACK},
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::theme::typography::TypeExt,
    ui::theme::{elevation, opacity, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};
use std::rc::Rc;

/// Return a target for recognized navigation keys; empty groups never select.
pub fn navigate(index: usize, len: usize, key: &str) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let index = index.min(len - 1);
    match key {
        "left" | "up" => Some(if index == 0 { len - 1 } else { index - 1 }),
        "right" | "down" => Some((index + 1) % len),
        "home" => Some(0),
        "end" => Some(len - 1),
        _ => None,
    }
}
pub type ChangeHandler<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;
#[derive(IntoElement)]
pub struct Segmented<T: Clone + PartialEq + 'static> {
    id: ElementId,
    options: Vec<(T, SharedString)>,
    selected: T,
    on_change: Option<ChangeHandler<T>>,
    icons: Vec<IconName>,
    counts: Vec<Option<usize>>,
    toolbar: bool,
    label: Option<SharedString>,
}
impl<T: Clone + PartialEq + 'static> Segmented<T> {
    pub fn new(
        id: impl Into<ElementId>,
        options: impl IntoIterator<Item = (T, SharedString)>,
        selected: T,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().collect(),
            selected,
            on_change: None,
            icons: Vec::new(),
            counts: Vec::new(),
            toolbar: false,
            label: None,
        }
    }
    /// Leading glyph per option, matched by position.
    pub fn icons(mut self, icons: impl IntoIterator<Item = IconName>) -> Self {
        self.icons = icons.into_iter().collect();
        self
    }
    /// Trailing muted count per option (e.g. results per filter), by position.
    pub fn counts(mut self, counts: impl IntoIterator<Item = Option<usize>>) -> Self {
        self.counts = counts.into_iter().collect();
        self
    }
    /// Floating-header geometry and glass material (see `Palette::glass`).
    pub fn toolbar(mut self) -> Self {
        self.toolbar = true;
        self
    }
    /// Accessible name of the group.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn on_change(mut self, handler: impl Fn(&T, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}
impl<T: Clone + PartialEq + 'static> RenderOnce for Segmented<T> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let selected = self
            .options
            .iter()
            .position(|(value, _)| value == &self.selected)
            .unwrap_or(0);
        let options = self.options.clone();
        let keyboard_handler = self.on_change.clone();
        let icons = self.icons;
        let counts = self.counts;
        let muted = p.text_muted;
        let in_toolbar = self.toolbar;
        let group_label = self.label.clone();
        let count = self.options.len();
        let (track, track_edge, height, outer, inner) = if in_toolbar {
            let (fill, _, edge) = p.glass();
            (fill, edge, space::TOOLBAR_CONTROL, radius::LG, radius::MD)
        } else {
            (
                p.ctl_rest,
                p.line_subtle,
                space::CONTROL_HEIGHT,
                radius::MD,
                radius::SM,
            )
        };
        let content = move |index: usize, label: SharedString, ink: Hsla| {
            div()
                .flex()
                .items_center()
                .gap(space::S6)
                .children(
                    icons
                        .get(index)
                        .map(|icon| Icon::new(*icon).size(space::S12 + space::S2).color(ink)),
                )
                .child(label)
                .children(counts.get(index).copied().flatten().map(|count| {
                    // Counts read as secondary: muted at rest, softened accent when active.
                    let ink = if ink == p.accent_text {
                        ink.opacity(opacity::INK_SOFT)
                    } else {
                        muted
                    };
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(ink)
                        .child(count.to_string())
                }))
        };
        div()
            .id(self.id)
            .flex()
            .border(space::FOCUS_RING)
            .border_color(transparent_black())
            .p(space::S2)
            .m(-space::S4)
            .rounded(outer + space::S4)
            .focusable()
            .tab_stop(true)
            .role(Role::TabList)
            .when_some(group_label, |el, label| el.aria_label(label))
            .focus_visible(move |s| s.border_color(p.focus_ring))
            .on_key_down(move |event, window, cx| {
                if let Some(index) = navigate(selected, options.len(), &event.keystroke.key) {
                    cx.stop_propagation();
                    if let Some(handler) = &keyboard_handler {
                        handler(&options[index].0, window, cx);
                    }
                }
            })
            .child(
                div()
                    .flex()
                    .h(height)
                    .p(space::S2)
                    .rounded(outer)
                    .bg(track)
                    .border(space::HAIRLINE)
                    .border_color(track_edge)
                    .children(self.options.into_iter().enumerate().map(
                        |(index, (value, label))| {
                            let active = value == self.selected;
                            let handler = self.on_change.clone();
                            let inactive_ink = p.text_secondary;
                            let active_ink = p.accent_text;
                            let pill = p.bg_raised;
                            let face = div()
                                .absolute()
                                .size_full()
                                .px(space::S12)
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(inner)
                                .type_style(typeface::SMALL_MEDIUM)
                                .when(active, |el| el.shadow(elevation::sm(&p)))
                                .child(content(
                                    index,
                                    label.clone(),
                                    if active {
                                        p.accent_text
                                    } else {
                                        p.text_secondary
                                    },
                                ));
                            // Selected state is a raised pill that cross-fades, not an underline.
                            let face = state_spring(
                                face,
                                "segment-pill",
                                active,
                                FEEDBACK,
                                cx,
                                move |el, t| {
                                    el.bg(pill.opacity(t))
                                        .text_color(inactive_ink.blend(active_ink.opacity(t)))
                                },
                            );
                            div()
                                .id(("segment", index))
                                .relative()
                                .h_full()
                                .min_w(space::S32)
                                // Width comes from an invisible copy of the label plus padding.
                                .px(space::S12)
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(inner)
                                .type_style(typeface::SMALL_MEDIUM)
                                .role(Role::Tab)
                                .aria_label(label.clone())
                                .aria_selected(active)
                                .aria_position_in_set(index + 1)
                                .aria_size_of_set(count)
                                .cursor_pointer()
                                .when(!active, |el| el.hover(move |s| s.bg(p.ctl_hover)))
                                .child(div().invisible().child(content(
                                    index,
                                    label,
                                    p.text_secondary,
                                )))
                                .child(face)
                                .on_click(move |_, window, cx| {
                                    if let Some(handler) = &handler {
                                        handler(&value, window, cx);
                                    }
                                })
                        },
                    )),
            )
    }
}
#[cfg(test)]
mod tests {
    use super::navigate;
    #[test]
    fn navigation_wraps_and_handles_empty_groups() {
        assert_eq!(navigate(0, 0, "right"), None);
        assert_eq!(navigate(0, 2, "left"), Some(1));
        assert_eq!(navigate(1, 2, "right"), Some(0));
        assert_eq!(navigate(1, 2, "home"), Some(0));
        assert_eq!(navigate(0, 2, "end"), Some(1));
        assert_eq!(navigate(0, 1, "left"), Some(0));
        assert_eq!(navigate(99, 2, "right"), Some(0));
        assert_eq!(navigate(0, 2, "enter"), None);
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used, reason = "test setup must succeed")]
mod interaction_tests {
    use super::Segmented;
    use gpui::{AppContext, Context, IntoElement, Render, TestAppContext, Window};
    use std::{cell::RefCell, rc::Rc};

    struct Fixture {
        selected: u8,
        seen: Rc<RefCell<Vec<u8>>>,
    }
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let this = cx.entity().downgrade();
            let seen = self.seen.clone();
            Segmented::new(
                "seg",
                [(0u8, "A".into()), (1, "B".into()), (2, "C".into())],
                self.selected,
            )
            .label("Letters")
            .on_change(move |value, _, cx| {
                seen.borrow_mut().push(*value);
                let value = *value;
                let _ = this.update(cx, |fixture, cx| {
                    fixture.selected = value;
                    cx.notify();
                });
            })
        }
    }

    #[gpui::test]
    fn arrows_home_end_move_selection(cx: &mut TestAppContext) {
        cx.update(crate::ui::theme::init);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let handle = cx.add_window({
            let seen = seen.clone();
            move |_, _| Fixture { selected: 0, seen }
        });
        cx.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.focus_next(cx);
        })
        .unwrap();
        for key in ["right", "right", "right", "left", "end", "home"] {
            cx.simulate_keystrokes(handle.into(), key);
            cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
        }
        assert_eq!(*seen.borrow(), vec![1, 2, 0, 2, 2, 0]);
    }
}
