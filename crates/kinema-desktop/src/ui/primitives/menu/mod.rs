//! Rounded native menus with context-menu and select adapters.
//!
//! `Menu` (this file) is the popup list; `context` attaches it to a right-click
//! or keyboard trigger; `select` is the typed compact select; `item` has the
//! plain-action constructors.
//!
//! Accessibility: the panel is `Role::Menu` named by [`Menu::label`]; rows are
//! `MenuItem` (or `MenuItemCheckBox` with a toggled state when checked), and
//! the highlighted row is reported as the active descendant because the menu
//! itself keeps keyboard focus. Submenu rows expose `expanded`; disabled rows
//! are inert and their reason is the description. Up/Down/Home/End move,
//! Enter/Space activate, Right/Left open/close a submenu, typing jumps, and
//! Esc (or an outside press) closes the menu and returns focus to the element
//! that held it before opening. The highlight is a fill; checked state also
//! shows a check glyph, so selection is not colour-only.
//!
//! A window-keyed registry owns only a weak reference: opening a context menu
//! dismisses the previous one without keeping an unmounted trigger alive.
use crate::ui::theme::layout::controls;
use crate::{
    ui::motion::{
        Closed, Presence,
        transition::{POPUP_IN, POPUP_OUT},
    },
    ui::primitives::icon::{Icon, IconName},
    ui::primitives::tooltip::tooltip,
    ui::theme::typography::{TextSize, TypeExt},
    ui::theme::{elevation, palette, radius, size, space},
};
use gpui::{prelude::*, *};
use std::{rc::Rc, time::Instant};

pub type MenuHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Action callbacks use the same application commands as toolbar buttons.
#[derive(Clone)]
pub enum MenuItem {
    Action {
        label: SharedString,
        shortcut: Option<SharedString>,
        icon: Option<IconName>,
        disabled_reason: Option<SharedString>,
        checked: bool,
        handler: MenuHandler,
    },
    Separator,
    Submenu {
        label: SharedString,
        items: Vec<MenuItem>,
    },
}
impl MenuItem {
    fn label(&self) -> Option<&str> {
        match self {
            Self::Action { label, .. } | Self::Submenu { label, .. } => Some(label),
            Self::Separator => None,
        }
    }
    fn disabled_text(&self) -> Option<SharedString> {
        match self {
            Self::Action {
                disabled_reason, ..
            } => disabled_reason.clone(),
            _ => None,
        }
    }
    fn enabled(&self) -> bool {
        matches!(
            self,
            Self::Action {
                disabled_reason: None,
                ..
            } | Self::Submenu { .. }
        )
    }
}

/// Pure keyboard policy. Separators and unavailable commands are never selected.
#[derive(Default, Debug)]
pub struct MenuNavigation {
    pub selected: Option<usize>,
    prefix: String,
}
impl MenuNavigation {
    pub fn step(&mut self, items: &[MenuItem], forward: bool) {
        if items.is_empty() {
            self.selected = None;
            return;
        }
        let len = items.len();
        let start = self.selected.unwrap_or(if forward { len - 1 } else { 0 });
        self.selected = (1..=len)
            .map(|n| {
                if forward {
                    (start + n) % len
                } else {
                    (start + len - n) % len
                }
            })
            .find(|&i| items[i].enabled());
        self.prefix.clear();
    }
    /// Pointer hover owns the single highlight and the keyboard index, so
    /// arrows continue from the hovered row. Returns whether anything changed.
    pub fn hover(&mut self, items: &[MenuItem], index: usize) -> bool {
        if !items.get(index).is_some_and(MenuItem::enabled) || self.selected == Some(index) {
            return false;
        }
        self.selected = Some(index);
        self.prefix.clear();
        true
    }
    pub fn edge(&mut self, items: &[MenuItem], last: bool) {
        self.selected = if last {
            items.iter().rposition(MenuItem::enabled)
        } else {
            items.iter().position(MenuItem::enabled)
        };
        self.prefix.clear();
    }
    pub fn type_ahead(&mut self, items: &[MenuItem], text: &str, reset: bool) {
        if reset {
            self.prefix.clear();
        }
        self.prefix.push_str(&text.to_lowercase());
        let find = |prefix: &str| {
            let len = items.len();
            (1..=len)
                .map(|n| (self.selected.unwrap_or(len.saturating_sub(1)) + n) % len)
                .find(|&i| {
                    items[i].enabled()
                        && items[i]
                            .label()
                            .is_some_and(|s| s.to_lowercase().starts_with(prefix))
                })
        };
        if let Some(i) = find(&self.prefix) {
            self.selected = Some(i);
        } else {
            self.prefix = text.to_lowercase();
            self.selected = find(&self.prefix);
        }
    }
}

/// Focus-owning menu. Subscribe to `DismissEvent` to remove the overlay.
pub struct Menu {
    items: Vec<MenuItem>,
    nav: MenuNavigation,
    submenu: Option<usize>,
    parent_selection: Option<usize>,
    focus: FocusHandle,
    previous: Option<FocusHandle>,
    label: SharedString,
    last_type: Option<Instant>,
    presence: Entity<Presence>,
    scroll: ScrollHandle,
    child_scroll: ScrollHandle,
    _window_subscriptions: Vec<Subscription>,
}
impl EventEmitter<DismissEvent> for Menu {}
impl Focusable for Menu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Menu {
    /// Accessible name of the menu (default "Menu").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    /// Opened by pointer: nothing is highlighted until the pointer or an arrow
    /// key picks a row. Keyboard opening (the default) highlights the first
    /// enabled (or checked) row.
    pub fn opened_by_pointer(mut self, pointer: bool) -> Self {
        if pointer {
            self.nav.selected = None;
        }
        self
    }
    /// Capture focus and open a menu. Nested submenu entries beyond one level
    /// are omitted; callers should keep the shared model at one submenu level.
    pub fn new(items: Vec<MenuItem>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let items = items
            .into_iter()
            .map(|item| match item {
                MenuItem::Submenu { label, mut items } => {
                    items.retain(|item| !matches!(item, MenuItem::Submenu { .. }));
                    MenuItem::Submenu { label, items }
                }
                item => item,
            })
            .collect::<Vec<_>>();
        let focus = cx.focus_handle();
        let previous = window.focused(cx);
        focus.focus(window, cx);
        let mut nav = MenuNavigation::default();
        nav.edge(&items, false);
        if let Some(index) = items.iter().position(|item| {
            matches!(
                item,
                MenuItem::Action {
                    checked: true,
                    disabled_reason: None,
                    ..
                }
            )
        }) {
            nav.selected = Some(index);
        }
        let presence = cx.new(|_| Presence::opened(POPUP_IN, POPUP_OUT));
        let initial_size = window.viewport_size();
        let mut was_active = window.is_window_active();
        Self {
            items,
            nav,
            submenu: None,
            parent_selection: None,
            focus,
            previous,
            label: "Menu".into(),
            last_type: None,
            presence: presence.clone(),
            scroll: ScrollHandle::new(),
            child_scroll: ScrollHandle::new(),
            _window_subscriptions: vec![
                cx.observe(&presence, |_, _, cx| cx.notify()),
                cx.subscribe(&presence, |_, _, _: &Closed, cx| cx.emit(DismissEvent)),
                cx.observe_window_bounds(window, move |this, window, cx| {
                    if window.viewport_size() != initial_size {
                        this.dismiss(window, cx);
                    }
                }),
                cx.observe_window_activation(window, move |this, window, cx| {
                    let active = window.is_window_active();
                    if was_active && !active {
                        this.dismiss(window, cx);
                    }
                    was_active = active;
                }),
            ],
        }
    }
    fn active_items(&self) -> &[MenuItem] {
        if let Some(i) = self.submenu
            && let MenuItem::Submenu { items, .. } = &self.items[i]
        {
            return items;
        }
        &self.items
    }
    /// Open and accepting input (not playing its exit).
    fn is_open(&self, cx: &App) -> bool {
        self.presence.read(cx).is_open()
    }
    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_open(cx) {
            return;
        }
        if let Some(previous) = &self.previous {
            previous.focus(window, cx);
        }
        // `DismissEvent` follows once the exit has played.
        self.presence.update(cx, |presence, cx| presence.close(cx));
        cx.notify();
    }
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_open(cx) {
            return;
        }
        let Some(i) = self.nav.selected else {
            return;
        };
        match self.active_items()[i].clone() {
            MenuItem::Action {
                disabled_reason: None,
                handler,
                ..
            } => {
                self.dismiss(window, cx);
                handler(window, cx);
            }
            MenuItem::Submenu { items, .. } if self.submenu.is_none() => {
                self.parent_selection = self.nav.selected;
                self.submenu = Some(i);
                self.nav.edge(&items, false);
                cx.notify();
            }
            _ => {}
        }
    }
    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_open(cx) {
            return;
        }
        let items = self.active_items().to_vec();
        match event.keystroke.key.as_str() {
            "up" => self.nav.step(&items, false),
            "down" => self.nav.step(&items, true),
            "home" => self.nav.edge(&items, false),
            "end" => self.nav.edge(&items, true),
            "enter" | "space" => self.activate(window, cx),
            "right" => {
                if self
                    .nav
                    .selected
                    .is_some_and(|i| matches!(items[i], MenuItem::Submenu { .. }))
                {
                    self.activate(window, cx);
                }
            }
            "left" if self.submenu.is_some() => {
                self.submenu = None;
                self.nav.selected = self.parent_selection;
            }
            "escape" => self.dismiss(window, cx),
            _ => {
                if !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.alt
                    && let Some(text) = &event.keystroke.key_char
                    && !text.chars().any(char::is_control)
                {
                    let now = Instant::now();
                    self.nav.type_ahead(
                        &items,
                        text,
                        self.last_type.is_none_or(|t| {
                            now.duration_since(t)
                                > crate::ui::theme::motion::timing::TYPEAHEAD_RESET
                        }),
                    );
                    self.last_type = Some(now);
                } else {
                    return;
                }
            }
        }
        if let Some(index) = self.nav.selected {
            if self.submenu.is_some() {
                self.child_scroll.scroll_to_item(index);
            } else {
                self.scroll.scroll_to_item(index);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
}
impl Menu {
    fn panel(
        &self,
        items: &[MenuItem],
        child: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = *palette(cx);
        let mut panel = div()
            .id(if child { "submenu-panel" } else { "menu-panel" })
            .min_w(controls::MENU_MIN_WIDTH)
            .max_w(window.viewport_size().width - space::S16)
            .max_h((window.viewport_size().height - space::S16).max(size::ROW))
            .overflow_y_scroll()
            .track_scroll(if child {
                &self.child_scroll
            } else {
                &self.scroll
            })
            .occlude()
            .role(Role::Menu)
            .aria_label(self.label.clone())
            .bg(p.popover)
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border)
            .shadow(elevation::lg(&p))
            .p(space::S4)
            .text(TextSize::Base)
            .text_color(p.foreground);
        for (i, item) in items.iter().enumerate() {
            if matches!(item, MenuItem::Separator) {
                panel = panel.child(
                    div()
                        .h(size::HAIRLINE)
                        .mx(space::S8)
                        .my(space::S4)
                        .bg(p.border),
                );
                continue;
            }
            let enabled = item.enabled();
            let selected = if child || self.submenu.is_none() {
                self.nav.selected == Some(i)
            } else {
                self.parent_selection == Some(i)
            };
            let (role, checked, expanded) = match item {
                MenuItem::Action { checked: true, .. } => (Role::MenuItemCheckBox, true, None),
                MenuItem::Submenu { .. } => (Role::MenuItem, false, Some(self.submenu == Some(i))),
                _ => (Role::MenuItem, false, None),
            };
            let mut row = div()
                .id(i)
                .role(role)
                .aria_label(SharedString::from(
                    item.label().unwrap_or_default().to_owned(),
                ))
                .when(checked, |el| el.aria_toggled(Toggled::True))
                .when_some(expanded, |el, open| el.aria_expanded(open))
                .when(selected, |el| el.aria_active_descendant())
                .when_some(item.disabled_text(), |el, reason| {
                    el.aria_description(reason)
                })
                .h(size::ROW)
                .flex()
                .items_center()
                .gap(space::S8)
                .px(space::S8)
                .rounded(radius::MD)
                .text_color(if enabled {
                    p.foreground
                } else {
                    p.subtle_foreground
                })
                .when(enabled, |row| {
                    row.on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if !child && this.submenu.is_some_and(|open| open != i) {
                            this.submenu = None;
                            this.nav.selected = None;
                        }
                        let items = if child {
                            this.active_items().to_vec()
                        } else {
                            this.items.clone()
                        };
                        if (child || this.submenu.is_none()) && this.nav.hover(&items, i) {
                            cx.notify();
                        }
                    }))
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if enabled {
                        if !child {
                            this.submenu = None;
                        }
                        this.nav.selected = Some(i);
                        this.activate(window, cx);
                    }
                    cx.stop_propagation();
                }));
            match item {
                MenuItem::Action {
                    label,
                    shortcut,
                    icon,
                    checked,
                    disabled_reason,
                    ..
                } => {
                    row = row
                        .child(div().w(size::ICON).when(*checked || icon.is_some(), |d| {
                            d.child(
                                Icon::new(if *checked {
                                    IconName::Check
                                } else {
                                    icon.unwrap_or(IconName::Check)
                                })
                                .color(if enabled {
                                    p.muted_foreground
                                } else {
                                    p.subtle_foreground
                                }),
                            )
                        }))
                        .child(div().flex_1().child(label.clone()));
                    if let Some(shortcut) = shortcut {
                        row = row.child(
                            div()
                                .text(TextSize::Sm)
                                .text_color(p.subtle_foreground)
                                .child(shortcut.clone()),
                        );
                    }
                    if let Some(reason) = disabled_reason {
                        row = row
                            .tooltip(tooltip(reason.clone()))
                            .tooltip_show_delay(crate::ui::theme::motion::timing::TOOLTIP_DELAY);
                    }
                }
                MenuItem::Submenu { label, .. } => {
                    row = row
                        .child(div().w(size::ICON))
                        .child(div().flex_1().child(label.clone()))
                        .child(Icon::new(IconName::ChevronRight));
                }
                MenuItem::Separator => unreachable!(),
            }
            let row = row.bg(if selected { p.accent } else { p.popover });
            panel = panel.child(row);
        }
        panel
    }
}
impl Render for Menu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div()
            .id("menu")
            .key_context("KinemaMenu")
            .track_focus(&self.focus)
            .flex()
            .items_start()
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.dismiss(window, cx)))
            .child(self.panel(&self.items, false, window, cx));
        if let Some(index) = self.submenu
            && let MenuItem::Submenu { items, .. } = &self.items[index]
        {
            panel = panel.child(self.panel(items, true, window, cx));
        }
        self.presence.read(cx).animate(panel, "menu", cx)
    }
}

mod context;
mod item;
mod select;

pub use context::{ContextMenu, ContextMenuExt, bind_keys};
pub use item::{menu_item, menu_item_icon};
pub use select::{Select, SelectEvent};

#[cfg(test)]
mod tests {
    use super::*;
    fn action(label: &str, disabled: bool) -> MenuItem {
        MenuItem::Action {
            label: label.to_owned().into(),
            shortcut: None,
            icon: None,
            disabled_reason: disabled.then(|| "Unavailable".into()),
            checked: false,
            handler: Rc::new(|_, _| {}),
        }
    }
    #[::core::prelude::v1::test]
    fn hover_sets_single_index_and_arrows_continue() {
        let items = vec![
            action("Alpha", false),
            MenuItem::Separator,
            action("Blocked", true),
            action("Beta", false),
            action("Gamma", false),
        ];
        let mut nav = MenuNavigation::default();
        assert!(nav.hover(&items, 3));
        assert_eq!(nav.selected, Some(3));
        assert!(!nav.hover(&items, 3), "unchanged hover does not repaint");
        assert!(!nav.hover(&items, 1), "separators never highlight");
        assert!(!nav.hover(&items, 2), "disabled rows never highlight");
        assert_eq!(nav.selected, Some(3));
        nav.step(&items, true);
        assert_eq!(nav.selected, Some(4));
        assert!(nav.hover(&items, 0));
        nav.step(&items, true);
        assert_eq!(nav.selected, Some(3));
    }
    #[::core::prelude::v1::test]
    fn navigation_skips_and_wraps() {
        let items = vec![
            MenuItem::Separator,
            action("Alpha", false),
            action("Blocked", true),
            action("Beta", false),
        ];
        let mut nav = MenuNavigation::default();
        nav.step(&items, true);
        assert_eq!(nav.selected, Some(1));
        nav.step(&items, false);
        assert_eq!(nav.selected, Some(3));
        nav.step(&items, true);
        assert_eq!(nav.selected, Some(1));
        nav.edge(&items, true);
        assert_eq!(nav.selected, Some(3));
        nav.edge(&items, false);
        assert_eq!(nav.selected, Some(1));
    }
    #[::core::prelude::v1::test]
    fn type_ahead_and_empty() {
        let items = vec![
            action("Alpine", false),
            action("Alpha", true),
            action("Beta", false),
            action("Almond", false),
        ];
        let mut nav = MenuNavigation::default();
        nav.type_ahead(&items, "a", true);
        assert_eq!(nav.selected, Some(0));
        nav.type_ahead(&items, "l", false);
        assert_eq!(nav.selected, Some(3));
        nav.type_ahead(&items, "b", true);
        assert_eq!(nav.selected, Some(2));
        nav.step(&[], true);
        assert_eq!(nav.selected, None);
        nav.type_ahead(&[], "z", true);
        assert_eq!(nav.selected, None);
    }
    #[::core::prelude::v1::test]
    fn all_disabled_has_no_selection() {
        let mut nav = MenuNavigation::default();
        let items = vec![MenuItem::Separator, action("No", true)];
        nav.step(&items, false);
        assert_eq!(nav.selected, None);
        nav.type_ahead(&items, "n", true);
        assert_eq!(nav.selected, None);
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(
    clippy::unwrap_used,
    reason = "test setup and event dispatch must succeed"
)]
mod interaction_tests {
    use super::context::OpenMenus;
    use super::*;
    use core::prelude::v1::test;
    use std::cell::Cell;

    struct ContextFixture {
        root: FocusHandle,
        target: FocusHandle,
        next: FocusHandle,
    }
    impl Render for ContextFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("fixture")
                .track_focus(&self.root)
                .child(
                    div()
                        .id("target")
                        .track_focus(&self.target)
                        .child("Target")
                        .context_menu("context-target", |_, _| vec![]),
                )
                .child(div().id("next").track_focus(&self.next).child("Next"))
        }
    }

    #[gpui::test]
    fn context_adapter_has_one_tab_stop_and_preserves_keyboard_target(cx: &mut TestAppContext) {
        let handle = cx.add_window(|window, cx| {
            crate::ui::theme::init(cx);
            crate::ui::theme::set_reduce_motion(true, cx);
            bind_keys(cx);
            let root = cx.focus_handle().tab_stop(false);
            root.focus(window, cx);
            ContextFixture {
                root,
                target: cx.focus_handle().tab_stop(true),
                next: cx.focus_handle().tab_stop(true),
            }
        });
        let Ok((target, next)) = handle.update(cx, |fixture, _, _| {
            (fixture.target.clone(), fixture.next.clone())
        }) else {
            panic!("fixture handles unavailable");
        };
        let navigation = cx.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            window.focus_next(cx);
            assert!(
                target.is_focused(window),
                "Tab must reach the child, not its adapter"
            );
            window.focus_next(cx);
            assert!(
                next.is_focused(window),
                "Target must have exactly one tab stop"
            );
            window.focus_prev(cx);
            assert!(target.is_focused(window));
        });
        assert!(navigation.is_ok(), "tab navigation failed: {navigation:?}");
        cx.simulate_keystrokes(handle.into(), "shift-f10");
        let opened = cx.update_window(handle.into(), |_, window, cx| {
            window.draw(cx).clear(cx);
            assert!(!target.is_focused(window));
            assert!(
                cx.global::<OpenMenus>()
                    .0
                    .contains_key(&window.window_handle().window_id()),
                "Focused descendant must inherit the context binding"
            );
        });
        assert!(opened.is_ok(), "context menu update failed: {opened:?}");
        cx.simulate_keystrokes(handle.into(), "escape");
        let returned = cx.update_window(handle.into(), |_, window, _| {
            assert!(target.is_focused(window))
        });
        assert!(returned.is_ok(), "return to child failed: {returned:?}");
    }

    #[gpui::test]
    fn bounds_change_dismisses_menu(cx: &mut TestAppContext) {
        let handle = cx.add_window(|window, cx| {
            crate::ui::theme::init(cx);
            crate::ui::theme::set_reduce_motion(true, cx);
            Menu::new(vec![], window, cx)
        });
        cx.run_until_parked();
        cx.simulate_window_resize(handle.into(), size(px(640.), px(480.)));
        cx.run_until_parked();
        let dismissed = handle.update(cx, |menu, _, cx| assert!(!menu.is_open(cx)));
        assert!(dismissed.is_ok(), "menu update failed: {dismissed:?}");
    }

    #[gpui::test]
    fn window_deactivation_dismisses_menu(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, cx| {
            crate::ui::theme::init(cx);
            crate::ui::theme::set_reduce_motion(true, cx);
            ContextFixture {
                root: cx.focus_handle(),
                target: cx.focus_handle().tab_stop(true),
                next: cx.focus_handle().tab_stop(true),
            }
        });
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        // Open after initial window setup/bounds notifications have settled,
        // as a user-opened menu would be.
        let menu = cx.update(|window, cx| cx.new(|cx| Menu::new(vec![], window, cx)));
        cx.update(|_, cx| assert!(menu.read(cx).is_open(cx)));
        cx.deactivate_window();
        cx.update(|_, cx| assert!(!menu.read(cx).is_open(cx)));
    }

    #[gpui::test]
    fn opening_and_dismissal_restore_focus(cx: &mut TestAppContext) {
        let handle = cx.add_window(|window, cx| {
            crate::ui::theme::init(cx);
            crate::ui::theme::set_reduce_motion(true, cx);
            let previous = cx.focus_handle();
            previous.focus(window, cx);
            Menu::new(vec![], window, cx)
        });
        let dismissed = Rc::new(Cell::new(0));
        let subscription = cx.update(|cx| {
            let count = dismissed.clone();
            let Ok(menu) = handle.root(cx) else {
                panic!("menu root unavailable");
            };
            cx.subscribe(&menu, move |_, _: &DismissEvent, _| {
                count.set(count.get() + 1)
            })
        });
        let updated = handle.update(cx, |menu, window, cx| {
            assert!(menu.focus.is_focused(window));
            menu.dismiss(window, cx);
            assert!(menu.previous.as_ref().is_some_and(|f| f.is_focused(window)));
            menu.dismiss(window, cx);
        });
        assert!(updated.is_ok(), "window update failed: {updated:?}");
        cx.run_until_parked();
        assert_eq!(dismissed.get(), 1);
        drop(subscription);
    }

    struct MenuHost {
        previous: FocusHandle,
        menu: Option<Entity<Menu>>,
        chosen: Rc<Cell<Option<&'static str>>>,
    }
    impl Render for MenuHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("host")
                .track_focus(&self.previous)
                .children(self.menu.clone())
        }
    }

    #[gpui::test]
    fn arrows_activate_and_escape_returns_focus(cx: &mut TestAppContext) {
        let chosen = Rc::new(Cell::new(None));
        let handle = cx.add_window({
            let chosen = chosen.clone();
            move |window, cx| {
                crate::ui::theme::init(cx);
                crate::ui::theme::set_reduce_motion(true, cx);
                let previous = cx.focus_handle();
                previous.focus(window, cx);
                MenuHost {
                    previous,
                    menu: None,
                    chosen,
                }
            }
        });
        for (keys, expect) in [("down enter", Some("Beta")), ("escape", None)] {
            handle
                .update(cx, |host, window, cx| {
                    let chosen = host.chosen.clone();
                    let pick = |name: &'static str| {
                        let chosen = chosen.clone();
                        menu_item(name, move |_, _| chosen.set(Some(name)))
                    };
                    host.chosen.set(None);
                    host.previous.focus(window, cx);
                    host.menu = Some(cx.new(|cx| {
                        Menu::new(vec![pick("Alpha"), pick("Beta")], window, cx).label("Actions")
                    }));
                    cx.notify();
                })
                .unwrap();
            cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
                .unwrap();
            cx.simulate_keystrokes(handle.into(), keys);
            cx.run_until_parked();
            handle
                .update(cx, |host, window, _| {
                    assert_eq!(host.chosen.get(), expect, "{keys}");
                    assert!(host.previous.is_focused(window), "{keys}: focus returned");
                })
                .unwrap();
        }
        assert_eq!(chosen.get(), None);
    }
}
