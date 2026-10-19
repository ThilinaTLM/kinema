//! V3 composition root: persistent Browse, route-local Detail, and shared sessions.
use super::{
    browse::{Browse, OpenTitle, Sort, menu_item},
    coordinator::{AppCoordinator, BrowseTitle, CoordinatorEvent},
    detail::{Detail, OpenDetailTitle},
};
use crate::widgets::{
    icon::IconName,
    icon_button::IconButton,
    menu::{Menu, Select, SelectEvent},
    popover::Popover,
    search_field::{SearchField, SearchFieldEvent},
    segmented::Segmented,
    text::TypeExt,
    text_input::TextInputEvent,
    tokens::{self, space as s, typeface as t},
};
use gpui::{prelude::*, *};
use kinema_core::MediaKind;

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
const SEARCH_MIN_WIDTH: Pixels = px(180.);

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
                if query != this.observed_query {
                    this.observed_query = query.clone();
                    if this.search.read(cx).text(cx) != query {
                        this.search.update(cx, |s, cx| s.set_text(query, cx));
                    }
                }
                cx.notify();
            }),
            cx.subscribe(&search, |this, search, _: &SearchFieldEvent, cx| {
                let query = search.read(cx).text(cx).to_owned();
                this.coordinator
                    .update(cx, |c, cx| c.load(c.kind, query, false, cx));
            }),
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

    fn open_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            Route::Browse => self
                .coordinator
                .update(cx, |c, cx| c.load(c.kind, c.query.clone(), true, cx)),
            Route::Detail(detail) => detail.update(cx, |d, cx| d.refresh(cx)),
        }
    }
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.detail_subscription = None;
        self.route = Route::Browse;
        crate::artwork::begin_generation("v3-detail", cx);
        self.coordinator
            .update(cx, |c, cx| c.cancel_title_requests(cx));
        self.browse.update(cx, |b, cx| b.restore_focus(window, cx));
        cx.notify();
    }
}
impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let kind = self.coordinator.read(cx).kind;
        let count = self.coordinator.read(cx).active_sessions();
        let change = cx.entity().downgrade();
        let in_detail = matches!(self.route, Route::Detail(_));
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
        let toolbar_title = match &self.route {
            Route::Browse => String::new(),
            Route::Detail(detail) => detail.read(cx).name().to_owned(),
        };
        let leading = if in_detail {
            div()
                .flex()
                .items_center()
                .gap(s::S8)
                .min_w_0()
                .child(
                    IconButton::new("detail-back", IconName::ArrowLeft, "Back")
                        .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                )
                .child(
                    div()
                        .type_style(t::HEADING)
                        .min_w_0()
                        .truncate()
                        .child(toolbar_title),
                )
                .into_any_element()
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
        // Left and right groups share the free space equally, so the search
        // field stays centered regardless of what each side holds.
        let toolbar = div()
            .h(s::TOOLBAR_HEIGHT)
            .flex_none()
            .flex()
            .items_center()
            .gap(s::S12)
            .px(s::S20)
            .bg(p.bg_surface)
            .border_b_1()
            .border_color(p.line_subtle)
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
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(s::S8)
                    .when(!in_detail, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(s::S6)
                                .child(
                                    div()
                                        .type_style(t::SMALL)
                                        .text_color(p.text_secondary)
                                        .child("Sort"),
                                )
                                .child(self.sort.clone()),
                        )
                    })
                    .child(
                        Popover::new("transfers")
                            .trigger(transfer_trigger)
                            .content(self.transfers.clone()),
                    )
                    .child(
                        IconButton::new("settings", IconName::Settings, "Settings").on_click({
                            let coordinator = self.coordinator.clone();
                            move |_, _, cx| coordinator.update(cx, |c, cx| c.open_settings(cx))
                        }),
                    )
                    .child(
                        IconButton::new("overflow", IconName::MoreHorizontal, "App menu")
                            .on_click(cx.listener(|this, _, window, cx| this.overflow(window, cx))),
                    ),
            );
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
                    this.back(window, cx);
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
            .child(toolbar)
            .children(
                self.coordinator
                    .read(cx)
                    .notice
                    .clone()
                    .map(|n| div().p(s::S8).child(n)),
            )
            .child(div().flex_1().min_h_0().child(body))
            .when_some(self.menu.clone(), |el, menu| {
                el.child(
                    deferred(
                        anchored()
                            .anchor(Anchor::TopRight)
                            .position(point(
                                window.viewport_size().width - s::S20,
                                s::TOOLBAR_HEIGHT,
                            ))
                            .child(menu),
                    )
                    .with_priority(10),
                )
            })
            .when(confirmation, |el| {
                el.child(super::quit::confirmation(
                    self.coordinator.clone(),
                    self.cancel_focus.clone(),
                ))
            })
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used)]
mod interaction_tests {
    use super::*;
    use crate::app::{AppEvent, AppOutcome};
    use ::core::prelude::v1::test;

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
        // First poster in Similar movies (32 px inset, 120×180), below
        // the 60%-height hero band and compact metadata/notice rows.
        visual.simulate_click(point(px(92.), px(1120.)), Modifiers::default());
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
        // Main is focused after replacement. With no IMDb mapping or paging
        // controls, reverse Tab reaches the last tabbable item: Similar's poster.
        cx.simulate_keystrokes(main.into(), "shift-tab enter");
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
}
