//! Temporary opt-in main window. Detail and settings destinations are explicit
//! placeholders until their owners integrate; no fake provider collections.
use super::{
    browse::{Browse, OpenTitle, Sort, menu_item},
    coordinator::{AppCoordinator, BrowseTitle},
};
use crate::widgets::{
    button::Button,
    icon::{Icon, IconName},
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
        V3Quit,
        V3NextFocus,
        V3PreviousFocus
    ]
);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-f", V3Search, Some("KinemaMain")),
        KeyBinding::new("f5", V3Refresh, Some("KinemaMain")),
        KeyBinding::new("secondary-r", V3Refresh, Some("KinemaMain")),
        KeyBinding::new("secondary-o", V3Local, Some("KinemaMain")),
        KeyBinding::new("alt-left", V3Back, Some("KinemaMain")),
        KeyBinding::new("escape", V3Back, Some("KinemaMain")),
        KeyBinding::new("backspace", V3Back, Some("KinemaMain")),
        KeyBinding::new("secondary-,", V3Settings, Some("KinemaMain")),
        KeyBinding::new("secondary-q", V3Quit, Some("KinemaMain")),
        KeyBinding::new("tab", V3NextFocus, Some("KinemaMain")),
        KeyBinding::new("shift-tab", V3PreviousFocus, Some("KinemaMain")),
    ]);
}
pub(crate) enum Route {
    Browse,
    Detail(BrowseTitle),
}
pub struct MainWindow {
    pub(crate) coordinator: Entity<AppCoordinator>,
    browse: Entity<Browse>,
    search: Entity<SearchField>,
    sort: Entity<Select<Sort>>,
    pub(crate) route: Route,
    menu: Option<Entity<Menu>>,
    menu_subscription: Option<Subscription>,
    chooser: Option<Task<()>>,
    cancel_focus: FocusHandle,
    focus: FocusHandle,
    observed_query: String,
    _subscriptions: Vec<Subscription>,
}
impl MainWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let coordinator = cx.new(AppCoordinator::new);
        Self::with_coordinator(coordinator, window, cx)
    }
    pub(crate) fn with_coordinator(
        coordinator: Entity<AppCoordinator>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let browse = cx.new(|cx| Browse::new(coordinator.clone(), cx));
        let search = cx.new(|cx| {
            let mut s = SearchField::new(window, cx);
            s.set_placeholder("Search movies…", cx);
            s
        });
        let sort = cx.new(|cx| Select::new(Sort::Provider, Sort::options(), cx));
        let editor = search.read(cx).input();
        let subscriptions = vec![
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
                this.coordinator
                    .update(cx, |c, cx| c.title_details(&event.0, false, cx));
                this.route = Route::Detail(event.0.clone());
                this.focus.focus(window, cx);
                cx.notify();
            }),
        ];
        let weak = coordinator.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |c, cx| c.request_quit(cx)).unwrap_or(true)
        });
        let focus = cx.focus_handle();
        browse.update(cx, |b, cx| b.restore_focus(window, cx));
        Self {
            focus,
            observed_query: String::new(),
            coordinator,
            browse,
            search,
            sort,
            route: Route::Browse,
            menu: None,
            menu_subscription: None,
            chooser: None,
            cancel_focus: cx.focus_handle().tab_stop(true),
            _subscriptions: subscriptions,
        }
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
        let refresh = self.coordinator.clone();
        let quit = self.coordinator.clone();
        let items = vec![
            menu_item("Open local file…", move |w, cx| {
                let _ = local.update(cx, |s, cx| s.open_local(w, cx));
            }),
            menu_item("Refresh", move |_, cx| {
                refresh.update(cx, |c, cx| c.load(c.kind, c.query.clone(), true, cx))
            }),
            menu_item("About Kinema", move |_, cx| {
                let _ = weak.update(cx, |s, cx| {
                    s.coordinator.update(cx, |c, cx| c.about(cx));
                });
            }),
            menu_item("Quit", move |_, cx| {
                let may_quit = quit.update(cx, |c, cx| c.request_quit(cx));
                if may_quit {
                    cx.quit();
                }
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
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.route = Route::Browse;
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
        let toolbar = div()
            .h(s::TOOLBAR_HEIGHT)
            .flex_none()
            .flex()
            .items_center()
            .gap(s::S12)
            .px(s::S20)
            .bg(p.bg_surface)
            .border_b_1()
            .border_color(p.line_hairline)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S6)
                    .child(Icon::new(IconName::Film))
                    .child("Kinema"),
            )
            .child(
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
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.))
                    .max_w(px(480.))
                    .child(self.search.clone()),
            )
            .child(self.sort.clone())
            .child(div().flex_1())
            .child(
                Popover::new("transfers")
                    .trigger(
                        IconButton::new("transfers-trigger", IconName::Transfers, "Transfers")
                            .badge(count),
                    )
                    .content(div().w(px(380.)).p(s::S16).child(if count == 0 {
                        "Nothing playing or downloading"
                    } else {
                        "Active sessions · transfer controls pending P2"
                    })),
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
            );
        let body = match &self.route {
            Route::Browse => self.browse.clone().into_any_element(),
            Route::Detail(title) => div()
                .flex_1()
                .p(s::S20)
                .flex()
                .flex_col()
                .gap(s::S16)
                .child(
                    Button::new("back", "Back")
                        .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                )
                .child(div().type_style(t::TITLE).child(title.name().to_owned()))
                .child("Detail integration pending P2")
                .into_any_element(),
        };
        let confirmation = self.coordinator.read(cx).quit_requested
            && self.coordinator.read(cx).needs_quit_confirmation();
        div()
            .id("v3-main")
            .track_focus(&self.focus)
            .key_context("KinemaMain")
            .on_action(cx.listener(|this, _: &V3Search,window,cx| { this.search.update(cx,|s,cx| s.focus(window,cx)); }))
            .on_action(cx.listener(|this, _: &V3Refresh,_,cx| { this.coordinator.update(cx,|c,cx| c.load(c.kind,c.query.clone(),true,cx)); }))
            .on_action(cx.listener(|this, _: &V3Local,window,cx| this.open_local(window,cx)))
            .on_action(cx.listener(|this, _: &V3Back,window,cx| { if !matches!(this.route,Route::Browse) { this.back(window,cx); } }))
            .on_action(cx.listener(|this, _: &V3Settings,_,cx| this.coordinator.update(cx,|c,cx| c.open_settings(cx))))
            .on_action(cx.listener(|this, _: &V3Quit,_,cx| { let may_quit = this.coordinator.update(cx,|c,cx| c.request_quit(cx)); if may_quit { cx.quit(); } }))
            .on_action(|_: &V3NextFocus,window,cx| window.focus_next(cx))
            .on_action(|_: &V3PreviousFocus,window,cx| window.focus_prev(cx))
            .on_key_down(cx.listener(|this,event: &KeyDownEvent,window,cx| {
                let modifiers = event.keystroke.modifiers;
                if matches!(this.route,Route::Browse) && !modifiers.control && !modifiers.platform && !modifiers.alt
                    && !this.search.read(cx).focus_handle(cx).contains_focused(window,cx)
                    && let Some(text) = event.keystroke.key_char.as_ref().filter(|text| !text.chars().any(char::is_control)) {
                    let query = format!("{}{text}",this.search.read(cx).text(cx));
                    this.search.update(cx,|s,cx| { s.set_text(query,cx); s.focus(window,cx); }); cx.stop_propagation();
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
                            .position(point(window.viewport_size().width-s::S20, s::TOOLBAR_HEIGHT))
                            .child(menu),
                    )
                    .with_priority(10),
                )
            })
            .when(confirmation, |el| {
                let cancel = self.coordinator.clone(); let dismiss = cancel.clone();
                el.child(crate::widgets::sheet::Sheet::new("quit-sheet","Quit Kinema?")
                    .body("Sessions are active or launches are pending. Quitting stops downloads and the local server; your player may lose media.")
                    .initial_focus(self.cancel_focus.clone())
                    .on_dismiss(move |_,cx| dismiss.update(cx,|c,cx| { c.quit_requested = false; cx.notify(); }))
                    .footer(Button::new("cancel-quit","Cancel").focus_handle(self.cancel_focus.clone()).on_click(move |_,_,cx| cancel.update(cx,|c,cx| { c.quit_requested = false; cx.notify(); })))
                    .footer(Button::new("confirm-quit","Quit").on_click(|_,_,cx| cx.quit())))
            })
    }
}
