//! Library's single Filters control: watch status and sort in one menu. It shows
//! the word "Filters" at normal widths and only an icon (with tooltip) when the
//! window is narrow, so the 48px toolbar row never grows.
use super::library::LibraryFilters;
use crate::widgets::{
    button::{Button, ButtonVariant},
    icon::IconName,
    menu::{Menu, MenuItem},
};
use gpui::{prelude::*, *};
use kinema_core::library::{LibrarySort, WatchFilter};
use std::rc::Rc;

/// Below this window width the control collapses to its icon.
pub(crate) const COMPACT_BELOW: Pixels = px(1100.);

/// New status/sort selection; the host reloads the Library query.
pub(crate) struct FiltersChanged(pub LibraryFilters);
impl EventEmitter<FiltersChanged> for FiltersButton {}

pub(crate) struct FiltersButton {
    filters: LibraryFilters,
    focus: FocusHandle,
    menu: Option<Entity<Menu>>,
    _menu_subscription: Option<Subscription>,
}
impl FiltersButton {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            filters: LibraryFilters::default(),
            focus: cx.focus_handle().tab_stop(true),
            menu: None,
            _menu_subscription: None,
        }
    }
    /// Mirror the host's filters (kind decides whether In progress is offered).
    pub(crate) fn set_filters(&mut self, filters: LibraryFilters, cx: &mut Context<Self>) {
        if self.filters != filters {
            self.filters = filters;
            cx.notify();
        }
    }
    /// Number of non-default choices, shown beside the label.
    fn active(&self) -> usize {
        usize::from(self.filters.status != WatchFilter::Any)
            + usize::from(self.filters.sort != LibrarySort::RecentlyAdded)
    }
    fn items(&self, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let weak = cx.entity().downgrade();
        let mut items = Vec::new();
        for (status, label) in LibraryFilters::status_options(self.filters.kind) {
            let weak = weak.clone();
            items.push(MenuItem::Action {
                label,
                shortcut: None,
                icon: None,
                disabled_reason: None,
                checked: self.filters.status == status,
                handler: Rc::new(move |_, cx| {
                    let _ = weak.update(cx, |this, cx| this.choose(|f| f.status = status, cx));
                }),
            });
        }
        items.push(MenuItem::Separator);
        for (sort, label) in LibraryFilters::sort_options() {
            let weak = weak.clone();
            items.push(MenuItem::Action {
                label,
                shortcut: None,
                icon: None,
                disabled_reason: None,
                checked: self.filters.sort == sort,
                handler: Rc::new(move |_, cx| {
                    let _ = weak.update(cx, |this, cx| this.choose(|f| f.sort = sort, cx));
                }),
            });
        }
        items
    }
    fn choose(&mut self, change: impl FnOnce(&mut LibraryFilters), cx: &mut Context<Self>) {
        let before = self.filters;
        change(&mut self.filters);
        if self.filters != before {
            cx.emit(FiltersChanged(self.filters));
            cx.notify();
        }
    }
    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &self.menu {
            menu.update(cx, |menu, cx| menu.dismiss(window, cx));
            return;
        }
        self.focus.focus(window, cx);
        let items = self.items(cx);
        let menu = cx.new(|cx| Menu::new(items, window, cx));
        self._menu_subscription = Some(cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.menu = None;
            cx.notify();
        }));
        self.menu = Some(menu);
        cx.notify();
    }
}
impl Focusable for FiltersButton {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for FiltersButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < COMPACT_BELOW;
        let active = self.active();
        let open = cx.listener(|this, _, window, cx| this.toggle(window, cx));
        let trigger = if compact {
            // Secondary (filled, bordered) rather than a bare ghost glyph: the
            // control must stay legible over any ambient backdrop.
            let mut button = Button::new("library-filters", "")
                .variant(ButtonVariant::Secondary)
                .icon(IconName::Menu)
                .focus_handle(self.focus.clone())
                .on_click(open);
            button.square = true;
            button.tooltip = Some("Filters".into());
            button.toggled = active > 0 || self.menu.is_some();
            button.into_any_element()
        } else {
            let label = if active > 0 {
                format!("Filters · {active}")
            } else {
                "Filters".to_owned()
            };
            Button::new("library-filters", label)
                .focus_handle(self.focus.clone())
                .on_click(open)
                .into_any_element()
        };
        div()
            .relative()
            .debug_selector(|| "library-filters".into())
            .child(trigger)
            .when_some(self.menu.clone(), |el, menu| {
                el.child(
                    div().absolute().top_full().right_0().child(
                        deferred(
                            anchored()
                                .snap_to_window_with_margin(crate::widgets::tokens::space::S8)
                                .child(menu),
                        )
                        .priority(10),
                    ),
                )
            })
    }
}
