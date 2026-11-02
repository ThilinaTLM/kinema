//! Typed compact select built on [`Menu`].
//!
//! Accessibility: the trigger is a combo-box button exposing its name, value
//! and expanded state; arrows/Home/End/type-ahead move within the menu, Enter
//! chooses and Esc closes and returns focus to the trigger.
use super::*;
use crate::ui::theme::typography::{TextSize, TypeExt, Weight};

/// A typed compact select. Subscribe to `SelectEvent::Changed` for updates.
pub enum SelectEvent<T> {
    Changed(T),
}
pub struct Select<T: Clone + PartialEq + 'static> {
    options: Vec<(T, SharedString)>,
    value: T,
    focus: FocusHandle,
    menu: Option<Entity<Menu>>,
    subscription: Option<Subscription>,
    icon: Option<IconName>,
    prefix: Option<SharedString>,
    toolbar: bool,
    label: Option<SharedString>,
}
impl<T: Clone + PartialEq + 'static> EventEmitter<SelectEvent<T>> for Select<T> {}
impl<T: Clone + PartialEq + 'static> Select<T> {
    pub fn new(value: T, options: Vec<(T, SharedString)>, cx: &mut Context<Self>) -> Self {
        Self {
            value,
            options,
            focus: cx.focus_handle().tab_stop(true),
            menu: None,
            subscription: None,
            icon: None,
            prefix: None,
            toolbar: false,
            label: None,
        }
    }
    /// Leading glyph, e.g. a sort icon.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
    /// Accessible name of the select (falls back to the prefix, then "Select").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
    fn name(&self) -> SharedString {
        self.label
            .clone()
            .or_else(|| self.prefix.clone())
            .unwrap_or_else(|| "Select".into())
    }
    /// Muted key shown before the value ("Sort" Seeders).
    pub fn prefix(mut self, prefix: impl Into<SharedString>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
    /// Floating-header geometry and glass material, matching `Segmented::toolbar`.
    pub fn toolbar_style(mut self) -> Self {
        self.toolbar = true;
        self
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn set_value(&mut self, value: T, cx: &mut Context<Self>) {
        self.value = value;
        cx.notify();
    }
    fn open(&mut self, pointer: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_some() {
            return;
        }
        self.focus.focus(window, cx);
        let weak = cx.entity().downgrade();
        let items = self
            .options
            .iter()
            .map(|(value, label)| {
                let value = value.clone();
                let weak = weak.clone();
                MenuItem::Action {
                    label: label.clone(),
                    shortcut: None,
                    icon: None,
                    disabled_reason: None,
                    checked: value == self.value,
                    handler: Rc::new(move |_, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if this.value != value {
                                this.value = value.clone();
                                cx.emit(SelectEvent::Changed(value.clone()));
                                cx.notify();
                            }
                        });
                    }),
                }
            })
            .collect();
        let name = self.name();
        let menu = cx.new(|cx| {
            Menu::new(items, window, cx)
                .label(name)
                .opened_by_pointer(pointer)
        });
        self.subscription = Some(cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.menu = None;
            cx.notify();
        }));
        self.menu = Some(menu);
        cx.notify();
    }
}
impl<T: Clone + PartialEq + 'static> Focusable for Select<T> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl<T: Clone + PartialEq + 'static> Render for Select<T> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let label = self
            .options
            .iter()
            .find(|(v, _)| *v == self.value)
            .map(|(_, l)| l.clone())
            .unwrap_or_default();
        div()
            .id("select")
            .role(Role::ComboBox)
            .aria_label(self.name())
            .aria_value(label.clone())
            .aria_expanded(self.menu.is_some())
            .track_focus(&self.focus)
            .focus_visible(move |s| s.border(size::FOCUS_RING).border_color(p.ring))
            .tab_stop(true)
            .relative()
            .rounded(if self.toolbar { radius::LG } else { radius::MD })
            .child({
                let face = div()
                    .flex()
                    .items_center()
                    .gap(space::S6)
                    .text_color(p.foreground)
                    .children(
                        self.icon
                            .map(|icon| Icon::new(icon).color(p.muted_foreground)),
                    )
                    .children(
                        self.prefix
                            .clone()
                            .map(|prefix| div().text_color(p.subtle_foreground).child(prefix)),
                    )
                    .child(label)
                    .child(Icon::new(IconName::ChevronDown).color(p.muted_foreground));
                if self.toolbar {
                    let (fill, hover, edge) = p.glass();
                    face.h(size::TOOLBAR_CONTROL)
                        .rounded(radius::LG)
                        .px(space::S12)
                        .bg(fill)
                        .border(size::HAIRLINE)
                        .border_color(edge)
                        .text(TextSize::Sm)
                        .weight(Weight::Medium)
                        .hover(move |s| s.bg(hover))
                } else {
                    face.h(size::CONTROL_SM)
                        .rounded(radius::MD)
                        .px(space::S8)
                        .bg(p.secondary)
                        .border_1()
                        .border_color(p.input)
                        .text(TextSize::Base)
                }
            })
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                this.open(!event.is_keyboard(), window, cx)
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "space" | "enter" | "down") {
                    this.open(false, window, cx);
                    cx.stop_propagation();
                }
            }))
            .when_some(self.menu.clone(), |el, menu| {
                el.child(
                    div().absolute().top_full().left_0().child(
                        deferred(anchored().snap_to_window_with_margin(space::S8).child(menu))
                            .priority(10),
                    ),
                )
            })
    }
}
