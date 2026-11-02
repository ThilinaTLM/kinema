//! V3 composition root: persistent Browse, route-local Detail, and shared sessions.
use super::{
    browse::{
        Browse, BrowseRequest, LibraryFilters, OpenTitle, Scope, Sort,
        filters::{self, DiscoverMode, DiscoverPanel},
    },
    detail::{Detail, OpenDetailTitle},
};
use crate::controllers::coordinator::{AppCoordinator, BrowseTitle, CoordinatorEvent};
use crate::ui::theme::layout::browse::{SCRIM_RAMP, SEARCH_MIN_WIDTH, SEARCH_WIDTH};
use crate::ui::theme::{motion::timing, opacity};
use crate::{
    ui::blocks::toolbar::{Toolbar, ToolbarChip, ToolbarGroup, TransferStatus, icon_cell},
    ui::motion::{AnimateExt, transition::DISCLOSE},
    ui::primitives::alert::{Alert, AlertTone},
    ui::primitives::button::Button,
    ui::primitives::icon::IconName,
    ui::primitives::menu::Menu,
    ui::primitives::menu_item_icon,
    ui::primitives::popover::{Popover, PopoverAlign, PopoverHandle},
    ui::primitives::search_field::{SearchField, SearchFieldEvent},
    ui::primitives::segmented::Segmented,
    ui::primitives::text_input::TextInputEvent,
    ui::theme::typography::{TextSize, TypeExt},
    ui::theme::{self, size, space as s},
};
mod header;
use header::{scope_face, scroll_scrim};
mod routes;
#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

use gpui::{prelude::*, *};
use kinema_core::{MediaKind, library::LibraryKind, provider::ProviderId};
use std::{cell::RefCell, rc::Rc};

actions!(
    v3_main,
    [
        V3Search,
        V3Refresh,
        V3Local,
        V3Back,
        V3Settings,
        V3Transfers,
        V3About,
        V3Quit,
        V3NextFocus,
        V3PreviousFocus
    ]
);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-f", V3Search, Some("KinemaMain")),
        KeyBinding::new("secondary-k", V3Search, Some("KinemaMain")),
        KeyBinding::new("f5", V3Refresh, Some("KinemaMain")),
        KeyBinding::new("secondary-r", V3Refresh, Some("KinemaMain")),
        KeyBinding::new("secondary-o", V3Local, Some("KinemaMain")),
        KeyBinding::new("alt-left", V3Back, Some("KinemaMain")),
        KeyBinding::new("escape", V3Back, Some("KinemaMain")),
        KeyBinding::new("backspace", V3Back, Some("KinemaMain")),
        KeyBinding::new("secondary-,", V3Settings, None),
        KeyBinding::new("secondary-q", V3Quit, None),
        KeyBinding::new("tab", V3NextFocus, Some("KinemaMain")),
        KeyBinding::new("shift-tab", V3PreviousFocus, Some("KinemaMain")),
    ]);
}
/// Search shortcut hint shown in the field; matches the `secondary-k` binding.
const SEARCH_HINT: &str = if cfg!(target_os = "macos") {
    "⌘K"
} else {
    "Ctrl K"
};

pub(crate) enum Route {
    Browse,
    Detail(Entity<super::detail::Detail>),
}
pub struct MainWindow {
    pub(crate) coordinator: Entity<AppCoordinator>,
    browse: Entity<Browse>,
    transfers: Entity<super::transfers::Transfers>,
    transfers_popover: Rc<RefCell<Option<PopoverHandle>>>,
    search: Entity<SearchField>,
    notice_seen: Option<String>,
    notice_task: Option<Task<()>>,
    pub(crate) route: Route,
    detail_subscription: Option<Subscription>,
    launch_pulse: u64,
    menu: Option<Entity<Menu>>,
    menu_subscription: Option<Subscription>,
    chooser: Option<Task<()>>,
    cancel_focus: FocusHandle,
    focus: FocusHandle,
    observed_query: String,
    _subscriptions: Vec<Subscription>,
}
impl MainWindow {
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn quit_cancel_is_focused(&self, window: &Window) -> bool {
        self.cancel_focus.is_focused(window)
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let coordinator = cx.new(AppCoordinator::new);
        super::settings::install(&coordinator, cx);
        Self::with_coordinator(coordinator, window, cx)
    }
    pub(crate) fn with_coordinator(
        coordinator: Entity<AppCoordinator>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        super::quit::install(&coordinator, window.window_handle(), cx);
        crate::controllers::lifecycle::install_native_visibility(window, cx);
        let transfers = cx.new(|cx| super::transfers::Transfers::new(coordinator.clone(), cx));
        let browse = cx.new(|cx| Browse::new(coordinator.clone(), cx));
        let search = cx.new(|cx| {
            let mut s = SearchField::new(window, cx)
                .shortcut_hint(SEARCH_HINT)
                .toolbar_style();
            s.set_placeholder("Search movies…", cx);
            s
        });
        let editor = search.read(cx).input();
        let subscriptions = vec![
            cx.subscribe(&coordinator, |this, _, event: &CoordinatorEvent, cx| {
                if matches!(
                    event,
                    CoordinatorEvent::App(crate::app::AppEvent {
                        outcome: crate::app::AppOutcome::PlaybackStarted { .. }
                            | crate::app::AppOutcome::SessionRelaunched { .. },
                        ..
                    })
                ) {
                    this.launch_pulse += 1;
                    cx.notify();
                }
            }),
            cx.subscribe_in(
                &editor,
                window,
                |this, _, event: &TextInputEvent, window, cx| {
                    if matches!(event, TextInputEvent::Cancelled) {
                        let browse = this.browse.clone();
                        window.defer(cx, move |window, cx| {
                            browse.update(cx, |b, cx| b.restore_focus(window, cx))
                        });
                    }
                },
            ),
            cx.observe(&coordinator, |this, coordinator, cx| {
                let query = coordinator.read(cx).query.clone();
                // Library keeps its own search text; Discover's query never overwrites it.
                if query != this.observed_query && this.browse.read(cx).scope() == Scope::Discover {
                    this.observed_query = query.clone();
                    if this.search.read(cx).text(cx) != query {
                        this.search.update(cx, |s, cx| s.set_text(query, cx));
                    }
                }
                let notice = coordinator.read(cx).personal_notice.clone();
                if notice != this.notice_seen {
                    this.notice_seen.clone_from(&notice);
                    this.notice_task = None;
                    // Only confirmations expire. Every failure (retryable or not)
                    // stays until Retry, Dismiss or a newer outcome replaces it.
                    if let Some(message) = notice
                        && !coordinator.read(cx).personal_notice_failed()
                    {
                        this.notice_task = Some(cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(timing::NOTICE_TIMEOUT).await;
                            let _ = this.update(cx, |this, cx| {
                                this.coordinator.update(cx, |c, cx| {
                                    if c.personal_notice.as_deref() == Some(message.as_str())
                                        && !c.personal_notice_failed()
                                    {
                                        c.dismiss_personal_notice(cx);
                                    }
                                })
                            });
                        }));
                    }
                }
                cx.notify();
            }),
            cx.subscribe(&search, |this, search, event: &SearchFieldEvent, cx| {
                // Routed by the scope showing *now*: a debounce that outlived a scope
                // switch carries the other scope's text and is a no-op there.
                let text = search.read(cx).text(cx).to_owned();
                let submitted = *event == SearchFieldEvent::Submitted;
                match this.browse.read(cx).scope() {
                    Scope::Library => {
                        let (filters, same) = {
                            let browse = this.browse.read(cx);
                            (browse.library_filters(), browse.library_search() == text)
                        };
                        if !same || submitted {
                            this.browse
                                .update(cx, |b, cx| b.set_library(filters, &text, cx));
                        }
                    }
                    Scope::Discover => {
                        if submitted || this.coordinator.read(cx).query != text {
                            this.coordinator
                                .update(cx, |c, cx| c.load(c.kind, text, false, cx));
                        }
                    }
                }
            }),
            cx.subscribe_in(
                &browse,
                window,
                |this, _, request: &BrowseRequest, window, cx| match request {
                    BrowseRequest::ShowDiscover => this.switch_scope(Scope::Discover, window, cx),
                    BrowseRequest::ClearLibraryFilters => this.clear_library(cx),
                },
            ),
            cx.subscribe_in(&browse, window, |this, _, event: &OpenTitle, window, cx| {
                this.open_title(event.0.clone(), window, cx);
            }),
        ];
        let weak = coordinator.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            if let Some(coordinator) = weak.upgrade() {
                crate::controllers::lifecycle::close_main(&coordinator, window, cx);
            } else {
                cx.quit();
            }
            false
        });
        let focus = cx.focus_handle();
        browse.update(cx, |b, cx| b.restore_focus(window, cx));
        Self {
            focus,
            observed_query: String::new(),
            coordinator,
            browse,
            transfers,
            transfers_popover: Rc::new(RefCell::new(None)),
            search,
            notice_seen: None,
            notice_task: None,
            route: Route::Browse,
            detail_subscription: None,
            launch_pulse: 0,
            menu: None,
            menu_subscription: None,
            chooser: None,
            cancel_focus: cx.focus_handle().tab_stop(true),
            _subscriptions: subscriptions,
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *theme::palette(cx);
        let scope = self.browse.read(cx).scope();
        let personal_notice = self.personal_notice(cx);
        let in_detail = matches!(self.route, Route::Detail(_));
        let inset = if in_detail {
            super::detail::page_inset(window.viewport_size().width)
        } else {
            size::PAGE_INSET
        };
        let compact = window.viewport_size().width < theme::layout::browse::COMPACT_BELOW;
        let toolbar = if in_detail {
            let on_streams =
                matches!(&self.route, Route::Detail(d) if d.read(cx).on_streams_page());
            let (back_name, back_label) = if on_streams {
                ("Back to details", "Back to details".to_owned())
            } else {
                ("Back to browse", format!("Back to {}", scope_face(scope).1))
            };
            Toolbar::new()
                .inset(inset)
                .leading({
                    // Same glass chip face and 32 px height as the other header
                    // controls; the visible label is the destination, the
                    // accessible name and tooltip keep the plain Back wording.
                    let p = *theme::palette(cx);
                    div()
                        .id("detail-back")
                        .debug_selector(|| "detail-back".into())
                        .flex_none()
                        .rounded(theme::radius::LG)
                        .role(Role::Button)
                        .aria_label(SharedString::from(back_name))
                        .focusable()
                        .tab_stop(true)
                        .focus_visible(move |s| s.border(size::FOCUS_RING).border_color(p.ring))
                        .tooltip(crate::ui::primitives::tooltip::tooltip(back_name))
                        .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                        .child(ToolbarChip::new(IconName::ArrowLeft, back_label).chevron(false))
                })
                .trailing(self.global_group(window, cx))
        } else {
            Toolbar::new()
                .inset(inset)
                .center_width(SEARCH_WIDTH, SEARCH_MIN_WIDTH)
                .leading(self.scope_control(scope, compact, cx))
                .leading(self.kind_control(scope, cx))
                .center(self.search.clone())
                .trailing(self.filters_control(scope, compact, cx))
                .trailing(self.global_group(window, cx))
        };
        let scrolled: Rc<dyn Fn(&App) -> Pixels> = match &self.route {
            Route::Browse => {
                let browse = self.browse.clone();
                Rc::new(move |cx| browse.read(cx).scrolled_under_header())
            }
            Route::Detail(detail) => {
                let detail = detail.clone();
                Rc::new(move |cx| detail.read(cx).scrolled_under_header())
            }
        };
        let body = match &self.route {
            Route::Browse => self.browse.clone().into_any_element(),
            Route::Detail(detail) => detail.clone().into_any_element(),
        };
        let confirmation = self.coordinator.read(cx).quit_requested
            && self.coordinator.read(cx).needs_quit_confirmation();
        div()
            .id("v3-main")
            .track_focus(&self.focus)
            .key_context("KinemaMain")
            .on_action(cx.listener(|this, _: &V3Search, window, cx| {
                if !matches!(this.route, Route::Browse) {
                    this.leave_detail(window, cx);
                }
                this.search.update(cx, |s, cx| s.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &V3Refresh, _, cx| {
                this.refresh(cx);
            }))
            .on_action(cx.listener(|this, _: &V3Local, window, cx| this.open_local(window, cx)))
            .on_action(cx.listener(|this, _: &V3Transfers, window, cx| {
                if this.coordinator.read(cx).quit_requested {
                    return;
                }
                let handle = this.transfers_popover.borrow().clone();
                if let Some(handle) = handle {
                    handle.show(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &V3Back, window, cx| {
                // GPUI resolves the Main Escape binding before the sheet's raw
                // key handler. A modal quit request must win over route Back.
                if this.coordinator.read(cx).quit_requested {
                    crate::controllers::lifecycle::cancel(cx);
                    this.coordinator.update(cx, |c, cx| {
                        c.quit_requested = false;
                        cx.notify();
                    });
                } else if !matches!(this.route, Route::Browse) {
                    this.back(window, cx);
                }
            }))
            .on_action(|_: &V3NextFocus, window, cx| window.focus_next(cx))
            .on_action(|_: &V3PreviousFocus, window, cx| window.focus_prev(cx))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let modifiers = event.keystroke.modifiers;
                if matches!(this.route, Route::Browse)
                    && !modifiers.control
                    && !modifiers.platform
                    && !modifiers.alt
                    && !this
                        .search
                        .read(cx)
                        .focus_handle(cx)
                        .contains_focused(window, cx)
                    && let Some(text) = event
                        .keystroke
                        .key_char
                        .as_ref()
                        .filter(|text| !text.chars().any(char::is_control))
                {
                    let query = format!("{}{text}", this.search.read(cx).text(cx));
                    this.search.update(cx, |s, cx| {
                        s.set_text(query, cx);
                        s.focus(window, cx);
                    });
                    cx.stop_propagation();
                }
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.background)
            .text_color(p.foreground)
            .text(TextSize::Base)
            .child(div().flex_1().min_h_0().child(body))
            .child(scroll_scrim(scrolled, p.background))
            .child({
                let notice = self.coordinator.read(cx).notice.clone();
                div()
                    .absolute()
                    .top(size::TOOLBAR_HEIGHT)
                    .left_0()
                    .right_0()
                    .p(s::S8)
                    .flex()
                    .flex_col()
                    .gap(s::S8)
                    .children(notice)
                    .children(personal_notice)
            })
            .child(toolbar)
            .when_some(self.menu.clone(), |el, menu| {
                el.child(
                    deferred(
                        anchored()
                            .anchor(Anchor::TopRight)
                            .position(point(
                                window.viewport_size().width - inset,
                                size::TOOLBAR_HEIGHT + s::S8,
                            ))
                            .child(menu),
                    )
                    .with_priority(10),
                )
            })
            .when(confirmation, |el| {
                el.child(super::quit::confirmation_for_close(
                    self.coordinator.clone(),
                    self.cancel_focus.clone(),
                    self.coordinator.read(cx).active_sessions(),
                    crate::controllers::lifecycle::is_close_fallback(cx),
                ))
            })
    }
}
