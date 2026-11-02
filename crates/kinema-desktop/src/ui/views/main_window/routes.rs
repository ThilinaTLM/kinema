//! Route transitions, scope switching and personal-collection actions.
use super::*;

impl MainWindow {
    pub(super) fn open_title(
        &mut self,
        title: BrowseTitle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        titles: Vec<(BrowseTitle, crate::controllers::coordinator::PersonalTitle)>,
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
        self.search.update(cx, |s, cx| {
            s.set_placeholder(Scope::Library.search_placeholder(MediaKind::Movie), cx);
            s.set_text(search, cx);
        });
        cx.notify();
    }
    /// Switch Discover/Library; each scope keeps its own search text, filters,
    /// scroll and selection, so only the visible scope's state is shown.
    pub(super) fn switch_scope(
        &mut self,
        scope: Scope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.browse.read(cx).scope() == scope {
            return;
        }
        self.browse
            .update(cx, |b, cx| b.set_scope(scope, window, cx));
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
    pub(super) fn update_filters(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut LibraryFilters),
    ) {
        let (before, search) = {
            let browse = self.browse.read(cx);
            (browse.library_filters(), browse.library_search().to_owned())
        };
        let mut filters = before;
        change(&mut filters);
        if filters == before {
            return;
        }
        self.browse
            .update(cx, |b, cx| b.set_library(filters, &search, cx));
        cx.notify();
    }
    pub(super) fn clear_library(&mut self, cx: &mut Context<Self>) {
        self.browse.update(cx, |b, cx| b.clear_library_filters(cx));
        self.search.update(cx, |s, cx| s.set_text("", cx));
        cx.notify();
    }
    /// The one global personal-state toast: confirmation (with Undo after a
    /// removal) or the latest failure (with the coordinator's exact Retry).
    pub(super) fn personal_notice(&self, cx: &mut Context<Self>) -> Option<Alert> {
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
            (true, _) => AlertTone::Error,
            (_, true) => AlertTone::Info,
            _ => AlertTone::Success,
        };
        let label = if retry {
            "Retry"
        } else if undo {
            "Undo"
        } else {
            "Dismiss"
        };
        Some(Alert::new(kind, message).action(
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

    pub(super) fn open_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn overflow(&mut self, pointer: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &self.menu {
            menu.update(cx, |menu, cx| menu.dismiss(window, cx));
            return;
        }
        let weak = cx.entity().downgrade();
        let local = weak.clone();
        let refresh = cx.entity().downgrade();
        let quit = self.coordinator.clone();
        let items = vec![
            menu_item_icon("Open local file", IconName::Folder, move |w, cx| {
                let _ = local.update(cx, |s, cx| s.open_local(w, cx));
            }),
            menu_item_icon("Refresh", IconName::Refresh, move |_, cx| {
                let _ = refresh.update(cx, |s, cx| s.refresh(cx));
            }),
            menu_item_icon("About Kinema", IconName::Info, move |_, cx| {
                let _ = weak.update(cx, |s, cx| {
                    s.coordinator.update(cx, |c, cx| c.about(cx));
                });
            }),
            menu_item_icon("Quit", IconName::Power, move |window, cx| {
                crate::ui::views::quit::request(&quit, window, cx);
            }),
        ];
        let menu = cx.new(|cx| Menu::new(items, window, cx).opened_by_pointer(pointer));
        self.menu_subscription = Some(cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.menu = None;
            cx.notify();
        }));
        self.menu = Some(menu);
        cx.notify();
    }
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("UI back requested");
        if let Route::Detail(detail) = &self.route
            && detail.clone().update(cx, |d, cx| d.go_back(cx))
        {
            return;
        }
        self.leave_detail(window, cx);
    }
    pub(super) fn leave_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
