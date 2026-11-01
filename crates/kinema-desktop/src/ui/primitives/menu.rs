//! Rounded native menus and context-menu/select adapters.
//!
//! A window-keyed registry owns only a weak reference: opening a context menu
//! dismisses the previous one without keeping an unmounted trigger alive.
use crate::{
    ui::motion::{self, Motion},
    ui::primitives::icon::{Icon, IconName},
    ui::primitives::tooltip::tooltip,
    ui::theme::typography::TypeExt,
    ui::theme::{elevation, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};
use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

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
    last_type: Option<Instant>,
    closing: bool,
    close_task: Option<Task<()>>,
    progress: Rc<Cell<f32>>,
    close_from: f32,
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
        let initial_size = window.viewport_size();
        let mut was_active = window.is_window_active();
        Self {
            items,
            nav,
            submenu: None,
            parent_selection: None,
            focus,
            previous,
            last_type: None,
            closing: false,
            close_task: None,
            progress: Rc::new(Cell::new(0.)),
            close_from: 1.,
            scroll: ScrollHandle::new(),
            child_scroll: ScrollHandle::new(),
            _window_subscriptions: vec![
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
    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.close_from = self.progress.get();
        self.closing = true;
        if let Some(previous) = &self.previous {
            previous.focus(window, cx);
        }
        let delay = motion::duration(Motion::PopupOut, cx);
        if delay.is_zero() {
            cx.emit(DismissEvent);
        } else {
            self.close_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update(cx, |_, cx| cx.emit(DismissEvent));
            }));
        }
        cx.notify();
    }
    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
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
        if self.closing {
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
                        self.last_type
                            .is_none_or(|t| now.duration_since(t) > Duration::from_millis(700)),
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
            .min_w(px(200.))
            .max_w(window.viewport_size().width - space::S16)
            .max_h((window.viewport_size().height - space::S16).max(space::ROW))
            .overflow_y_scroll()
            .track_scroll(if child {
                &self.child_scroll
            } else {
                &self.scroll
            })
            .occlude()
            .bg(p.bg_overlay)
            .rounded(radius::XL)
            .border_1()
            .border_color(p.line_subtle)
            .shadow(elevation::lg(&p))
            .p(space::S4)
            .type_style(typeface::BODY)
            .text_color(p.text_primary);
        for (i, item) in items.iter().enumerate() {
            if matches!(item, MenuItem::Separator) {
                panel = panel.child(
                    div()
                        .h(space::HAIRLINE)
                        .mx(space::S8)
                        .my(space::S4)
                        .bg(p.line_subtle),
                );
                continue;
            }
            let enabled = item.enabled();
            let selected = if child || self.submenu.is_none() {
                self.nav.selected == Some(i)
            } else {
                self.parent_selection == Some(i)
            };
            let mut row = div()
                .id(i)
                .h(space::ROW)
                .flex()
                .items_center()
                .gap(space::S8)
                .px(space::S8)
                .rounded(radius::MD)
                .text_color(if enabled {
                    p.text_primary
                } else {
                    p.text_muted
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
                        .child(div().w(space::ICON).when(*checked || icon.is_some(), |d| {
                            d.child(
                                Icon::new(if *checked {
                                    IconName::Check
                                } else {
                                    icon.unwrap_or(IconName::Check)
                                })
                                .color(if enabled {
                                    p.text_secondary
                                } else {
                                    p.text_muted
                                }),
                            )
                        }))
                        .child(div().flex_1().child(label.clone()));
                    if let Some(shortcut) = shortcut {
                        row = row.child(
                            div()
                                .type_style(typeface::SMALL)
                                .text_color(p.text_muted)
                                .child(shortcut.clone()),
                        );
                    }
                    if let Some(reason) = disabled_reason {
                        row = row
                            .tooltip(tooltip(reason.clone()))
                            .tooltip_show_delay(Duration::from_millis(500));
                    }
                }
                MenuItem::Submenu { label, .. } => {
                    row = row
                        .child(div().w(space::ICON))
                        .child(div().flex_1().child(label.clone()))
                        .child(Icon::new(IconName::ChevronRight));
                }
                MenuItem::Separator => unreachable!(),
            }
            let row = row.bg(if selected { p.ctl_hover } else { p.bg_overlay });
            panel = panel.child(row);
        }
        panel
    }
}
impl Render for Menu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closing
            && motion::duration(Motion::PopupOut, cx).is_zero()
            && self.close_task.take().is_some()
        {
            cx.emit(DismissEvent);
        }
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
        let closing = self.closing;
        let token = if closing {
            Motion::PopupOut
        } else {
            Motion::PopupIn
        };
        let duration = motion::duration(token, cx);
        let progress_value = self.progress.clone();
        let close_from = self.close_from;
        if duration.is_zero() {
            progress_value.set(1.);
            panel.into_any_element()
        } else {
            panel
                .with_animation(
                    if closing { "menu-out" } else { "menu-in" },
                    Animation::new(duration).with_easing(motion::ease_out_cubic),
                    move |el, progress| {
                        let t = if closing {
                            close_from * (1. - progress)
                        } else {
                            progress
                        };
                        progress_value.set(t);
                        el.opacity(t).relative().top(px(4. * (1. - t)))
                    },
                )
                .into_any_element()
        }
    }
}

actions!(kinema_context_menu, [OpenContextMenu]);
/// Install once at application startup. The wrapper supplies the dispatch context.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("shift-f10", OpenContextMenu, Some("KinemaContextMenu")),
        KeyBinding::new("menu", OpenContextMenu, Some("KinemaContextMenu")),
    ]);
}
#[derive(Default)]
struct OpenMenus(HashMap<WindowId, WeakEntity<Menu>>);
impl Global for OpenMenus {}
#[derive(Default)]
struct ContextState {
    menu: Option<Entity<Menu>>,
    position: Point<Pixels>,
    subscription: Option<Subscription>,
}

/// Adds pointer and keyboard context-menu support to an interactive element.
/// The wrapper is not a tab stop: keyboard actions bubble from the focused
/// descendant through its key context. Focus stays on that descendant for
/// keyboard opening/return; pointer opening has a non-tab-stop fallback handle
/// when the target has no focused descendant.
pub trait ContextMenuExt: InteractiveElement + IntoElement + Sized + 'static {
    fn context_menu(
        self,
        id: impl Into<ElementId>,
        builder: impl Fn(&mut Window, &mut App) -> Vec<MenuItem> + 'static,
    ) -> ContextMenu<Self> {
        ContextMenu {
            element: self,
            id: id.into(),
            builder: Rc::new(builder),
            style: StyleRefinement::default(),
        }
    }
}
impl<E: InteractiveElement + IntoElement + Sized + 'static> ContextMenuExt for E {}
type MenuBuilder = Rc<dyn Fn(&mut Window, &mut App) -> Vec<MenuItem>>;
#[derive(IntoElement)]
pub struct ContextMenu<E: InteractiveElement + IntoElement + 'static> {
    element: E,
    id: ElementId,
    builder: MenuBuilder,
    /// Layout/appearance of the wrapper element, so a context menu never has
    /// to break the flex chain it sits in.
    style: StyleRefinement,
}
impl<E: InteractiveElement + IntoElement + 'static> Styled for ContextMenu<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl<E: InteractiveElement + IntoElement + 'static> RenderOnce for ContextMenu<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ContextState::default());
        let focus = window.use_keyed_state((self.id.clone(), "focus"), cx, |_, cx| {
            cx.focus_handle().tab_stop(false)
        });
        let focus = focus.read(cx).clone();
        let open = {
            let state = state.clone();
            let builder = self.builder;
            let focus = focus.clone();
            Rc::new(
                move |position: Point<Pixels>, pointer: bool, window: &mut Window, cx: &mut App| {
                    let old = cx
                        .try_global::<OpenMenus>()
                        .and_then(|g| g.0.get(&window.window_handle().window_id()))
                        .cloned();
                    if let Some(old) = old.and_then(|m| m.upgrade()) {
                        old.update(cx, |menu, cx| menu.dismiss(window, cx));
                    }
                    if !focus.contains_focused(window, cx) {
                        focus.focus(window, cx);
                    }
                    let items = builder(window, cx);
                    let menu = cx.new(|cx| Menu::new(items, window, cx).opened_by_pointer(pointer));
                    let subscription = cx.subscribe(&menu, {
                        let state = state.downgrade();
                        let window_id = window.window_handle().window_id();
                        move |menu, _: &DismissEvent, cx| {
                            let registry = cx.global_mut::<OpenMenus>();
                            if registry
                                .0
                                .get(&window_id)
                                .is_some_and(|open| open.entity_id() == menu.entity_id())
                            {
                                registry.0.remove(&window_id);
                            }
                            let _ = state.update(cx, |state, cx| {
                                state.menu = None;
                                cx.notify();
                            });
                        }
                    });
                    if cx.try_global::<OpenMenus>().is_none() {
                        cx.set_global(OpenMenus::default());
                    }
                    cx.global_mut::<OpenMenus>()
                        .0
                        .insert(window.window_handle().window_id(), menu.downgrade());
                    state.update(cx, |state, cx| {
                        state.menu = Some(menu);
                        state.position = position;
                        state.subscription = Some(subscription);
                        cx.notify();
                    });
                },
            )
        };
        let bounds = window.use_keyed_state((self.id.clone(), "bounds"), cx, |_, _| {
            Bounds::<Pixels>::default()
        });
        let key_bounds = bounds.clone();
        let key_open = open.clone();
        let mut root = div()
            .id(self.id)
            .relative()
            .key_context("KinemaContextMenu")
            .track_focus(&focus)
            .tab_stop(false)
            .on_action(move |_: &OpenContextMenu, window, cx| {
                key_open(key_bounds.read(cx).bottom_left(), false, window, cx);
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                open(event.position, true, window, cx);
                cx.stop_propagation();
            })
            .child(self.element)
            .child(
                canvas(
                    move |b, _, cx| {
                        bounds.update(cx, |stored, _| *stored = b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        root.style().refine(&self.style);
        let state = state.read(cx);
        if let Some(menu) = &state.menu {
            root = root.child(
                deferred(
                    anchored()
                        .position(state.position)
                        .snap_to_window_with_margin(space::S8)
                        .child(menu.clone()),
                )
                .priority(10),
            );
        }
        root
    }
}

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
        }
    }
    /// Leading glyph, e.g. a sort icon.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
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
        let menu = cx.new(|cx| Menu::new(items, window, cx).opened_by_pointer(pointer));
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
            .track_focus(&self.focus)
            .focus_visible(move |s| s.border(space::FOCUS_RING).border_color(p.focus_ring))
            .tab_stop(true)
            .relative()
            .rounded(if self.toolbar { radius::LG } else { radius::MD })
            .child({
                let face = div()
                    .flex()
                    .items_center()
                    .gap(space::S6)
                    .text_color(p.text_primary)
                    .children(
                        self.icon
                            .map(|icon| Icon::new(icon).color(p.text_secondary)),
                    )
                    .children(
                        self.prefix
                            .clone()
                            .map(|prefix| div().text_color(p.text_muted).child(prefix)),
                    )
                    .child(label)
                    .child(Icon::new(IconName::ChevronDown).color(p.text_secondary));
                if self.toolbar {
                    let (fill, hover, edge) = p.glass();
                    face.h(space::TOOLBAR_CONTROL)
                        .rounded(radius::LG)
                        .px(space::S12)
                        .bg(fill)
                        .border(space::HAIRLINE)
                        .border_color(edge)
                        .type_style(typeface::SMALL_MEDIUM)
                        .hover(move |s| s.bg(hover))
                } else {
                    face.h(space::CONTROL_HEIGHT)
                        .rounded(radius::MD)
                        .px(space::S8)
                        .bg(p.ctl_rest)
                        .border_1()
                        .border_color(p.line_control)
                        .type_style(typeface::BODY)
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
mod interaction_tests {
    use super::*;
    use core::prelude::v1::test;

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
        let dismissed = handle.update(cx, |menu, _, _| assert!(menu.closing));
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
        cx.update(|_, cx| assert!(!menu.read(cx).closing));
        cx.deactivate_window();
        cx.update(|_, cx| assert!(menu.read(cx).closing));
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
}
