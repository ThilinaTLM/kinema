//! V3 composition root: persistent Browse, route-local Detail, and shared sessions.
use super::{
    browse::{
        Browse, BrowseRequest, LibraryFilters, OpenTitle, Scope, Sort,
        filters::{FiltersButton, FiltersChanged},
        menu_item,
    },
    coordinator::{AppCoordinator, BrowseTitle, CoordinatorEvent},
    detail::{Detail, OpenDetailTitle},
};
use crate::widgets::{
    button::Button,
    icon::IconName,
    icon_button::IconButton,
    menu::{Menu, Select, SelectEvent},
    notice::{Notice, NoticeKind},
    popover::Popover,
    search_field::{SearchField, SearchFieldEvent},
    segmented::Segmented,
    text::TypeExt,
    text_input::TextInputEvent,
    tokens::{self, radius, space as s, typeface as t},
};
use gpui::{prelude::*, *};
use kinema_core::{MediaKind, library::LibraryKind};
use std::{rc::Rc, time::Duration};

/// How long a non-actionable personal-state notice stays visible.
const NOTICE_TIMEOUT: Duration = Duration::from_secs(10);

actions!(
    v3_main,
    [
        V3Search,
        V3Refresh,
        V3Local,
        V3Back,
        V3Settings,
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
/// Preferred search width; it yields space to the side groups when narrow.
const SEARCH_WIDTH: Pixels = px(420.);
const SEARCH_MIN_WIDTH: Pixels = px(160.);

pub(crate) enum Route {
    Browse,
    Detail(Entity<super::detail::Detail>),
}
pub struct MainWindow {
    pub(crate) coordinator: Entity<AppCoordinator>,
    browse: Entity<Browse>,
    transfers: Entity<super::transfers::Transfers>,
    search: Entity<SearchField>,
    sort: Entity<Select<Sort>>,
    scope_select: Entity<Select<Scope>>,
    library_kind: Entity<Select<LibraryKind>>,
    filters_button: Entity<FiltersButton>,
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
        super::settings_window::install(&coordinator, cx);
        Self::with_coordinator(coordinator, window, cx)
    }
    pub(crate) fn with_coordinator(
        coordinator: Entity<AppCoordinator>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        super::quit::install(&coordinator, window.window_handle(), cx);
        let transfers = cx.new(|cx| super::transfers::Transfers::new(coordinator.clone(), cx));
        let browse = cx.new(|cx| Browse::new(coordinator.clone(), cx));
        let search = cx.new(|cx| {
            let mut s = SearchField::new(window, cx).shortcut_hint(SEARCH_HINT);
            s.set_placeholder("Search movies…", cx);
            s
        });
        let sort = cx.new(|cx| Select::new(Sort::Provider, Sort::options(), cx));
        let scope_select = cx.new(|cx| Select::new(Scope::Discover, Scope::options(), cx));
        let library_kind =
            cx.new(|cx| Select::new(LibraryKind::All, LibraryFilters::kind_options(), cx));
        let filters_button = cx.new(FiltersButton::new);
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
                            cx.background_executor().timer(NOTICE_TIMEOUT).await;
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
                &scope_select,
                window,
                |this, _, event: &SelectEvent<Scope>, window, cx| {
                    let SelectEvent::Changed(scope) = event;
                    this.switch_scope(*scope, window, cx);
                },
            ),
            cx.subscribe(
                &library_kind,
                |this, _, event: &SelectEvent<LibraryKind>, cx| {
                    let SelectEvent::Changed(kind) = event;
                    let kind = *kind;
                    this.update_filters(cx, |f| f.set_kind(kind));
                },
            ),
            cx.subscribe(&filters_button, |this, _, event: &FiltersChanged, cx| {
                let FiltersChanged(next) = *event;
                this.update_filters(cx, |f| {
                    f.status = next.status;
                    f.sort = next.sort;
                });
            }),
            cx.subscribe_in(
                &browse,
                window,
                |this, _, request: &BrowseRequest, window, cx| match request {
                    BrowseRequest::ShowDiscover => this.switch_scope(Scope::Discover, window, cx),
                    BrowseRequest::ClearLibraryFilters => this.clear_library(cx),
                },
            ),
            cx.subscribe(&sort, |this, _, event: &SelectEvent<Sort>, cx| {
                let SelectEvent::Changed(sort) = event;
                this.browse.update(cx, |b, cx| {
                    b.sort = *sort;
                    cx.notify();
                });
            }),
            cx.subscribe_in(&browse, window, |this, _, event: &OpenTitle, window, cx| {
                this.open_title(event.0.clone(), window, cx);
            }),
        ];
        let weak = coordinator.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            if let Some(coordinator) = weak.upgrade() {
                super::quit::request(&coordinator, window, cx);
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
            search,
            sort,
            scope_select,
            library_kind,
            filters_button,
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
    fn open_title(&mut self, title: BrowseTitle, window: &mut Window, cx: &mut Context<Self>) {
        title.log_navigation("detail");
        if matches!(self.route, Route::Browse) {
            self.browse
                .update(cx, |b, cx| b.remember_context(window, cx));
        }
        // Retire the old view/subscription before its shared resources change.
        // Browse stays mounted as an entity, preserving its selection and scroll.
        self.detail_subscription = None;
        self.route = Route::Browse;
        self.coordinator
            .update(cx, |c, cx| c.cancel_title_requests(cx));
        let coordinator = self.coordinator.clone();
        let detail = cx.new(|cx| Detail::new(coordinator, title, cx));
        self.detail_subscription = Some(cx.subscribe_in(
            &detail,
            window,
            |this, source, event: &OpenDetailTitle, window, cx| {
                if matches!(&this.route, Route::Detail(current) if current == source) {
                    this.open_title(event.0.clone(), window, cx);
                }
            },
        ));
        self.route = Route::Detail(detail);
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Native fixture seam for production Library captures: seeds saved results and
    /// personal state, selects Library and mirrors the toolbar, with no worker read.
    #[cfg(feature = "ui-probe")]
    pub(crate) fn show_library_fixture(
        &mut self,
        titles: Vec<(BrowseTitle, super::coordinator::PersonalTitle)>,
        filters: LibraryFilters,
        search: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.coordinator.update(cx, |c, cx| {
            let mut shown = Vec::new();
            for (title, personal) in titles {
                c.fixture_personal(&title, personal, Vec::new());
                shown.push(title);
            }
            let generation = c.library.begin(true);
            c.library_titles = shown.clone();
            c.library_has_more = false;
            c.library.fetched(generation, shown, 0);
            cx.notify();
        });
        self.browse.update(cx, |b, cx| {
            b.show_library_fixture(filters, search, window, cx)
        });
        self.scope_select
            .update(cx, |s, cx| s.set_value(Scope::Library, cx));
        self.library_kind
            .update(cx, |s, cx| s.set_value(filters.kind, cx));
        self.filters_button
            .update(cx, |button, cx| button.set_filters(filters, cx));
        self.search.update(cx, |s, cx| {
            s.set_placeholder(Scope::Library.search_placeholder(MediaKind::Movie), cx);
            s.set_text(search, cx);
        });
        cx.notify();
    }
    /// Switch Discover/Library; each scope keeps its own search text, filters,
    /// scroll and selection, so only the visible scope's state is shown.
    fn switch_scope(&mut self, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        if self.browse.read(cx).scope() == scope {
            return;
        }
        self.browse
            .update(cx, |b, cx| b.set_scope(scope, window, cx));
        self.scope_select.update(cx, |s, cx| s.set_value(scope, cx));
        let (text, placeholder) = match scope {
            Scope::Library => (
                self.browse.read(cx).library_search().to_owned(),
                scope.search_placeholder(MediaKind::Movie),
            ),
            Scope::Discover => {
                let c = self.coordinator.read(cx);
                (c.query.clone(), scope.search_placeholder(c.kind))
            }
        };
        self.search.update(cx, |s, cx| {
            s.set_placeholder(placeholder, cx);
            if s.text(cx) != text {
                s.set_text(text, cx);
            }
        });
        cx.notify();
    }
    fn update_filters(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut LibraryFilters)) {
        let (before, search) = {
            let browse = self.browse.read(cx);
            (browse.library_filters(), browse.library_search().to_owned())
        };
        let mut filters = before;
        change(&mut filters);
        if filters == before {
            return;
        }
        self.filters_button
            .update(cx, |button, cx| button.set_filters(filters, cx));
        self.browse
            .update(cx, |b, cx| b.set_library(filters, &search, cx));
        cx.notify();
    }
    fn clear_library(&mut self, cx: &mut Context<Self>) {
        self.library_kind
            .update(cx, |s, cx| s.set_value(LibraryKind::All, cx));
        self.browse.update(cx, |b, cx| b.clear_library_filters(cx));
        let filters = self.browse.read(cx).library_filters();
        self.filters_button
            .update(cx, |button, cx| button.set_filters(filters, cx));
        self.search.update(cx, |s, cx| s.set_text("", cx));
        cx.notify();
    }
    /// The one global personal-state toast: confirmation (with Undo after a
    /// removal) or the latest failure (with the coordinator's exact Retry).
    fn personal_notice(&self, cx: &mut Context<Self>) -> Option<Notice> {
        let (message, failed, retry, undo) = {
            let c = self.coordinator.read(cx);
            let failed = c.personal_notice_failed();
            (
                c.personal_notice.clone()?,
                failed,
                failed && c.can_retry_personal_write(),
                !failed && c.can_undo_membership(),
            )
        };
        let kind = match (failed, undo) {
            (true, _) => NoticeKind::Error,
            (_, true) => NoticeKind::Info,
            _ => NoticeKind::Success,
        };
        let label = if retry {
            "Retry"
        } else if undo {
            "Undo"
        } else {
            "Dismiss"
        };
        Some(Notice::new(kind, message).action(
            Button::new("personal-notice-action", label).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.coordinator.update(cx, |c, cx| {
                        let result = if retry {
                            c.retry_personal_write(cx)
                        } else if undo {
                            c.undo_membership(cx)
                        } else {
                            c.dismiss_personal_notice(cx);
                            Ok(())
                        };
                        if let Err(error) = result {
                            c.report_personal_failure(error, cx);
                        }
                    });
                },
            )),
        ))
    }

    fn open_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("UI local file chooser requested");
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open local file".into()),
        });
        let coordinator = self.coordinator.clone();
        self.chooser = Some(
            cx.spawn_in(window, async move |_, cx| match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = coordinator.update(cx, |c, cx| {
                            c.command(crate::app::AppAction::PlayLocal { path }, cx)
                        });
                    }
                }
                Ok(Ok(None)) => {}
                _ => {
                    coordinator.update(cx, |c, cx| {
                        tracing::warn!(
                            category = "file_chooser",
                            "The local file chooser could not be opened"
                        );
                        c.notice = Some("The local file chooser could not be opened.".into());
                        cx.notify();
                    });
                }
            }),
        );
    }

    fn overflow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &self.menu {
            menu.update(cx, |menu, cx| menu.dismiss(window, cx));
            return;
        }
        let weak = cx.entity().downgrade();
        let local = weak.clone();
        let refresh = cx.entity().downgrade();
        let quit = self.coordinator.clone();
        let items = vec![
            menu_item("Open local file…", move |w, cx| {
                let _ = local.update(cx, |s, cx| s.open_local(w, cx));
            }),
            menu_item("Refresh", move |_, cx| {
                let _ = refresh.update(cx, |s, cx| s.refresh(cx));
            }),
            menu_item("About Kinema", move |_, cx| {
                let _ = weak.update(cx, |s, cx| {
                    s.coordinator.update(cx, |c, cx| c.about(cx));
                });
            }),
            menu_item("Quit", move |window, cx| {
                super::quit::request(&quit, window, cx);
            }),
        ];
        let menu = cx.new(|cx| Menu::new(items, window, cx));
        self.menu_subscription = Some(cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.menu = None;
            cx.notify();
        }));
        self.menu = Some(menu);
        cx.notify();
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        match &self.route {
            Route::Browse if self.browse.read(cx).scope() == Scope::Library => {
                self.browse.update(cx, |b, cx| b.reload_library(cx))
            }
            Route::Browse => self
                .coordinator
                .update(cx, |c, cx| c.load(c.kind, c.query.clone(), true, cx)),
            Route::Detail(detail) => detail.update(cx, |d, cx| d.refresh(cx)),
        }
    }
    /// Back one step: Streams page -> detail page -> Browse.
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("UI back requested");
        if let Route::Detail(detail) = &self.route
            && detail.clone().update(cx, |d, cx| d.go_back(cx))
        {
            return;
        }
        self.leave_detail(window, cx);
    }
    fn leave_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!(route = "browse", "UI route changed");
        self.detail_subscription = None;
        self.route = Route::Browse;
        crate::artwork::begin_generation("v3-detail", cx);
        self.coordinator
            .update(cx, |c, cx| c.cancel_title_requests(cx));
        // Detail may have acquired metadata that changes derived watch status, sort
        // or titles; re-run the same local query (no remote request, loaded range
        // retained by the coordinator) before focus returns to the grid.
        if self.browse.read(cx).scope() == Scope::Library {
            self.browse.update(cx, |b, cx| b.reload_library(cx));
        }
        self.browse.update(cx, |b, cx| b.restore_focus(window, cx));
        cx.notify();
    }
}

/// Opacity of the control pills drawn over detail backdrops.
const PILL_ALPHA: f32 = 0.78;

/// Equal-size, centered cell for one toolbar icon control; grows only for a badge.
fn icon_cell(child: impl IntoElement) -> Div {
    div()
        .flex_none()
        .min_w(s::ICON_TARGET)
        .h(s::CONTROL_HEIGHT)
        .flex()
        .items_center()
        .justify_center()
        .child(child)
}

/// Scroll distance over which the scrim fades in.
const SCRIM_RAMP: Pixels = px(24.);

/// Top fade behind the floating header. Offset is read at paint time so it
/// tracks scrolling without re-rendering the window; it is invisible at scroll top.
fn scroll_scrim(scrolled: Rc<dyn Fn(&App) -> Pixels>, canvas_color: Hsla) -> impl IntoElement {
    let height = s::TOOLBAR_HEIGHT + s::S24;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            let strength = (scrolled(cx) / SCRIM_RAMP).clamp(0., 1.);
            if strength <= 0. {
                return;
            }
            // Stepped eased ramp: two-stop GPUI gradients leave visible seams
            // where alpha reaches zero, thin solid strips do not.
            let step = height / SCRIM_STEPS as f32;
            for i in 0..SCRIM_STEPS {
                let t = (i as f32 + 0.5) / SCRIM_STEPS as f32;
                let hold = SCRIM_HOLD;
                let falloff = if t <= hold {
                    1.
                } else {
                    let u = (t - hold) / (1. - hold);
                    1. - u * u * (3. - 2. * u)
                };
                let color = Hsla {
                    a: SCRIM_ALPHA * strength * falloff,
                    ..canvas_color
                };
                let strip = Bounds::new(
                    point(bounds.origin.x, bounds.origin.y + step * i as f32),
                    size(bounds.size.width, step + px(0.5)),
                );
                window.paint_quad(fill(strip, color));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .right_0()
    .h(height)
}
/// Peak opacity of the scrim at the window edge.
const SCRIM_ALPHA: f32 = 0.92;
/// Fraction of the scrim band (from the top) that stays at peak opacity.
const SCRIM_HOLD: f32 = 0.6;
const SCRIM_STEPS: usize = 36;

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let kind = self.coordinator.read(cx).kind;
        let scope = self.browse.read(cx).scope();
        let personal_notice = self.personal_notice(cx);
        let count = self.coordinator.read(cx).active_sessions();
        let change = cx.entity().downgrade();
        let in_detail = matches!(self.route, Route::Detail(_));
        // The popover wraps its trigger; a control-height flex box keeps the glyph
        // on the same centre line as its neighbours.
        let transfer_trigger = div().child(
            IconButton::new("transfers-trigger", IconName::Transfers, "Transfers").badge(count),
        );
        let pulse = crate::widgets::motion::duration(crate::widgets::motion::Motion::Feedback, cx);
        let transfer_trigger = if self.launch_pulse > 0 && !pulse.is_zero() {
            transfer_trigger
                .with_animation(
                    ("transfer-launch", self.launch_pulse),
                    Animation::new(pulse * 2),
                    |el, t| el.opacity(0.5 + 0.5 * t),
                )
                .into_any_element()
        } else {
            transfer_trigger.into_any_element()
        };
        // Light backdrops swallow bare ghost icons, so light browse pills too.
        let light = tokens::theme(cx).mode == tokens::Mode::Light;
        let pill_bg = Hsla {
            a: PILL_ALPHA,
            ..p.bg_overlay
        };
        // Compact translucent control group: every control sits in an
        // equal icon-target cell, centered on both axes.
        let pill = move |el: Div| {
            el.flex_none()
                .flex()
                .items_center()
                .gap(s::S2)
                .rounded(radius::SM)
                .bg(pill_bg)
                .border_1()
                .border_color(p.line_subtle)
                .p(s::S2)
        };
        let actions = vec![
            icon_cell(
                Popover::new("transfers")
                    .trigger(transfer_trigger)
                    .content(self.transfers.clone()),
            )
            // Popover's wrapper reports a taller box than its glyph; top-aligning
            // puts the glyph on the same centre line as its neighbours.
            .items_start(),
            icon_cell(
                IconButton::new("settings", IconName::Settings, "Settings").on_click({
                    let coordinator = self.coordinator.clone();
                    move |_, _, cx| coordinator.update(cx, |c, cx| c.open_settings(cx))
                }),
            ),
            icon_cell(
                IconButton::new("overflow", IconName::MoreHorizontal, "App menu")
                    .on_click(cx.listener(|this, _, window, cx| this.overflow(window, cx))),
            ),
        ];
        let mut actions = Some(actions);
        let back_label = match &self.route {
            Route::Detail(detail) if detail.read(cx).on_streams_page() => "Back to details",
            _ => "Back to browse",
        };
        let leading = if in_detail {
            // One left-aligned control group: back | divider | app actions.
            pill(div())
                .child(icon_cell(
                    IconButton::new("detail-back", IconName::ArrowLeft, back_label)
                        .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                ))
                .child(
                    div()
                        .flex_none()
                        .w(px(1.))
                        .h(s::S16)
                        .mx(s::S4)
                        .bg(p.line_hairline),
                )
                .children(actions.take().unwrap_or_default())
                .into_any_element()
        } else {
            let kind_control = if scope == Scope::Library {
                self.library_kind.clone().into_any_element()
            } else {
                Segmented::new(
                    "kind",
                    [
                        (MediaKind::Movie, "Movies".into()),
                        (MediaKind::Series, "Series".into()),
                    ],
                    kind,
                )
                .on_change(move |kind, _, cx| {
                    let _ = change.update(cx, |this, cx| {
                        let query = this.search.read(cx).text(cx).to_owned();
                        this.coordinator
                            .update(cx, |c, cx| c.load(*kind, query, false, cx));
                        this.search.update(cx, |s, cx| {
                            s.set_placeholder(
                                if *kind == MediaKind::Movie {
                                    "Search movies…"
                                } else {
                                    "Search series…"
                                },
                                cx,
                            )
                        });
                    });
                })
                .into_any_element()
            };
            div()
                .flex()
                .items_center()
                .gap(s::S8)
                .child(self.scope_select.clone())
                .child(kind_control)
                .into_any_element()
        };
        // Left and right groups share the free space equally, so the search
        // field stays centered regardless of what each side holds.
        // Transparent overlay: no bar surface or border. The controls float over
        // the page; `scroll_scrim` provides legibility once content scrolls under.
        let toolbar = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(s::TOOLBAR_HEIGHT)
            .flex()
            .items_center()
            .gap(s::S12)
            .px(if in_detail {
                super::detail::page_inset(window.viewport_size().width)
            } else {
                s::S20
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(leading),
            )
            .when(!in_detail, |el| {
                el.child(
                    div()
                        .w(SEARCH_WIDTH)
                        .min_w(SEARCH_MIN_WIDTH)
                        .child(self.search.clone()),
                )
            })
            .when(!in_detail, |el| {
                el.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(s::S8)
                        .child(if scope == Scope::Library {
                            div()
                                .flex()
                                .items_center()
                                .gap(s::S6)
                                .child(self.filters_button.clone())
                                .into_any_element()
                        } else {
                            div()
                                .flex()
                                .items_center()
                                .gap(s::S6)
                                .child(
                                    div()
                                        .type_style(t::SMALL)
                                        .text_color(p.text_primary)
                                        .child("Sort"),
                                )
                                .child(self.sort.clone())
                                .into_any_element()
                        })
                        .child(if light {
                            pill(div())
                                .children(actions.take().unwrap_or_default())
                                .into_any_element()
                        } else {
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(s::S8)
                                .children(actions.take().unwrap_or_default())
                                .into_any_element()
                        }),
                )
            });
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
            .on_action(cx.listener(|this, _: &V3Back, window, cx| {
                // GPUI resolves the Main Escape binding before the sheet's raw
                // key handler. A modal quit request must win over route Back.
                if this.coordinator.read(cx).quit_requested {
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
            .bg(p.bg_canvas)
            .text_color(p.text_primary)
            .type_style(t::BODY)
            .child(div().flex_1().min_h_0().child(body))
            .child(scroll_scrim(scrolled, p.bg_canvas))
            .child({
                let notice = self.coordinator.read(cx).notice.clone();
                div()
                    .absolute()
                    .top(s::TOOLBAR_HEIGHT)
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
                            .anchor(if in_detail {
                                Anchor::TopLeft
                            } else {
                                Anchor::TopRight
                            })
                            .position(point(
                                if in_detail {
                                    s::S20
                                } else {
                                    window.viewport_size().width - s::S20
                                },
                                s::TOOLBAR_HEIGHT,
                            ))
                            .child(menu),
                    )
                    .with_priority(10),
                )
            })
            .when(confirmation, |el| {
                el.child(super::quit::confirmation_for(
                    self.coordinator.clone(),
                    self.cancel_focus.clone(),
                    count,
                ))
            })
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod interaction_tests {
    use super::*;
    use crate::app::{AppEvent, AppOutcome};
    use ::core::prelude::v1::test;
    use std::time::Duration;

    fn movie(id: u64, name: &str) -> crate::tmdb::TmdbTitle {
        crate::tmdb::TmdbTitle {
            id,
            kind: MediaKind::Movie,
            name: name.into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        }
    }

    fn mounted_detail(main: WindowHandle<MainWindow>, cx: &mut TestAppContext) -> Entity<Detail> {
        main.read_with(cx, |view, _| match &view.route {
            Route::Detail(detail) => detail.clone(),
            Route::Browse => panic!("expected Detail route"),
        })
        .unwrap()
    }

    #[gpui::test]
    fn similar_pointer_and_keyboard_replace_detail_and_back_restores_browse(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            tokens::init(cx);
            tokens::set_reduce_motion(true, cx);
            bind_keys(cx);
        });
        let first = movie(1, "Movie one");
        let second = movie(2, "Movie two");
        let third = movie(3, "Movie three");
        let coordinator = cx.new(|cx| {
            let mut c = AppCoordinator::fixture(vec![BrowseTitle::Tmdb(first.clone())], None, cx);
            for (id, titles) in [(1, vec![second.clone()]), (2, vec![third]), (3, vec![])] {
                c.fixture_similar.insert(
                    (id, 1),
                    crate::tmdb::TmdbPage {
                        titles,
                        page: 1,
                        total_pages: 1,
                    },
                );
            }
            c.load(MediaKind::Movie, "Movie".into(), false, cx);
            c
        });
        let main = cx
            .add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
        let browse = main
            .update(cx, |view, _, cx| {
                view.browse.update(cx, |b, cx| {
                    b.sort = Sort::Year;
                    cx.notify();
                });
                view.browse.clone()
            })
            .unwrap();
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        // A tall fixed viewport exposes the production recommendation row below
        // the hero, without scrolling or relying on a test-only rendering path.
        visual.simulate_resize(size(px(1280.), px(1400.)));
        cx.run_until_parked();
        cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        visual.simulate_click(point(px(100.), px(150.)), Modifiers::default());
        cx.run_until_parked();
        let initial = mounted_detail(main, cx);
        assert_eq!(
            initial.read_with(cx, |d, _| d.name().to_owned()),
            first.name
        );
        let retired = initial.downgrade();
        drop(initial);
        let obsolete = coordinator.update(cx, |c, cx| {
            // Simulate outstanding title/season/source/recommendation requests.
            // Keep the recommendation value visible while its refresh is pending.
            let generations = [
                c.details.begin(false),
                c.tmdb_details.begin(false),
                c.tmdb_season.begin(false),
                c.sources.begin(false),
                c.similar.begin(false),
            ];
            cx.notify();
            generations
        });
        cx.run_until_parked();
        cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        // Locate the Similar poster from its laid-out bounds, so the test does
        // not depend on the hero height or carousel placement. Detail must tag
        // each tile with `.debug_selector(|| format!("similar-title-{id}"))`.
        let bounds = visual.debug_bounds("similar-title-2").unwrap_or_else(|| {
            panic!("Detail's Similar tile needs debug_selector `similar-title-<id>`")
        });
        visual.simulate_click(bounds.center(), Modifiers::default());
        cx.run_until_parked();
        let current = mounted_detail(main, cx);
        assert_eq!(
            current.read_with(cx, |d, _| d.name().to_owned()),
            second.name
        );
        assert!(retired.upgrade().is_none());
        coordinator.update(cx, |c, _| {
            assert!(!c.details.failed(obsolete[0], "late title"));
            assert!(!c.tmdb_details.failed(obsolete[1], "late TMDB title"));
            assert!(!c.tmdb_season.failed(obsolete[2], "late season"));
            assert!(!c.sources.failed(obsolete[3], "late sources"));
            assert!(!c.similar.failed(obsolete[4], "late recommendations"));
        });
        let retired = current.downgrade();
        drop(current);
        // Keyboard path without assuming the tile is the last tab stop (Detail
        // may keep a Streams Retry stop after it). Mouse-down alone focuses the
        // tile without activating it; shift-tab then tab must come back to the
        // same stop, and only then does Enter open the recommendation.
        cx.run_until_parked();
        cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let tile = visual.debug_bounds("similar-title-3").unwrap_or_else(|| {
            panic!("Detail's Similar tile needs debug_selector `similar-title-<id>`")
        });
        visual.simulate_mouse_down(tile.center(), MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        assert_eq!(
            mounted_detail(main, cx).read_with(cx, |d, _| d.name().to_owned()),
            "Movie two",
            "mouse-down must focus, not open"
        );
        cx.simulate_keystrokes(main.into(), "shift-tab tab enter");
        cx.run_until_parked();
        let current = mounted_detail(main, cx);
        assert_eq!(
            current.read_with(cx, |d, _| d.name().to_owned()),
            "Movie three"
        );
        assert!(retired.upgrade().is_none());
        drop(current);
        cx.simulate_keystrokes(main.into(), "alt-left");
        cx.run_until_parked();
        main.update(cx, |view, _, cx| {
            assert!(matches!(view.route, Route::Browse));
            assert_eq!(view.browse, browse);
            assert_eq!(view.browse.read(cx).sort, Sort::Year);
            assert_eq!(view.coordinator.read(cx).query, "Movie");
            assert_eq!(view.coordinator.read(cx).kind, MediaKind::Movie);
            assert!(view.detail_subscription.is_none());
        })
        .unwrap();
        // The original Browse poster, not the last Similar result, regains focus.
        cx.simulate_keystrokes(main.into(), "enter");
        assert_eq!(
            mounted_detail(main, cx).read_with(cx, |d, _| d.name().to_owned()),
            "Movie one"
        );
    }

    #[gpui::test]
    fn successful_initial_launch_and_relaunch_pulse_but_failure_does_not(cx: &mut TestAppContext) {
        cx.update(tokens::init);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        let main = cx
            .add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
        for (outcome, expected) in [
            (
                AppOutcome::PlaybackStarted {
                    session: crate::media::PlaybackSession {
                        id: "offline-session".into(),
                        url: "http://127.0.0.1:1234/offline".into(),
                    },
                    file_name: "fixture.mkv".into(),
                },
                1,
            ),
            (
                AppOutcome::SessionRelaunched {
                    session_id: "offline-session".into(),
                },
                2,
            ),
            (
                AppOutcome::Failed {
                    message: "Launch failed".into(),
                },
                2,
            ),
        ] {
            coordinator.update(cx, |_, cx| {
                cx.emit(CoordinatorEvent::App(AppEvent {
                    generation: 1,
                    outcome,
                }));
            });
            cx.run_until_parked();
            main.update(cx, |view, _, _| assert_eq!(view.launch_pulse, expected))
                .unwrap();
        }
    }

    fn draw(main: WindowHandle<MainWindow>, cx: &mut TestAppContext) {
        cx.run_until_parked();
        cx.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }

    fn fixture_window(
        cx: &mut TestAppContext,
    ) -> (Entity<AppCoordinator>, WindowHandle<MainWindow>) {
        cx.update(|cx| {
            tokens::init(cx);
            tokens::set_reduce_motion(true, cx);
            bind_keys(cx);
        });
        let coordinator = cx.new(|cx| {
            let mut c = AppCoordinator::fixture(
                vec![BrowseTitle::Tmdb(movie(1, "Provider movie"))],
                None,
                cx,
            );
            c.load(MediaKind::Movie, "Provider".into(), false, cx);
            c
        });
        let main = cx
            .add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
        draw(main, cx);
        (coordinator, main)
    }

    /// Replace the Library result list as a completed local read would.
    fn set_library(coordinator: &Entity<AppCoordinator>, ids: &[u64], cx: &mut TestAppContext) {
        coordinator.update(cx, |c, cx| {
            let generation = c.library.begin(true);
            c.library_titles = ids
                .iter()
                .map(|id| BrowseTitle::Tmdb(movie(*id, &format!("Saved {id}"))))
                .collect();
            c.library_has_more = false;
            c.library.fetched(generation, c.library_titles.clone(), 0);
            cx.notify();
        });
    }

    fn type_in_search(main: WindowHandle<MainWindow>, text: &str, cx: &mut TestAppContext) {
        main.update(cx, |view, _, cx| {
            view.search.update(cx, |s, cx| s.set_text(text, cx))
        })
        .unwrap();
        cx.run_until_parked();
    }

    #[gpui::test]
    fn library_scope_reads_local_results_and_restores_each_scopes_context(cx: &mut TestAppContext) {
        let (coordinator, main) = fixture_window(cx);
        let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
        assert_eq!(browse.read_with(cx, |b, _| b.card_keys()), ["Movie:tmdb:1"]);
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        set_library(&coordinator, &[7, 8], cx);
        draw(main, cx);
        // Library shows its own rows, never the provider grid filtered.
        assert_eq!(
            browse.read_with(cx, |b, _| b.card_keys()),
            ["Movie:tmdb:7", "Movie:tmdb:8"]
        );
        main.read_with(cx, |v, cx| {
            assert_eq!(v.search.read(cx).text(cx), "");
        })
        .unwrap();
        type_in_search(main, "saved", cx);
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        browse.read_with(cx, |b, _| assert_eq!(b.library_search(), "saved"));
        coordinator.read_with(cx, |c, _| assert_eq!(c.query, "Provider"));
        // Back in Discover the provider query is restored, not Library text.
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Discover, window, cx)
        })
        .unwrap();
        draw(main, cx);
        main.read_with(cx, |v, cx| {
            assert_eq!(v.search.read(cx).text(cx), "Provider")
        })
        .unwrap();
        assert_eq!(browse.read_with(cx, |b, _| b.card_keys()), ["Movie:tmdb:1"]);
    }

    #[gpui::test]
    fn stale_discover_search_cannot_leak_into_library_or_provider(cx: &mut TestAppContext) {
        let (coordinator, main) = fixture_window(cx);
        let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        type_in_search(main, "dune", cx);
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Discover, window, cx)
        })
        .unwrap();
        // A Discover edit is still inside its debounce window when the user
        // flips back to Library.
        type_in_search(main, "Other", cx);
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        main.read_with(cx, |v, cx| assert_eq!(v.search.read(cx).text(cx), "dune"))
            .unwrap();
        browse.read_with(cx, |b, _| assert_eq!(b.library_search(), "dune"));
        coordinator.read_with(cx, |c, _| {
            assert_eq!(c.query, "Provider", "no provider fetch")
        });
        // Rapid back-and-forth settles on the provider's own query.
        for scope in [Scope::Discover, Scope::Library, Scope::Discover] {
            main.update(cx, |view, window, cx| view.switch_scope(scope, window, cx))
                .unwrap();
        }
        cx.background_executor
            .advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        main.read_with(cx, |v, cx| {
            assert_eq!(v.search.read(cx).text(cx), "Provider")
        })
        .unwrap();
        browse.read_with(cx, |b, _| assert_eq!(b.library_search(), "dune"));
        coordinator.read_with(cx, |c, _| assert_eq!(c.query, "Provider"));
    }

    #[gpui::test]
    fn membership_toggle_reports_failure_without_opening_detail(cx: &mut TestAppContext) {
        use crate::widgets::poster_card::MembershipRequested;
        let (coordinator, main) = fixture_window(cx);
        let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
        let card = browse.read_with(cx, |b, _| b.card_for_test(0));
        // The fixture has no catalog worker, so the write is rejected up front:
        // the failure is surfaced and nothing navigates.
        card.update(cx, |_, cx| cx.emit(MembershipRequested(true)));
        draw(main, cx);
        coordinator.read_with(cx, |c, _| assert!(c.personal_notice.is_some()));
        main.read_with(cx, |v, _| assert!(matches!(v.route, Route::Browse)))
            .unwrap();
        // An unacknowledged state is never shown as unsaved: still loading/failed.
        assert!(!matches!(
            card.read_with(cx, |card, _| card.membership),
            crate::widgets::state_toggle::ToggleStatus::Known { on: false, .. }
        ));
    }

    #[gpui::test]
    fn filtered_out_focus_moves_to_next_then_previous_card(cx: &mut TestAppContext) {
        let (coordinator, main) = fixture_window(cx);
        let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        set_library(&coordinator, &[1, 2, 3], cx);
        draw(main, cx);
        let focused = |cx: &mut TestAppContext| {
            main.update(cx, |_, window, cx| browse.read(cx).focused_key(window, cx))
                .unwrap()
        };
        for (focus, remaining, expected) in [
            (1usize, vec![1u64, 3], "Movie:tmdb:3"),
            (2, vec![1, 2], "Movie:tmdb:2"),
        ] {
            set_library(&coordinator, &[1, 2, 3], cx);
            draw(main, cx);
            let card = browse.read_with(cx, |b, _| b.card_for_test(focus));
            main.update(cx, |_, window, cx| {
                card.update(cx, |c, cx| c.focus(window, cx))
            })
            .unwrap();
            draw(main, cx);
            assert!(focused(cx).is_some());
            set_library(&coordinator, &remaining, cx);
            draw(main, cx);
            assert_eq!(focused(cx).as_deref(), Some(expected));
        }
    }

    #[gpui::test]
    fn one_filters_control_is_text_when_wide_and_icon_only_at_960(cx: &mut TestAppContext) {
        let (_, main) = fixture_window(cx);
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        let mut widths = Vec::new();
        for width in [1280., 960.] {
            visual.simulate_resize(size(px(width), px(800.)));
            draw(main, cx);
            let bounds = visual
                .debug_bounds("library-filters")
                .expect("Library shows exactly one Filters control");
            // Single 48px toolbar row, fully inside the window.
            assert!(
                bounds.top() >= px(0.) && bounds.bottom() <= s::TOOLBAR_HEIGHT,
                "{bounds:?}"
            );
            assert!(bounds.right() <= px(width), "{bounds:?}");
            widths.push(bounds.size.width);
        }
        assert!(widths[0] > widths[1] + px(20.), "{widths:?}");
        // Discover has no Filters control.
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Discover, window, cx)
        })
        .unwrap();
        draw(main, cx);
        assert!(visual.debug_bounds("library-filters").is_none());
    }

    #[gpui::test]
    fn library_requests_the_next_local_page_only_when_more_exist(cx: &mut TestAppContext) {
        let (coordinator, main) = fixture_window(cx);
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        // Establish the active query (the fixture has no worker, so the read fails).
        coordinator.update(cx, |c, cx| {
            c.load_library(Default::default(), cx);
        });
        let ids: Vec<u64> = (1..=14).collect();
        set_library(&coordinator, &ids, cx);
        let seeded = coordinator.read_with(cx, |c, _| c.library.generation());
        draw(main, cx);
        coordinator.read_with(cx, |c, _| {
            assert_eq!(c.library.generation(), seeded, "no more pages: no request");
        });
        coordinator.update(cx, |c, cx| {
            c.library_has_more = true;
            cx.notify();
        });
        draw(main, cx);
        cx.run_until_parked();
        coordinator.read_with(cx, |c, _| {
            assert!(
                c.library.generation() > seeded,
                "a visible short list pages"
            );
            assert_eq!(c.library_titles.len(), 14, "loaded cards stay on failure");
        });
    }

    #[gpui::test]
    fn only_confirmations_expire_and_failures_persist(cx: &mut TestAppContext) {
        let (coordinator, _main) = fixture_window(cx);
        for failure in [false, true] {
            coordinator.update(cx, |c, cx| {
                if failure {
                    c.report_personal_failure("Could not update “Dune”: busy.".into(), cx);
                } else {
                    c.personal_notice = Some("Updated “Dune”.".into());
                    cx.notify();
                }
            });
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(Duration::from_secs(60));
            cx.run_until_parked();
            coordinator.read_with(cx, |c, _| {
                assert_eq!(c.personal_notice.is_none(), !failure);
            });
        }
        // Dismiss is the explicit way to clear a failure.
        coordinator.update(cx, |c, cx| c.dismiss_personal_notice(cx));
        coordinator.read_with(cx, |c, _| assert!(c.personal_notice.is_none()));
    }

    fn local_entry(id: i64, added_at: Option<i64>) -> crate::catalog::library::LibraryEntry {
        use kinema_core::{
            catalog_normalized::{CanonicalTitle, ExternalIdentifier, FieldValue},
            library::TitleId,
        };
        let mut fields = std::collections::BTreeMap::new();
        fields.insert("name".to_owned(), FieldValue::Text(format!("Saved {id}")));
        crate::catalog::library::LibraryEntry {
            metadata: CanonicalTitle {
                id,
                kind: MediaKind::Movie,
                identifiers: vec![ExternalIdentifier {
                    namespace: "imdb".into(),
                    external_id: format!("tt{id:07}"),
                }],
                fields,
                provenance: vec![],
                ratings: vec![],
                updated_at: 0,
            },
            personal: crate::catalog::library::PersonalTitle {
                title_id: TitleId::new(id).unwrap(),
                kind: MediaKind::Movie,
                added_at,
                movie_watch: None,
                show_summary: None,
            },
        }
    }

    /// Apply committed results the way worker events arrive: first the mutation
    /// acknowledgement, then the refreshed first Library page.
    fn commit(
        coordinator: &Entity<AppCoordinator>,
        revision: u64,
        mutation: Option<(i64, bool, Option<i64>)>,
        page: Vec<crate::catalog::library::LibraryEntry>,
        cx: &mut TestAppContext,
    ) {
        use crate::{
            catalog::library::{LibraryOperation, LibraryPage, LibraryResult, TitleRef},
            catalog_worker::local::{LocalEvent, LocalRequest},
        };
        use kinema_core::library::{LibraryQuery, TitleId};
        coordinator.update(cx, |c, cx| {
            if let Some((id, saved, previous_added_at)) = mutation {
                let personal = local_entry(id, saved.then_some(20)).personal;
                c.fixture_apply_personal_event(
                    LocalEvent {
                        request: LocalRequest {
                            request_id: revision * 10,
                            generation: 0,
                            operation: LibraryOperation::SetMembership {
                                title: TitleRef::Canonical(TitleId::new(id).unwrap()),
                                saved,
                                restore_added_at: None,
                            },
                        },
                        revision,
                        result: Ok(LibraryResult::Membership {
                            personal,
                            previous_added_at,
                        }),
                    },
                    cx,
                );
            }
            let generation = c.library.begin(false);
            let total = page.len();
            c.fixture_apply_personal_event(
                LocalEvent {
                    request: LocalRequest {
                        request_id: revision * 10 + 1,
                        generation,
                        operation: LibraryOperation::List(LibraryQuery {
                            limit: 60,
                            ..Default::default()
                        }),
                    },
                    revision,
                    result: Ok(LibraryResult::Page(LibraryPage {
                        entries: page,
                        total,
                    })),
                },
                cx,
            );
        });
    }

    #[gpui::test]
    fn committed_remove_then_undo_restores_card_order_and_keeps_focus_by_title(
        cx: &mut TestAppContext,
    ) {
        use crate::widgets::state_toggle::ToggleStatus;
        let (coordinator, main) = fixture_window(cx);
        let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        let all = || {
            vec![
                local_entry(1, Some(30)),
                local_entry(2, Some(20)),
                local_entry(3, Some(10)),
            ]
        };
        commit(&coordinator, 1, None, all(), cx);
        draw(main, cx);
        let keys = |cx: &mut TestAppContext| browse.read_with(cx, |b, _| b.card_keys());
        assert_eq!(keys(cx), ["canonical:1", "canonical:2", "canonical:3"]);
        let focused = |cx: &mut TestAppContext| {
            main.update(cx, |_, window, cx| browse.read(cx).focused_key(window, cx))
                .unwrap()
        };
        // Focus the middle card, then commit its removal and the refreshed page.
        let card = browse.read_with(cx, |b, _| b.card_for_test(1));
        main.update(cx, |_, window, cx| {
            card.update(cx, |c, cx| c.focus(window, cx))
        })
        .unwrap();
        draw(main, cx);
        assert_eq!(focused(cx).as_deref(), Some("canonical:2"));
        commit(
            &coordinator,
            2,
            Some((2, false, Some(20))),
            vec![local_entry(1, Some(30)), local_entry(3, Some(10))],
            cx,
        );
        draw(main, cx);
        assert_eq!(keys(cx), ["canonical:1", "canonical:3"]);
        assert_eq!(focused(cx).as_deref(), Some("canonical:3"), "next card");
        coordinator.read_with(cx, |c, _| assert!(c.personal_notice.is_some()));
        // Undo: the committed re-add and refreshed page restore the card at its
        // original sort position; focus stays on the title it was already on.
        commit(&coordinator, 3, Some((2, true, None)), all(), cx);
        draw(main, cx);
        assert_eq!(keys(cx), ["canonical:1", "canonical:2", "canonical:3"]);
        assert_eq!(focused(cx).as_deref(), Some("canonical:3"));
        let restored = browse.read_with(cx, |b, _| b.card_for_test(1));
        assert!(matches!(
            restored.read_with(cx, |c, _| c.membership),
            ToggleStatus::Known { on: true, .. }
        ));
    }

    #[gpui::test]
    fn back_to_library_reruns_the_same_local_query_and_keeps_cards(cx: &mut TestAppContext) {
        let (coordinator, main) = fixture_window(cx);
        let browse = main.read_with(cx, |v, _| v.browse.clone()).unwrap();
        main.update(cx, |view, window, cx| {
            view.switch_scope(Scope::Library, window, cx)
        })
        .unwrap();
        coordinator.update(cx, |c, cx| c.load_library(Default::default(), cx));
        set_library(&coordinator, &[1, 2], cx);
        draw(main, cx);
        let before = coordinator.read_with(cx, |c, _| c.library.generation());
        main.update(cx, |view, window, cx| {
            view.open_title(BrowseTitle::Tmdb(movie(1, "Saved 1")), window, cx)
        })
        .unwrap();
        cx.run_until_parked();
        main.update(cx, |view, window, cx| view.leave_detail(window, cx))
            .unwrap();
        draw(main, cx);
        coordinator.read_with(cx, |c, _| {
            assert!(c.library.generation() > before, "same query re-run on Back");
        });
        browse.read_with(cx, |b, _| {
            assert_eq!(b.scope(), Scope::Library);
            assert_eq!(b.library_filters(), LibraryFilters::default());
            assert_eq!(b.card_keys().len(), 2, "loaded cards survive the refresh");
        });
    }

    #[gpui::test]
    fn back_pops_streams_to_the_detail_page_then_to_browse(cx: &mut TestAppContext) {
        use super::super::detail::Page;
        cx.update(|cx| {
            tokens::init(cx);
            tokens::set_reduce_motion(true, cx);
            bind_keys(cx);
        });
        let mut title = movie(1, "Movie one");
        title.imdb = kinema_core::catalog::ImdbId::new("tt1234567").ok();
        let coordinator = cx.new(|cx| {
            let mut c = AppCoordinator::fixture(vec![BrowseTitle::Tmdb(title.clone())], None, cx);
            c.fixture_similar.insert(
                (1, 1),
                crate::tmdb::TmdbPage {
                    titles: vec![],
                    page: 1,
                    total_pages: 1,
                },
            );
            c
        });
        let main = cx
            .add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        main.update(cx, |view, window, cx| {
            view.open_title(BrowseTitle::Tmdb(title.clone()), window, cx)
        })
        .unwrap();
        cx.run_until_parked();
        let detail = mounted_detail(main, cx);
        detail.update(cx, |d, cx| d.run_primary_for_test(cx));
        cx.run_until_parked();
        assert_eq!(detail.read_with(cx, |d, _| d.page()), Page::Streams);
        visual.dispatch_action(V3Back);
        cx.run_until_parked();
        // Same Detail entity, now on its overview page: state intact.
        assert_eq!(detail.read_with(cx, |d, _| d.page()), Page::Overview);
        assert!(matches!(
            main.read_with(
                cx,
                |v, _| matches!(&v.route, Route::Detail(d) if *d == detail)
            ),
            Ok(true)
        ));
        visual.dispatch_action(V3Back);
        cx.run_until_parked();
        main.read_with(cx, |v, _| assert!(matches!(v.route, Route::Browse)))
            .unwrap();
    }
}
