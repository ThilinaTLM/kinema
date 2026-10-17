use std::{path::PathBuf, time::Duration};

use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Selectable as _, Sizable as _, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    list::{List, ListEvent, ListState},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use kinema_core::{
    MediaKind,
    catalog::CatalogTitle,
    sources::{IndexerKind, PlaybackTarget},
};

use crate::{
    app::{AppAction, AppCommand, AppEvent, AppOutcome, AppWorker},
    app_state::Resource,
    app_tracker::{Pending, Tracker},
    catalog_state::ScreenState,
    catalog_worker::MetadataEvent,
    catalog_worker::{CatalogWorker, Poll, Request, Submit},
    credentials::SecretString,
    metadata_bridge::{self, Ask, MetaEvent, Polled},
    playback_view::{PlaybackIntent, PlaybackView},
    settings::MetadataProvider,
    settings_view::{SettingsIntent, SettingsView},
    tmdb_view::{TmdbIntent, TmdbView},
    ui::{
        catalog_view::{self, GridMetrics, GridState, Nav, TitleDelegate, ViewMode},
        detail_view::{self, DetailPane},
        episodes::EpisodeDelegate,
        source_picker::{self, SourcesPane},
        sources::{SortKey, SourceDelegate, SourceOptions},
        strip::{StripModel, status_strip},
    },
};

actions!(kinema, [FocusSearch, GoBack]);

use kinema_desktop::design::tokens::metrics::{
    RAIL_WIDTH, TOOLBAR_HEIGHT, WORKSPACE_GAP, WORKSPACE_PADDING as WORKSPACE_PAD,
};

fn grid_metrics(window: &Window, items: usize) -> GridMetrics {
    let width = f32::from(window.viewport_size().width);
    GridMetrics::new(width - RAIL_WIDTH - 2.0 * WORKSPACE_PAD, items)
}

pub(crate) fn bind_keys(cx: &mut App) {
    let find = if cfg!(target_os = "macos") {
        "cmd-f"
    } else {
        "ctrl-f"
    };
    cx.bind_keys([
        KeyBinding::new(find, FocusSearch, Some("Shell")),
        KeyBinding::new("escape", GoBack, Some("Shell")),
        KeyBinding::new("alt-left", GoBack, Some("Shell")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Catalog(MediaKind),
    Transfers,
    Settings,
    Help,
}

impl Default for Page {
    fn default() -> Self {
        Self::Catalog(MediaKind::Movie)
    }
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Catalog(MediaKind::Movie) => "Movies",
            Self::Catalog(MediaKind::Series) => "Series",
            Self::Transfers => "Transfers",
            Self::Settings => "Settings",
            Self::Help => "About this scaffold",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Catalog(MediaKind::Movie) => {
                "Popular titles load from Cinemeta when you open this page; Search looks up a title."
            }
            Self::Catalog(MediaKind::Series) => {
                "Popular titles load from Cinemeta when you open this page; Search looks up a title."
            }
            Self::Transfers => {
                "Prepare a source, choose a file, then open it in your external player."
            }
            Self::Settings => {
                "Provider, player and cache-limit preferences, and debrid credentials."
            }
            Self::Help => {
                "Kinema Next is a pre-1.0 desktop scaffold, not the complete MVP. Catalog browsing, details, episodes and source listing use a local saved-results cache. Playback needs your own debrid account: sources are prepared through it, cached under the configured limit, served on loopback only and opened in external VLC or mpv. Real providers and players are untested here."
            }
        }
    }
}

/// What the platform file chooser returned.
enum Chosen {
    Path(Option<PathBuf>),
    Cancelled,
    Failed,
}

fn submit_meta(
    worker: &Result<CatalogWorker, String>,
    ask: Ask,
    generation: u64,
    refresh: bool,
) -> Result<(), String> {
    match worker {
        Ok(worker) => metadata_bridge::request(worker, ask, generation, refresh),
        Err(message) => Err(message.clone()),
    }
}

/// Forwards a cancel for work that is still pending; a settled resource has
/// nothing queued, so nothing is sent.
fn cancel_meta(worker: &Result<CatalogWorker, String>, busy: bool, ask: Ask, generation: u64) {
    if let (true, Ok(worker)) = (busy, worker) {
        metadata_bridge::cancel(worker, ask, generation);
    }
}

fn index(kind: MediaKind) -> usize {
    match kind {
        MediaKind::Movie => 0,
        MediaKind::Series => 1,
    }
}

/// Independent per-kind catalog state: search, detail and source navigation.
struct BrowsePage {
    offset: u32,
    res: Resource<Vec<CatalogTitle>>,
}

/// Cancel before invalidating generations so the worker receives the original
/// request identity. Keep loaded values for navigation back to this screen.
fn cancel_browse(pages: &mut [BrowsePage], mut forward: impl FnMut(u32, u64)) {
    for page in pages {
        if page.res.busy() {
            forward(page.offset, page.res.generation());
            page.res.cancel();
        }
    }
}

/// Forward the original identity before cancellation. Resume markers belong to
/// this exact resource generation, never a later retry or replacement.
fn suspend_resource<T>(
    resource: &mut Resource<T>,
    resume: &mut Option<u64>,
    forward: impl FnOnce(u64),
) {
    if resource.busy() {
        forward(resource.generation());
        resource.cancel();
        *resume = resource.value().is_none().then_some(resource.generation());
    }
}

fn take_resource_resume<T>(resource: &Resource<T>, resume: &mut Option<u64>) -> bool {
    resume.take().is_some_and(|generation| {
        generation == resource.generation() && !resource.busy() && resource.value().is_none()
    })
}

struct CatalogPane {
    screen: ScreenState,
    detail_resume: Option<u64>,
    sources_resume: Option<u64>,
    /// Popular titles, one resource per loaded page; shown when no search is active.
    browse: Vec<BrowsePage>,
    input: Entity<InputState>,
    list: Entity<ListState<TitleDelegate>>,
    grid: GridState,
    detail: Option<DetailPane>,
    _subscriptions: Vec<Subscription>,
}

pub(crate) struct Shell {
    page: Page,
    /// Movies/series filter inside the Browse destination.
    browse_kind: MediaKind,
    view_mode: ViewMode,
    /// Last rendered grid geometry, shared with the virtualized rows.
    grid_metrics: GridMetrics,
    restore_grid_focus: bool,
    next_base: u64,
    panes: [CatalogPane; 2],
    /// `Err` holds why the worker thread could not start.
    worker: Result<CatalogWorker, String>,
    app: Result<AppWorker, String>,
    tracker: Tracker,
    settings_view: Entity<SettingsView>,
    playback_view: Entity<PlaybackView>,
    tmdb: Entity<TmdbView>,
    /// Persisted provider choice; catalog pages wait for the first settings snapshot.
    metadata: MetadataProvider,
    settings_loaded: bool,
    tmdb_token_present: bool,
    /// Owned file-chooser wait; replacing it cancels the previous one.
    chooser: Option<Task<()>>,
    /// Page-level outcome shown above the content: `(text, is_error)`.
    notice: Option<(String, bool)>,
    quit_confirmed: bool,
    app_stop_reported: bool,
    _app_subscriptions: Vec<Subscription>,
    focus: FocusHandle,
    _poll: Task<()>,
}

impl Shell {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let panes = [MediaKind::Movie, MediaKind::Series].map(|kind| {
            let placeholder = match kind {
                MediaKind::Movie => "Search movies",
                MediaKind::Series => "Search series",
            };
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            let list = cx.new(|cx| ListState::new(TitleDelegate::new(), window, cx));
            let subscriptions = vec![
                cx.subscribe_in(&input, window, move |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.search(kind, cx);
                    }
                }),
                cx.subscribe_in(
                    &list,
                    window,
                    move |this, list, event: &ListEvent, window, cx| {
                        if let ListEvent::Confirm(ix) = event {
                            let title = list.read(cx).delegate().title(*ix).cloned();
                            if let Some(title) = title {
                                this.open_title(kind, title, window, cx);
                            }
                        }
                    },
                ),
            ];
            CatalogPane {
                screen: ScreenState::new(kind),
                browse: Vec::new(),
                detail_resume: None,
                sources_resume: None,
                input,
                list,
                grid: GridState::new(kind, cx),
                detail: None,
                _subscriptions: subscriptions,
            }
        });
        // Thread start only; the catalog service is opened on the worker.
        let worker = CatalogWorker::spawn_default()
            .map_err(|error| format!("The catalog worker could not start: {error}"));
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| this.drain(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let settings_view = SettingsView::new(window, cx);
        let playback_view = PlaybackView::new(window, cx);
        let tmdb = TmdbView::new(MediaKind::Movie, window, cx);
        let app_subscriptions = vec![
            cx.subscribe_in(
                &settings_view,
                window,
                |this, _, intent: &SettingsIntent, _, cx| this.on_settings_intent(intent, cx),
            ),
            cx.subscribe_in(
                &playback_view,
                window,
                |this, _, intent: &PlaybackIntent, _, cx| this.on_playback_intent(intent, cx),
            ),
            cx.subscribe_in(&tmdb, window, |this, _, intent: &TmdbIntent, window, cx| {
                this.on_tmdb_intent(intent, window, cx);
            }),
        ];
        // Thread start only; settings, vault and media open on the worker.
        let app = AppWorker::spawn()
            .map_err(|error| format!("The application worker could not start: {error}"));
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| this.may_close(window, cx))
                .unwrap_or(true)
        });
        let mut shell = Self {
            page: Page::default(),
            browse_kind: MediaKind::Movie,
            view_mode: ViewMode::default(),
            grid_metrics: GridMetrics::new(900.0, 0),
            restore_grid_focus: false,
            next_base: 0,
            panes,
            worker,
            app,
            tracker: Tracker::default(),
            settings_view,
            playback_view,
            tmdb,
            metadata: MetadataProvider::default(),
            settings_loaded: false,
            tmdb_token_present: false,
            chooser: None,
            notice: None,
            quit_confirmed: false,
            app_stop_reported: false,
            _app_subscriptions: app_subscriptions,
            focus,
            _poll: poll,
        };
        shell.send(Pending::Load, AppAction::LoadSettings, cx);
        shell
    }

    fn pane(&self, kind: MediaKind) -> &CatalogPane {
        &self.panes[index(kind)]
    }

    fn pane_mut(&mut self, kind: MediaKind) -> &mut CatalogPane {
        &mut self.panes[index(kind)]
    }

    fn drain(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut disconnected = false;
        let mut meta = Vec::new();
        if let Ok(worker) = &self.worker {
            loop {
                match worker.poll() {
                    Poll::Event(event) => {
                        changed |= self.panes[index(event.kind)].screen.apply(event);
                    }
                    Poll::Empty => break,
                    Poll::Disconnected => {
                        disconnected = true;
                        break;
                    }
                }
            }
            meta.extend(std::iter::from_fn(|| metadata_bridge::poll(worker)));
        }
        for event in meta {
            match event {
                Polled::Cinemeta(event) => changed |= self.apply_meta(event, cx),
                Polled::Tmdb(event) => {
                    self.tmdb
                        .update(cx, |view, cx| view.handle_event(event, cx));
                    changed = true;
                }
            }
        }
        let mut app_events = Vec::new();
        let mut app_stopped = false;
        if let Ok(app) = &self.app {
            app_events.extend(std::iter::from_fn(|| app.poll()));
            app_stopped = app.is_stopped();
        }
        if app_stopped && !self.app_stop_reported {
            self.app_stop_reported = true;
            let message = "The application service stopped unexpectedly. Restart Kinema.";
            self.settings_view.update(cx, |view, cx| {
                view.set_feedback(message.to_owned(), true, cx)
            });
            self.playback_view
                .update(cx, |view, cx| view.show_error(message.to_owned(), cx));
            changed = true;
        }
        for event in app_events {
            changed |= self.apply_app(event, window, cx);
        }
        if disconnected {
            changed |= self.fail_all("The catalog worker stopped unexpectedly.");
        }
        if changed {
            self.sync_titles(cx);
            cx.notify();
        }
    }

    fn fail_all(&mut self, message: &str) -> bool {
        let mut changed = false;
        for pane in &mut self.panes {
            if pane.screen.busy() {
                pane.screen.fail_current(message);
                changed = true;
            }
            for page in &mut pane.browse {
                let generation = page.res.generation();
                changed |= page.res.failed(generation, message);
            }
            if let Some(detail) = &mut pane.detail {
                let generation = detail.details.generation();
                changed |= detail.details.failed(generation, message);
                if let Some(sources) = &mut detail.sources {
                    let generation = sources.sources.generation();
                    changed |= sources.sources.failed(generation, message);
                }
            }
        }
        changed
    }

    /// Pushes search or browse rows into the virtualized lists.
    fn sync_titles(&mut self, cx: &mut Context<Self>) {
        for pane in &mut self.panes {
            let screen = &pane.screen;
            let (mut titles, empty): (Vec<CatalogTitle>, Option<SharedString>) =
                if screen.active().is_some() {
                    let empty = (!screen.busy() && screen.fetched_at.is_some())
                        .then(|| SharedString::from("No titles found."));
                    (screen.titles.clone(), empty)
                } else {
                    let titles: Vec<CatalogTitle> = pane
                        .browse
                        .iter()
                        .filter_map(|page| page.res.value())
                        .flatten()
                        .cloned()
                        .collect();
                    let settled = !pane.browse.is_empty()
                        && pane
                            .browse
                            .iter()
                            .all(|p| !p.res.busy() && p.res.value().is_some());
                    let empty = (settled && titles.is_empty())
                        .then(|| SharedString::from("The catalog returned no titles."));
                    (titles, empty)
                };
            // Adjacent provider pages can overlap; one mounted element per identity.
            let mut seen = std::collections::HashSet::new();
            titles.retain(|title| seen.insert((title.kind(), title.id().clone())));
            let grid_changed = pane.grid.set(&titles, empty.clone(), cx);
            let changed = pane
                .list
                .update(cx, |list, _| list.delegate_mut().set(&titles, empty));
            if changed || grid_changed {
                pane.list.update(cx, |_, cx| cx.notify());
            }
        }
    }

    // ---- browse -------------------------------------------------------

    fn ensure_browse(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        if self.pane(kind).detail.is_some() || self.pane(kind).screen.active().is_some() {
            return;
        }
        if self.pane(kind).browse.is_empty() {
            self.load_browse_page(kind, 0, false, cx);
        } else {
            // A first load cancelled by navigation has no value to restore.
            let missing: Vec<_> = self
                .pane(kind)
                .browse
                .iter()
                .filter(|page| !page.res.busy() && page.res.value().is_none())
                .map(|page| page.offset)
                .collect();
            for offset in missing {
                self.load_browse_page(kind, offset, false, cx);
            }
        }
    }

    fn cancel_browse(&mut self, kind: MediaKind) {
        let worker = &self.worker;
        cancel_browse(&mut self.panes[index(kind)].browse, |offset, generation| {
            cancel_meta(worker, true, Ask::Browse { kind, offset }, generation);
        });
    }

    fn load_browse_page(
        &mut self,
        kind: MediaKind,
        offset: u32,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let base = self.next_base();
        let worker = &self.worker;
        let pane = &mut self.panes[index(kind)];
        let position = match pane.browse.iter().position(|p| p.offset == offset) {
            Some(position) => position,
            None => {
                pane.browse.push(BrowsePage {
                    offset,
                    res: Resource::with_base(base),
                });
                pane.browse.len() - 1
            }
        };
        let page = &mut pane.browse[position];
        let generation = page.res.begin(false);
        if let Err(message) = submit_meta(worker, Ask::Browse { kind, offset }, generation, refresh)
        {
            page.res.failed(generation, message);
        }
        self.sync_titles(cx);
        cx.notify();
    }

    /// Refresh keeps only the first page so paging restarts from fresh data.
    fn refresh_browse(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        self.cancel_browse(kind);
        self.pane_mut(kind).grid.invalidate_artwork(cx);
        self.pane_mut(kind).browse.truncate(1);
        self.load_browse_page(kind, 0, true, cx);
    }

    fn load_more(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let pane = self.pane(kind);
        if pane.browse.iter().any(|p| p.res.busy()) {
            return;
        }
        let loaded: usize = pane
            .browse
            .iter()
            .filter_map(|p| p.res.value())
            .map(Vec::len)
            .sum();
        let Ok(offset) = u32::try_from(loaded) else {
            return;
        };
        self.load_browse_page(kind, offset, false, cx);
    }

    fn suspend_search(&mut self, kind: MediaKind) {
        if let Some(generation) = self.pane(kind).screen.pending_generation() {
            if let Ok(worker) = &self.worker {
                worker.cancel_search(kind, generation);
            }
            self.pane_mut(kind).screen.suspend();
        }
    }

    fn resume_search(&mut self, kind: MediaKind) {
        if self.pane(kind).detail.is_none() {
            let request = self.pane_mut(kind).screen.resume();
            self.dispatch(kind, request);
        }
    }

    fn clear_search(&mut self, kind: MediaKind, window: &mut Window, cx: &mut Context<Self>) {
        self.suspend_search(kind);
        self.pane_mut(kind).grid.invalidate_artwork(cx);
        self.pane_mut(kind).screen.clear();
        self.pane(kind)
            .input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.ensure_browse(kind, cx);
        self.sync_titles(cx);
        cx.notify();
    }

    fn apply_meta(&mut self, event: MetaEvent, cx: &mut Context<Self>) -> bool {
        match event {
            MetaEvent::Browse {
                kind,
                offset,
                generation,
                stage,
            } => self
                .pane_mut(kind)
                .browse
                .iter_mut()
                .find(|page| page.offset == offset)
                .is_some_and(|page| page.res.apply(generation, stage)),
            MetaEvent::Details {
                kind,
                generation,
                stage,
            } => {
                let Some(detail) = self.pane_mut(kind).detail.as_mut() else {
                    return false;
                };
                let applied = detail.details.apply(generation, stage);
                if applied {
                    detail.sync_episodes(cx);
                }
                applied
            }
            MetaEvent::Sources {
                kind,
                generation,
                stage,
            } => {
                let Some(sources) = self
                    .pane_mut(kind)
                    .detail
                    .as_mut()
                    .and_then(|d| d.sources.as_mut())
                else {
                    return false;
                };
                let applied = sources.sources.apply(generation, stage);
                if applied {
                    sources.sync(cx);
                }
                applied
            }
        }
    }

    // ---- navigation and metadata provider ---------------------------------

    /// Switches page and starts whichever metadata source the page needs.
    fn enter_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if let Page::Catalog(kind) = self.page {
            self.cancel_browse(kind);
            self.suspend_search(kind);
            self.pane_mut(kind).grid.invalidate_artwork(cx);
        }
        self.page = page;
        if let Page::Catalog(kind) = page {
            self.browse_kind = kind;
        }
        match page {
            Page::Catalog(kind) if self.settings_loaded => match self.metadata {
                MetadataProvider::Cinemeta => {
                    self.tmdb.update(cx, |view, cx| view.deactivate(cx));
                    self.ensure_browse(kind, cx);
                    self.resume_search(kind);
                    self.resume_retained_detail(kind, cx);
                }
                MetadataProvider::Tmdb if !self.tmdb_token_present => {
                    // No silent fallback to Cinemeta: send the user to setup.
                    self.tmdb.update(cx, |view, cx| view.deactivate(cx));
                    self.page = Page::Settings;
                    self.notice = Some((
                        "TMDB is the selected metadata provider but has no saved access token. Add one under Metadata credentials, or choose Cinemeta."
                            .to_owned(),
                        true,
                    ));
                }
                MetadataProvider::Tmdb => {
                    self.tmdb
                        .update(cx, |view, cx| view.activate(kind, window, cx));
                }
            },
            Page::Catalog(_) => {}
            _ => self.tmdb.update(cx, |view, cx| view.deactivate(cx)),
        }
        cx.notify();
    }

    fn on_tmdb_intent(&mut self, intent: &TmdbIntent, window: &mut Window, cx: &mut Context<Self>) {
        match intent {
            TmdbIntent::Request(request) => {
                let refusal = match &self.worker {
                    Ok(worker) => (worker.submit_metadata(request.clone()) == Submit::Closed)
                        .then(|| "The catalog worker is not running.".to_owned()),
                    Err(message) => Some(message.clone()),
                };
                if let Some(message) = refusal {
                    // Answer as a failed result so the view leaves its loading state.
                    let event = MetadataEvent {
                        generation: request.generation,
                        action: request.action.clone(),
                        result: Err(message),
                        saved: false,
                        refreshing: false,
                    };
                    self.tmdb
                        .update(cx, |view, cx| view.handle_event(event, cx));
                }
            }
            TmdbIntent::Cancel { action, generation } => {
                if let Ok(worker) = &self.worker {
                    worker.cancel_metadata(action, *generation);
                }
            }
            TmdbIntent::FindSources { target } => {
                self.open_sources_for_target(target.clone(), window, cx);
            }
        }
    }

    /// Opens the source picker for a target that came from another metadata
    /// provider (an IMDb ID). The picker needs no Cinemeta details fetch.
    fn open_sources_for_target(
        &mut self,
        target: PlaybackTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = target.kind();
        let id = target.imdb_id().clone();
        let Ok(title) = CatalogTitle::new(id.clone(), kind, id.as_str().to_owned(), None) else {
            return;
        };
        let mut detail = self.build_detail(kind, title, window, cx);
        detail.synthetic = true;
        if let PlaybackTarget::Episode {
            season, episode, ..
        } = &target
        {
            detail.selected = Some((*season, *episode));
        }
        self.cancel_browse(kind);
        self.suspend_search(kind);
        self.cancel_replaced_detail(kind);
        self.pane_mut(kind).detail = Some(detail);
        self.open_sources(kind, window, cx);
    }

    // ---- search -------------------------------------------------------

    fn search(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let text = self.pane(kind).input.read(cx).value();
        let request = self.pane_mut(kind).screen.search(&text);
        if request.is_some() {
            self.cancel_browse(kind);
            self.pane_mut(kind).grid.invalidate_artwork(cx);
        }
        self.dispatch(kind, request);
        self.sync_titles(cx);
        cx.notify();
    }

    fn refresh_search(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        self.pane_mut(kind).grid.invalidate_artwork(cx);
        let request = self.pane_mut(kind).screen.refresh();
        self.dispatch(kind, request);
        self.sync_titles(cx);
        cx.notify();
    }

    fn dispatch(&mut self, kind: MediaKind, request: Option<Request>) {
        let Some(request) = request else { return };
        let failure = match &self.worker {
            Ok(worker) if worker.submit(request) != Submit::Closed => return,
            Ok(_) => "The catalog worker is not running.".to_owned(),
            Err(message) => message.clone(),
        };
        self.pane_mut(kind).screen.fail_current(&failure);
    }

    // ---- detail -------------------------------------------------------

    fn open_title(
        &mut self,
        kind: MediaKind,
        title: CatalogTitle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_browse(kind);
        self.suspend_search(kind);
        self.pane_mut(kind).grid.invalidate_artwork(cx);
        let detail = self.build_detail(kind, title, window, cx);
        self.cancel_replaced_detail(kind);
        self.pane_mut(kind).detail = Some(detail);
        self.request_details(kind, false, true, cx);
        cx.notify();
    }

    /// Provider changes obsolete pending work even in retained hidden panes.
    fn suspend_retained_detail(&mut self, kind: MediaKind) {
        let pane = &mut self.panes[index(kind)];
        let Some(detail) = pane.detail.as_mut() else {
            return;
        };
        let ask = Ask::Details {
            kind,
            id: detail.title.id().clone(),
        };
        suspend_resource(&mut detail.details, &mut pane.detail_resume, |generation| {
            cancel_meta(&self.worker, true, ask, generation);
        });
        if let Some(sources) = detail.sources.as_mut() {
            let ask = Ask::Sources {
                indexer: sources.indexer,
                target: sources.target.clone(),
            };
            suspend_resource(
                &mut sources.sources,
                &mut pane.sources_resume,
                |generation| {
                    cancel_meta(&self.worker, true, ask, generation);
                },
            );
        }
    }

    fn resume_retained_detail(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let pane = &mut self.panes[index(kind)];
        let Some(detail) = pane.detail.as_ref() else {
            return;
        };
        let details =
            take_resource_resume(&detail.details, &mut pane.detail_resume) && !detail.synthetic;
        let sources = detail.sources.as_ref().is_some_and(|sources| {
            take_resource_resume(&sources.sources, &mut pane.sources_resume)
        });
        if details {
            self.request_details(kind, false, false, cx);
        }
        if sources {
            self.request_sources(kind, false, false, cx);
        }
    }

    fn cancel_replaced_detail(&mut self, kind: MediaKind) {
        let Some(detail) = self.panes[index(kind)].detail.as_mut() else {
            return;
        };
        cancel_meta(
            &self.worker,
            detail.details.busy(),
            Ask::Details {
                kind,
                id: detail.title.id().clone(),
            },
            detail.details.generation(),
        );
        detail.details.cancel();
        if let Some(sources) = detail.sources.as_mut() {
            cancel_meta(
                &self.worker,
                sources.sources.busy(),
                Ask::Sources {
                    indexer: sources.indexer,
                    target: sources.target.clone(),
                },
                sources.sources.generation(),
            );
            sources.sources.cancel();
        }
    }

    fn build_detail(
        &mut self,
        kind: MediaKind,
        title: CatalogTitle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DetailPane {
        let episodes = cx.new(|cx| ListState::new(EpisodeDelegate::new(), window, cx));
        let subscription = cx.subscribe_in(
            &episodes,
            window,
            move |this, list, event: &ListEvent, window, cx| match event {
                ListEvent::Select(ix) => {
                    let picked = list
                        .read(cx)
                        .delegate()
                        .episode(*ix)
                        .map(|e| (e.season, e.number));
                    this.select_episode(kind, picked, cx);
                }
                ListEvent::Confirm(ix) => {
                    let picked = list
                        .read(cx)
                        .delegate()
                        .episode(*ix)
                        .map(|e| (e.season, e.number));
                    this.select_episode(kind, picked, cx);
                    this.open_sources(kind, window, cx);
                }
                ListEvent::Cancel => {}
            },
        );
        let base = self.next_base();
        DetailPane {
            title,
            details: Resource::with_base(base),
            season: None,
            episodes,
            selected: None,
            sources: None,
            notice: None,
            synthetic: false,
            _subscriptions: vec![subscription],
        }
    }

    fn next_base(&mut self) -> u64 {
        self.next_base += 1 << 20;
        self.next_base
    }

    fn request_details(
        &mut self,
        kind: MediaKind,
        refresh: bool,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        let worker = &self.worker;
        let Some(detail) = self.panes[index(kind)].detail.as_mut() else {
            return;
        };
        let generation = detail.details.begin(replace);
        let ask = Ask::Details {
            kind,
            id: detail.title.id().clone(),
        };
        if let Err(message) = submit_meta(worker, ask, generation, refresh) {
            detail.details.failed(generation, message);
        }
        detail.sync_episodes(cx);
    }

    pub(crate) fn refresh_details(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        self.request_details(kind, true, false, cx);
        cx.notify();
    }

    /// Back: leaves the source picker first, then the detail pane. Late results
    /// for the closed view are rejected by generation.
    pub(crate) fn close_top(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let worker = &self.worker;
        let pane = &mut self.panes[index(kind)];
        if let Some(detail) = &mut pane.detail {
            let close_sources = |detail: &mut DetailPane| {
                if let Some(mut sources) = detail.sources.take() {
                    let ask = Ask::Sources {
                        indexer: sources.indexer,
                        target: sources.target.clone(),
                    };
                    cancel_meta(
                        worker,
                        sources.sources.busy(),
                        ask,
                        sources.sources.generation(),
                    );
                    sources.sources.cancel();
                    true
                } else {
                    false
                }
            };
            let closed_sources = close_sources(detail);
            if detail.synthetic || !closed_sources {
                let ask = Ask::Details {
                    kind,
                    id: detail.title.id().clone(),
                };
                cancel_meta(
                    worker,
                    detail.details.busy(),
                    ask,
                    detail.details.generation(),
                );
                detail.details.cancel();
                pane.detail = None;
            }
        }
        if self.pane(kind).detail.is_none() {
            self.restore_grid_focus = true;
            self.ensure_browse(kind, cx);
            self.resume_search(kind);
        }
        cx.notify();
    }

    pub(crate) fn select_season(&mut self, kind: MediaKind, season: u32, cx: &mut Context<Self>) {
        if let Some(detail) = self.pane_mut(kind).detail.as_mut() {
            detail.season = Some(season);
            detail.selected = None;
            detail.sync_episodes(cx);
        }
        cx.notify();
    }

    fn select_episode(
        &mut self,
        kind: MediaKind,
        picked: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) {
        if let Some(detail) = self.pane_mut(kind).detail.as_mut() {
            detail.selected = picked;
        }
        cx.notify();
    }

    // ---- sources ------------------------------------------------------

    pub(crate) fn open_sources(
        &mut self,
        kind: MediaKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base = self.next_base();
        let Some(detail) = self.panes[index(kind)].detail.as_mut() else {
            return;
        };
        let (target, label) = match (kind, detail.selected) {
            (MediaKind::Movie, _) => (
                PlaybackTarget::Movie(detail.title.id().clone()),
                detail.title.name().to_owned(),
            ),
            (MediaKind::Series, Some((season, episode))) => (
                PlaybackTarget::Episode {
                    series: detail.title.id().clone(),
                    season,
                    episode,
                },
                format!("{} S{season:02}E{episode:02}", detail.title.name()),
            ),
            (MediaKind::Series, None) => return,
        };
        let list = cx.new(|cx| ListState::new(SourceDelegate::new(), window, cx));
        let subscription = cx.subscribe_in(&list, window, move |_, _, _: &ListEvent, _, cx| {
            // Selection only; the explicit "Use selected source" button acts.
            cx.notify();
        });
        detail.notice = None;
        detail.sources = Some(SourcesPane {
            target,
            label,
            indexer: IndexerKind::Torrentio,
            sources: Resource::with_base(base),
            options: SourceOptions::default(),
            list,
            _subscription: subscription,
        });
        self.request_sources(kind, false, true, cx);
        cx.notify();
    }

    fn request_sources(
        &mut self,
        kind: MediaKind,
        refresh: bool,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        let worker = &self.worker;
        let Some(pane) = self.panes[index(kind)]
            .detail
            .as_mut()
            .and_then(|d| d.sources.as_mut())
        else {
            return;
        };
        let generation = pane.sources.begin(replace);
        let ask = Ask::Sources {
            indexer: pane.indexer,
            target: pane.target.clone(),
        };
        if let Err(message) = submit_meta(worker, ask, generation, refresh) {
            pane.sources.failed(generation, message);
        }
        pane.sync(cx);
    }

    fn sources_mut(&mut self, kind: MediaKind) -> Option<&mut SourcesPane> {
        self.pane_mut(kind)
            .detail
            .as_mut()
            .and_then(|d| d.sources.as_mut())
    }

    pub(crate) fn set_indexer(
        &mut self,
        kind: MediaKind,
        indexer: IndexerKind,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.panes[index(kind)]
            .detail
            .as_mut()
            .and_then(|d| d.sources.as_mut())
        else {
            return;
        };
        if pane.indexer == indexer {
            return;
        }
        let old = Ask::Sources {
            indexer: pane.indexer,
            target: pane.target.clone(),
        };
        cancel_meta(
            &self.worker,
            pane.sources.busy(),
            old,
            pane.sources.generation(),
        );
        pane.indexer = indexer;
        pane.options.quality = None;
        self.request_sources(kind, false, true, cx);
        cx.notify();
    }

    pub(crate) fn refresh_sources(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        self.request_sources(kind, true, false, cx);
        cx.notify();
    }

    pub(crate) fn sort_sources(&mut self, kind: MediaKind, key: SortKey, cx: &mut Context<Self>) {
        if let Some(pane) = self.sources_mut(kind) {
            pane.options.select_sort(key);
            pane.sync(cx);
        }
        cx.notify();
    }

    pub(crate) fn filter_quality(
        &mut self,
        kind: MediaKind,
        quality: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.sources_mut(kind) {
            pane.options.quality = quality;
            pane.sync(cx);
        }
        cx.notify();
    }

    pub(crate) fn filter_seeders(&mut self, kind: MediaKind, min: u32, cx: &mut Context<Self>) {
        if let Some(pane) = self.sources_mut(kind) {
            pane.options.min_seeders = min;
            pane.sync(cx);
        }
        cx.notify();
    }

    pub(crate) fn prepare_selected(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let Some(detail) = self.panes[index(kind)].detail.as_ref() else {
            return;
        };
        let Some(sources) = detail.sources.as_ref() else {
            return;
        };
        let Some(candidate) = sources
            .list
            .read(cx)
            .delegate()
            .selected_candidate()
            .cloned()
        else {
            return;
        };
        let target = sources.target.clone();
        self.page = Page::Transfers;
        self.playback_view.update(cx, |view, cx| {
            view.show_busy("Preparing the source…".into(), cx)
        });
        // A refusal is reported on the Transfers view by `send`.
        self.send(
            Pending::Prepare,
            AppAction::Prepare { target, candidate },
            cx,
        );
    }

    // ---- application worker ---------------------------------------------

    /// Queues a command. A refusal (worker missing or at capacity) is reported on
    /// the view that asked, and nothing is left pending.
    fn send(&mut self, pending: Pending, action: AppAction, cx: &mut Context<Self>) -> bool {
        let release_id = match &action {
            AppAction::ReleaseSession(id) => Some(id.clone()),
            _ => None,
        };
        let refusal = match &self.app {
            Err(message) => message.clone(),
            Ok(app) => {
                let generation = match &release_id {
                    Some(id) => self.tracker.issue_release(id.clone()),
                    None => self.tracker.issue(pending),
                };
                if app.submit(AppCommand { generation, action }) {
                    return true;
                }
                self.tracker.withdraw(generation);
                "The application service is busy; try again in a moment.".to_owned()
            }
        };
        match pending {
            Pending::Load | Pending::Settings | Pending::Credential | Pending::Cleanup => {
                self.settings_view
                    .update(cx, |view, cx| view.set_feedback(refusal, true, cx));
            }
            Pending::Release => {
                let id = release_id.unwrap_or_default();
                self.playback_view
                    .update(cx, |view, cx| view.release_failed(&id, refusal, cx));
            }
            Pending::Prepare | Pending::Play { .. } => {
                self.playback_view
                    .update(cx, |view, cx| view.show_error(refusal, cx));
            }
        }
        false
    }

    fn on_settings_intent(&mut self, intent: &SettingsIntent, cx: &mut Context<Self>) {
        match intent {
            SettingsIntent::Save(settings) => {
                self.send(
                    Pending::Settings,
                    AppAction::SaveSettings(settings.clone()),
                    cx,
                );
            }
            SettingsIntent::SetCredential { provider, value } => {
                self.send(
                    Pending::Credential,
                    AppAction::SetCredential {
                        provider: *provider,
                        // The event is borrowed; the original zeroizes on drop.
                        value: SecretString::new(value.expose().to_owned()),
                    },
                    cx,
                );
            }
            SettingsIntent::DeleteCredential(provider) => {
                self.send(
                    Pending::Credential,
                    AppAction::DeleteCredential(*provider),
                    cx,
                );
            }
            SettingsIntent::TestCredential(provider) => {
                self.send(
                    Pending::Credential,
                    AppAction::TestCredential(*provider),
                    cx,
                );
            }
            SettingsIntent::SetTmdbCredential { value } => {
                self.send(
                    Pending::Credential,
                    AppAction::SetTmdbCredential {
                        // The event is borrowed; the original zeroizes on drop.
                        value: SecretString::new(value.expose().to_owned()),
                    },
                    cx,
                );
            }
            SettingsIntent::DeleteTmdbCredential => {
                self.send(Pending::Credential, AppAction::DeleteTmdbCredential, cx);
            }
            SettingsIntent::TestTmdbCredential => {
                self.send(Pending::Credential, AppAction::TestTmdbCredential, cx);
            }
            SettingsIntent::CleanupUnused => {
                self.send(Pending::Cleanup, AppAction::CleanupUnused, cx);
            }
        }
    }

    fn on_playback_intent(&mut self, intent: &PlaybackIntent, cx: &mut Context<Self>) {
        match intent {
            PlaybackIntent::PlayFile {
                preparation,
                file_id,
            } => {
                self.send(
                    Pending::Play { remote: true },
                    AppAction::PlayFile {
                        preparation: *preparation,
                        file_id: file_id.clone(),
                    },
                    cx,
                );
            }
            PlaybackIntent::ReleaseSession(id) => {
                self.send(Pending::Release, AppAction::ReleaseSession(id.clone()), cx);
            }
            PlaybackIntent::OpenLocalFile => self.open_local_file(cx),
            PlaybackIntent::CleanupUnused => {
                self.send(Pending::Cleanup, AppAction::CleanupUnused, cx);
            }
            PlaybackIntent::OpenSettings => {
                self.page = Page::Settings;
                cx.notify();
            }
        }
    }

    /// Asks the platform file chooser asynchronously; nothing blocks rendering.
    fn open_local_file(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open media file".into()),
        });
        self.chooser = Some(cx.spawn(async move |this, cx| {
            let picked = match chosen.await {
                Ok(Ok(Some(paths))) => Chosen::Path(paths.into_iter().next()),
                Ok(Ok(None)) | Err(_) => Chosen::Cancelled,
                Ok(Err(_)) => Chosen::Failed,
            };
            this.update(cx, |this, cx| this.local_file_chosen(picked, cx))
                .ok();
        }));
    }

    fn local_file_chosen(&mut self, picked: Chosen, cx: &mut Context<Self>) {
        self.chooser = None;
        match picked {
            Chosen::Path(Some(path)) => {
                self.playback_view.update(cx, |view, cx| {
                    view.show_busy("Opening the local file…".into(), cx);
                });
                self.send(
                    Pending::Play { remote: false },
                    AppAction::PlayLocal { path },
                    cx,
                );
            }
            Chosen::Path(None) | Chosen::Cancelled => {}
            Chosen::Failed => self.playback_view.update(cx, |view, cx| {
                view.show_error("The file chooser could not be opened.".into(), cx);
            }),
        }
    }

    /// Routes a worker result by the command it answers, not by arrival order.
    fn apply_app(&mut self, event: AppEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let AppEvent {
            generation,
            outcome,
        } = event;
        match outcome {
            AppOutcome::Usage(usage) => {
                self.playback_view
                    .update(cx, |view, cx| view.show_usage(usage, cx));
            }
            AppOutcome::MediaFailed {
                session_id,
                message,
            } => {
                self.playback_view.update(cx, |view, cx| {
                    view.show_media_error(&session_id, message, cx)
                });
            }
            AppOutcome::CacheFull { message } => {
                if self.tracker.resolve(generation).current {
                    self.playback_view
                        .update(cx, |view, cx| view.show_cache_full(message, cx));
                }
            }
            AppOutcome::Invalidated { released } => {
                // The account changed: leases are gone and any file list is stale.
                for id in &released {
                    self.tracker.session_released(id);
                }
                self.playback_view.update(cx, |view, cx| {
                    for id in &released {
                        view.clear_session(id, cx);
                    }
                    view.clear_prepared(cx);
                });
            }
            AppOutcome::Settings {
                settings,
                credentials,
                tmdb_credential,
            } => self.apply_settings(
                generation,
                (settings, credentials, tmdb_credential),
                None,
                window,
                cx,
            ),
            AppOutcome::SettingsWarning {
                settings,
                credentials,
                tmdb_credential,
                message,
            } => self.apply_settings(
                generation,
                (settings, credentials, tmdb_credential),
                Some(message),
                window,
                cx,
            ),
            AppOutcome::MetadataCredential { status } => {
                self.tracker.resolve(generation);
                self.settings_view
                    .update(cx, |view, cx| view.set_tmdb_status(status, cx));
            }
            AppOutcome::TmdbCredentialChecked => {
                self.tracker.resolve(generation);
                self.settings_view
                    .update(cx, |view, cx| view.tmdb_credential_checked(cx));
            }
            AppOutcome::CredentialChecked { provider } => {
                self.tracker.resolve(generation);
                self.settings_view
                    .update(cx, |view, cx| view.credential_checked(provider, cx));
            }
            AppOutcome::Prepared {
                prepared,
                target,
                selection,
            } => {
                if self.tracker.resolve(generation).current {
                    self.playback_view.update(cx, |view, cx| {
                        view.show_prepared(generation, prepared, target, selection, cx);
                    });
                }
            }
            AppOutcome::PlaybackStarted { session, file_name } => {
                let resolved = self.tracker.resolve(generation);
                let remote = matches!(resolved.pending, Some(Pending::Play { remote: true }));
                // The server runs whether or not this result is still shown.
                self.tracker.session_started(session.id.clone(), remote);
                if resolved.current {
                    self.playback_view
                        .update(cx, |view, cx| view.show_started(session, file_name, cx));
                }
            }
            AppOutcome::Released { id } => {
                self.tracker.resolve(generation);
                self.tracker.session_released(&id);
                self.playback_view
                    .update(cx, |view, cx| view.show_released(&id, cx));
            }
            AppOutcome::Cleaned { bytes } => {
                self.tracker.resolve(generation);
                let message = format!(
                    "Removed {} of unused cached media.",
                    crate::playback_view::format_bytes(bytes)
                );
                self.settings_view
                    .update(cx, |view, cx| view.set_feedback(message.clone(), false, cx));
                self.notice = Some((message, false));
            }
            AppOutcome::CacheLimitNeeded { size, limit } => {
                if self.tracker.resolve(generation).current {
                    self.playback_view
                        .update(cx, |view, cx| view.show_limit_needed(size, limit, cx));
                }
            }
            // Capacity reached is a terminal error for the command, not progress.
            AppOutcome::Busy { label: message } | AppOutcome::Failed { message } => {
                self.fail_command(generation, message, window, cx);
            }
        }
        cx.notify();
        true
    }

    /// A successful settings snapshot is always applied: the mutation happened.
    /// A warning means it was applied but something needs the user's attention.
    fn apply_settings(
        &mut self,
        generation: u64,
        (settings, credentials, tmdb_credential): (
            crate::settings::Settings,
            Vec<(
                kinema_core::sources::DebridKind,
                crate::app::CredentialState,
            )>,
            crate::app::CredentialState,
        ),
        warning: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.tracker.resolve(generation);
        let provider = settings.metadata;
        let token_present = matches!(tmdb_credential, crate::app::CredentialState::Present);
        let acknowledged = matches!(
            resolved.pending,
            Some(Pending::Settings | Pending::Credential)
        );
        self.settings_view.update(cx, |view, cx| {
            view.set_snapshot(settings, credentials, cx);
            view.set_tmdb_status(tmdb_credential, cx);
            // The worker acknowledged a mutation (even an unchanged status).
            if acknowledged {
                view.finish_operation(cx);
            }
            if let Some(message) = warning {
                view.set_feedback(message, true, cx);
            }
        });
        if acknowledged {
            self.playback_view
                .update(cx, |view, cx| view.settings_changed(cx));
        }
        let first = !self.settings_loaded;
        let switched = self.metadata != provider;
        self.settings_loaded = true;
        if switched {
            for kind in [MediaKind::Movie, MediaKind::Series] {
                self.suspend_search(kind);
                self.cancel_browse(kind);
                self.suspend_retained_detail(kind);
                self.pane_mut(kind).grid.invalidate_artwork(cx);
            }
        }
        self.metadata = provider;
        self.tmdb_token_present = token_present;
        // Provider selection is persisted state: re-enter the page so the right
        // metadata source loads (or Settings opens when TMDB has no token).
        if (first || switched) && matches!(self.page, Page::Catalog(_)) {
            self.enter_page(self.page, window, cx);
        }
    }

    fn fail_command(
        &mut self,
        generation: u64,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let resolved = self.tracker.resolve(generation);
        match resolved.pending {
            Some(Pending::Settings | Pending::Credential | Pending::Cleanup) => {
                self.settings_view
                    .update(cx, |view, cx| view.set_feedback(message, true, cx));
            }
            Some(Pending::Load) if resolved.current => {
                self.settings_view
                    .update(cx, |view, cx| view.set_feedback(message, true, cx));
                // Unreadable settings: continue with the default provider so the
                // catalog is usable; the error stays visible in Settings.
                if !self.settings_loaded {
                    self.settings_loaded = true;
                    if matches!(self.page, Page::Catalog(_)) {
                        self.enter_page(self.page, window, cx);
                    }
                }
            }
            Some(Pending::Release) => {
                let id = resolved.release.unwrap_or_default();
                self.playback_view
                    .update(cx, |view, cx| view.release_failed(&id, message, cx));
            }
            Some(Pending::Prepare | Pending::Play { .. }) if resolved.current => {
                self.playback_view
                    .update(cx, |view, cx| view.show_error(message, cx));
            }
            _ => {}
        }
    }

    // ---- shutdown -------------------------------------------------------

    /// True when the window may close now. With active playback sessions it opens
    /// a confirmation first: quitting stops the local media server. External
    /// players are never closed by Kinema.
    fn may_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.quit_confirmed || !self.tracker.needs_quit_confirmation() {
            return true;
        }
        let sessions = self.tracker.active_sessions();
        let activity = if sessions > 0 {
            format!("{sessions} playback session(s) are active")
        } else {
            "A playback launch is in progress".to_owned()
        };
        if !window.has_active_dialog(cx) {
            let weak = cx.entity().downgrade();
            window.open_alert_dialog(cx, move |alert, _, _| {
                let weak = weak.clone();
                alert
                    .confirm()
                    .title("Stop the local media server?")
                    .description(format!(
                        "{activity}. Quitting stops Kinema's local media server, so VLC or mpv will lose the stream. Kinema does not close the player; close it yourself if you want."
                    ))
                    .ok_text("Quit and stop server")
                    .cancel_text("Keep Kinema open")
                    .on_ok(move |_, _, cx| {
                        weak.update(cx, |this, _| this.quit_confirmed = true).ok();
                        cx.quit();
                        true
                    })
            });
        }
        false
    }

    // ---- keyboard -----------------------------------------------------

    fn on_focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Page::Catalog(kind) = self.page
            && self.pane(kind).detail.is_none()
        {
            self.pane(kind)
                .input
                .update(cx, |input, cx| input.focus(window, cx));
        }
    }

    fn on_go_back(&mut self, cx: &mut Context<Self>) {
        if let Page::Catalog(kind) = self.page {
            self.close_top(kind, cx);
        }
    }

    // ---- rendering ----------------------------------------------------

    fn grid_key(
        &mut self,
        kind: MediaKind,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let grid = &mut self.pane_mut(kind).grid;
        if key == "enter" {
            if let Some(title) = grid
                .selected()
                .and_then(|ix| grid.titles().get(ix))
                .cloned()
            {
                self.open_title(kind, title, window, cx);
            }
            cx.stop_propagation();
            return;
        }
        let Some(nav) = Nav::from_key(key) else {
            return;
        };
        let metrics = grid_metrics(window, grid.titles().len());
        let next = catalog_view::step(
            grid.focused_selection(window, cx),
            nav,
            metrics.columns,
            grid.titles().len(),
        );
        grid.select(next);
        grid.focus_selected(window, cx);
        if let Some(ix) = next {
            grid.scroll
                .scroll_to_item(ix / metrics.columns, ScrollStrategy::Nearest);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn click_card(
        &mut self,
        kind: MediaKind,
        id: &kinema_core::catalog::ImdbId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.page != Page::Catalog(kind)
            || self.metadata != MetadataProvider::Cinemeta
            || self.pane(kind).detail.is_some()
        {
            return false;
        }
        let grid = &mut self.pane_mut(kind).grid;
        let Some(ix) = catalog_view::title_index(grid.titles(), kind, id) else {
            return false;
        };
        let title = grid.titles()[ix].clone();
        grid.select(Some(ix));
        grid.focus_selected(window, cx);
        self.open_title(kind, title, window, cx);
        true
    }

    fn render_catalog(&self, kind: MediaKind, cx: &mut Context<Self>) -> Div {
        let pane = self.pane(kind);
        let screen = &pane.screen;
        let muted = cx.theme().muted_foreground;
        let searching = screen.active().is_some();
        let browse_busy = pane.browse.iter().any(|p| p.res.busy());

        let mut status = div().flex().flex_col().gap(px(WORKSPACE_GAP));
        if let Some(message) = &screen.input_error {
            status = status.child(div().text_color(cx.theme().danger).child(message.clone()));
        }
        if let Some(query) = screen.active() {
            status = status.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("clear-search")
                            .icon(IconName::Close)
                            .label("Clear search")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.clear_search(kind, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_color(muted)
                            .child(format!("Results for “{}”", query.text())),
                    ),
            );
            status = status.child(status_strip(
                "search-error",
                &StripModel {
                    phase: screen.phase,
                    error: screen.error.as_deref(),
                    has_data: screen.fetched_at.is_some(),
                    origin: screen.source,
                    fetched_at: screen.fetched_at,
                    service: "Cinemeta",
                },
                cx,
            ));
        } else {
            let first = pane.browse.first().map(|p| &p.res);
            let last = pane.browse.last().map(|p| &p.res);
            let phase = last.map_or(crate::app_state::Phase::Idle, Resource::phase);
            let has_data = pane.browse.iter().any(|p| p.res.value().is_some());
            status = status.child(status_strip(
                "browse-error",
                &StripModel {
                    phase,
                    error: last.and_then(Resource::error),
                    has_data,
                    origin: first.map_or(crate::app_state::Origin::None, Resource::origin),
                    fetched_at: first.and_then(Resource::fetched_at),
                    service: "Cinemeta",
                },
                cx,
            ));
        }

        let items = if pane.grid.titles().is_empty() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .children(pane.grid.empty().cloned())
                .into_any_element()
        } else if self.view_mode == ViewMode::List {
            div()
                .size_full()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .child(List::new(&pane.list))
                .into_any_element()
        } else {
            let metrics = self.grid_metrics;
            let rows = metrics.total_rows;
            uniform_list(
                ("catalog-grid", index(kind)),
                rows,
                cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                    let grid = &this.pane(kind).grid;
                    range
                        .map(|row| {
                            let cards: Vec<_> = metrics
                                .row_items(row)
                                .map(|ix| {
                                    let mut data = grid.card(ix);
                                    if let Some(url) = &data.poster_url {
                                        use kinema_desktop::{
                                            artwork::ArtworkState, design::PosterStatus,
                                        };
                                        match kinema_desktop::artwork::lookup_or_request(
                                            grid.artwork_owner,
                                            grid.artwork_generation,
                                            url,
                                            cx,
                                        ) {
                                            ArtworkState::Ready(image) => data.poster = Some(image),
                                            ArtworkState::Loading => {
                                                data.poster_status = PosterStatus::Loading;
                                            }
                                            ArtworkState::Failed => {
                                                data.poster_status = PosterStatus::Failed;
                                            }
                                        }
                                    }
                                    let id = grid.titles()[ix].id().clone();
                                    let source_id = id.clone();
                                    kinema_desktop::design::MediaCard::new(data)
                                        .width(metrics.card_width)
                                        .track_focus(grid.card_focus(ix))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.click_card(kind, &id, window, cx);
                                        }))
                                        .on_source(cx.listener(move |this, _, window, cx| {
                                            if this.click_card(kind, &source_id, window, cx)
                                                && kind == MediaKind::Movie
                                            {
                                                this.open_sources(kind, window, cx);
                                            }
                                        }))
                                })
                                .collect();
                            div()
                                .h(px(metrics.row_height))
                                .flex()
                                .gap(px(kinema_desktop::design::tokens::metrics::CARD_GAP_X))
                                .children(cards)
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&pane.grid.scroll)
            .size_full()
            .into_any_element()
        };
        let mut body = div()
            .id(("catalog-body", index(kind)))
            .flex_1()
            .min_h_0()
            .track_focus(&pane.grid.focus)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if this.view_mode == ViewMode::Grid {
                    this.grid_key(kind, event, window, cx);
                }
            }))
            .child(items);
        let can_load_more = !searching
            && !browse_busy
            && pane
                .browse
                .last()
                .and_then(|p| p.res.value())
                .is_some_and(|titles| !titles.is_empty());
        let footer = can_load_more.then(|| {
            div().flex().justify_center().child(
                Button::new("load-more")
                    .icon(IconName::ArrowDown)
                    .label("Load more")
                    .outline()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| this.load_more(kind, cx))),
            )
        });
        body = body.flex().flex_col();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(WORKSPACE_GAP))
            .child(status)
            .child(body)
            .children(footer)
    }

    /// Fixed rail: four destinations, never expands.
    fn render_rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sessions = self.tracker.active_sessions();
        let entries = [
            (
                "Browse",
                IconName::Frame,
                Page::Catalog(self.browse_kind),
                matches!(self.page, Page::Catalog(_)),
            ),
            (
                "Transfers",
                IconName::HardDrive,
                Page::Transfers,
                self.page == Page::Transfers,
            ),
            (
                "Settings",
                IconName::Settings,
                Page::Settings,
                self.page == Page::Settings,
            ),
            ("About", IconName::Info, Page::Help, self.page == Page::Help),
        ];
        let theme = cx.theme();
        let (marker, border, bg) = (theme.ring, theme.border, theme.sidebar);
        let items: Vec<_> = entries
            .into_iter()
            .map(|(label, icon, page, active)| {
                let badge = (page == Page::Transfers && sessions > 0).then(|| {
                    div()
                        .absolute()
                        .top(px(2.0))
                        .right(px(6.0))
                        .px_1()
                        .rounded_full()
                        .bg(cx.theme().primary)
                        .text_color(cx.theme().primary_foreground)
                        .text_size(px(10.0))
                        .child(sessions.to_string())
                });
                div()
                    .relative()
                    .w(px(RAIL_WIDTH))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(active, |item| {
                        item.child(
                            div()
                                .absolute()
                                .left_0()
                                .top(px(8.0))
                                .w(px(3.0))
                                .h(px(20.0))
                                .bg(marker),
                        )
                    })
                    .child(
                        Button::new(label)
                            .ghost()
                            .icon(Icon::new(icon).size_5())
                            .w(px(36.0))
                            .h(px(36.0))
                            .selected(active)
                            .tooltip(label)
                            .accessibility_label(label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !(active && matches!(page, Page::Catalog(_))) {
                                    this.enter_page(page, window, cx);
                                }
                            })),
                    )
                    .children(badge)
            })
            .collect();
        div()
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .pt_2()
            .bg(bg)
            .border_r_1()
            .border_color(border)
            .children(items)
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sessions = self.tracker.active_sessions();
        let mut bar = div()
            .h(px(TOOLBAR_HEIGHT))
            .flex_none()
            .px(px(WORKSPACE_PAD))
            .flex()
            .items_center()
            .gap(px(WORKSPACE_GAP))
            .border_b_1()
            .border_color(cx.theme().border);
        let title = match self.page {
            Page::Catalog(_) => "Browse",
            other => other.title(),
        };
        if let Page::Catalog(kind) = self.page {
            let pane = self.pane(kind);
            let can_go_back = pane.detail.is_some() || pane.screen.active().is_some();
            if can_go_back {
                bar = bar.child(
                    Button::new("back")
                        .icon(IconName::ArrowLeft)
                        .ghost()
                        .h(px(28.0))
                        .tooltip("Back")
                        .accessibility_label("Back")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.pane(kind).detail.is_some() {
                                this.close_top(kind, cx);
                            } else {
                                this.clear_search(kind, window, cx);
                            }
                        })),
                );
            }
        }
        bar = bar.child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        );
        if let Page::Catalog(kind) = self.page {
            for (label, k) in [("Movies", MediaKind::Movie), ("Series", MediaKind::Series)] {
                bar = bar.child(
                    Button::new(label)
                        .label(label)
                        .ghost()
                        .h(px(28.0))
                        .selected(kind == k)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.page != Page::Catalog(k) {
                                this.enter_page(Page::Catalog(k), window, cx);
                            }
                        })),
                );
            }
            let searchable = self.settings_loaded
                && self.metadata == MetadataProvider::Cinemeta
                && self.pane(kind).detail.is_none();
            if searchable {
                let pane = self.pane(kind);
                let busy = if pane.screen.active().is_some() {
                    pane.screen.busy()
                } else {
                    pane.browse.iter().any(|p| p.res.busy())
                };
                bar = bar
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.0))
                            .max_w(px(420.0))
                            .child(Input::new(&pane.input).h(px(28.0))),
                    )
                    .child(
                        Button::new("search")
                            .icon(IconName::Search)
                            .label("Search")
                            .primary()
                            .h(px(28.0))
                            .on_click(cx.listener(move |this, _, _, cx| this.search(kind, cx))),
                    )
                    .child(
                        Button::new("refresh")
                            .icon(IconName::RefreshCw)
                            .ghost()
                            .h(px(28.0))
                            .tooltip("Refresh")
                            .accessibility_label("Refresh")
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.pane(kind).screen.active().is_some() {
                                    this.refresh_search(kind, cx);
                                } else {
                                    this.refresh_browse(kind, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("view-mode")
                            .icon(match self.view_mode {
                                ViewMode::Grid => IconName::Menu,
                                ViewMode::List => IconName::LayoutDashboard,
                            })
                            .ghost()
                            .h(px(28.0))
                            .tooltip(match self.view_mode {
                                ViewMode::Grid => "Show as list",
                                ViewMode::List => "Show as grid",
                            })
                            .accessibility_label("Toggle grid or list")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.view_mode = match this.view_mode {
                                    ViewMode::Grid => ViewMode::List,
                                    ViewMode::List => ViewMode::Grid,
                                };
                                cx.notify();
                            })),
                    );
            }
        }
        let session_text = match sessions {
            0 => "No active sessions".to_owned(),
            1 => "1 session active".to_owned(),
            n => format!("{n} sessions active"),
        };
        bar.child(div().flex_1())
            .child(
                Button::new("sessions")
                    .label(session_text)
                    .ghost()
                    .h(px(28.0))
                    .tooltip("Open Transfers")
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.page != Page::Transfers {
                            this.enter_page(Page::Transfers, window, cx);
                        }
                    })),
            )
            .child(
                Button::new("quit")
                    .label("Quit")
                    .ghost()
                    .h(px(28.0))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.may_close(window, cx) {
                            cx.quit();
                        }
                    })),
            )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let metrics = grid_metrics(window, self.pane(self.browse_kind).grid.titles().len());
        if metrics.columns != self.grid_metrics.columns {
            let grid = &self.pane(self.browse_kind).grid;
            if let Some(selected) = grid.focused_selection(window, cx) {
                grid.scroll
                    .scroll_to_item(metrics.row_of(selected), ScrollStrategy::Nearest);
            }
        }
        self.grid_metrics = metrics;
        if std::mem::take(&mut self.restore_grid_focus)
            && self.metadata == MetadataProvider::Cinemeta
            && matches!(self.page, Page::Catalog(_))
        {
            self.pane(self.browse_kind).grid.focus_selected(window, cx);
        }
        let content = match self.page {
            Page::Catalog(kind) => match &self.pane(kind).detail {
                Some(DetailPane {
                    sources: Some(sources),
                    notice,
                    ..
                }) => source_picker::render(sources, kind, notice.as_deref(), cx),
                _ if !self.settings_loaded => div()
                    .flex_1()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading settings…"),
                _ if self.metadata == MetadataProvider::Tmdb => {
                    div().flex_1().min_h_0().child(self.tmdb.clone())
                }
                Some(detail) => detail_view::render(detail, kind, cx),
                None => self.render_catalog(kind, cx),
            },
            Page::Transfers => div().flex_1().min_h_0().child(self.playback_view.clone()),
            Page::Settings => div().flex_1().min_h_0().child(self.settings_view.clone()),
            Page::Help => div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_2xl().child(Page::Help.title()))
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(Page::Help.description()),
                )
                .child(
                    // TMDB-required attribution: approved logo, unmodified, with the exact notice.
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .pt_4()
                        .child(img(crate::app_assets::TMDB_LOGO).h(px(18.0)))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(crate::settings_view::TMDB_ATTRIBUTION),
                        ),
                ),
        };
        let notice = self.notice.as_ref().map(|(text, is_error)| {
            let alert = if *is_error {
                Alert::error("app-notice", text.clone())
            } else {
                Alert::info("app-notice", text.clone())
            };
            div()
                .pb_3()
                .child(alert.on_close(cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                })))
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("Shell")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.on_focus_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoBack, _, cx| this.on_go_back(cx)))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(13.0))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_rail(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(self.render_toolbar(cx))
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .p(px(WORKSPACE_PAD))
                                    .gap(px(WORKSPACE_GAP))
                                    .overflow_hidden()
                                    .children(notice)
                                    .child(content),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowsePage, Page, cancel_browse, suspend_resource, take_resource_resume};

    #[test]
    fn provider_suspension_resumes_only_interrupted_first_loads() {
        let mut resource = Resource::<Vec<u8>>::with_base(100);
        let original = resource.begin(false);
        let mut marker = None;
        let mut forwarded = None;
        suspend_resource(&mut resource, &mut marker, |generation| {
            forwarded = Some(generation)
        });
        assert_eq!(forwarded, Some(original));
        assert!(!resource.fetched(original, vec![1], 1));
        let mut unexpected_cancel = false;
        suspend_resource(&mut resource, &mut marker, |_| unexpected_cancel = true);
        assert!(!unexpected_cancel);
        assert!(take_resource_resume(&resource, &mut marker));
        assert!(!take_resource_resume(&resource, &mut marker));
        resource.begin(false);
        suspend_resource(&mut resource, &mut marker, |_| {});
        let retry = resource.begin(false);
        assert!(!take_resource_resume(&resource, &mut marker));
        assert!(resource.failed(retry, "offline"));
        suspend_resource(&mut resource, &mut marker, |_| unexpected_cancel = true);
        assert!(!unexpected_cancel);
        assert!(!take_resource_resume(&resource, &mut marker));
        assert_eq!(resource.error(), Some("offline"));
    }

    #[test]
    fn provider_suspension_preserves_loaded_and_successful_empty_values() {
        for value in [vec![], vec![1_u8]] {
            let mut resource = Resource::with_base(100);
            let generation = resource.begin(false);
            assert!(resource.cache_hit(generation, value.clone(), 7, true));
            let mut marker = None;
            suspend_resource(&mut resource, &mut marker, |original| {
                assert_eq!(original, generation)
            });
            assert_eq!(resource.value(), Some(&value));
            assert_eq!(resource.fetched_at(), Some(7));
            assert!(!take_resource_resume(&resource, &mut marker));
            assert!(!resource.fetched(generation, vec![2], 8));
        }
    }
    use crate::app_state::{Resource, Stage};
    use kinema_core::MediaKind;

    #[test]
    fn leaving_browse_forwards_original_generations_and_rejects_late_results() {
        let mut pages = vec![
            BrowsePage {
                offset: 0,
                res: Resource::with_base(100),
            },
            BrowsePage {
                offset: 100,
                res: Resource::with_base(200),
            },
            BrowsePage {
                offset: 200,
                res: Resource::with_base(300),
            },
        ];
        let first = pages[0].res.begin(false);
        assert!(pages[0].res.cache_hit(first, Vec::new(), 7, true));
        let second = pages[1].res.begin(false);
        let settled = pages[2].res.begin(false);
        assert!(pages[2].res.fetched(settled, Vec::new(), 8));
        let mut forwarded = Vec::new();
        cancel_browse(&mut pages, |offset, generation| {
            forwarded.push((offset, generation))
        });
        assert_eq!(forwarded, vec![(0, first), (100, second)]);
        assert!(pages.iter().all(|page| !page.res.busy()));
        assert_eq!(pages[0].res.fetched_at(), Some(7));
        assert!(pages[0].res.value().is_some());
        assert!(pages[1].res.value().is_none());
        assert_eq!(pages[2].res.generation(), settled);
        assert!(!pages[1].res.apply(
            second,
            Stage::Fetched {
                value: Vec::new(),
                fetched_at: 9
            }
        ));
        let resumed = pages[1].res.begin(false);
        assert!(
            !pages[1]
                .res
                .apply(second, Stage::Failed("late failure".to_owned()))
        );
        assert!(pages[1].res.fetched(resumed, Vec::new(), 10));
        forwarded.clear();
        cancel_browse(&mut pages, |offset, generation| {
            forwarded.push((offset, generation))
        });
        assert!(forwarded.is_empty());
    }

    #[test]
    fn starts_on_movies() {
        assert_eq!(Page::default(), Page::Catalog(MediaKind::Movie));
    }

    #[test]
    fn catalog_pages_are_distinct_and_describe_cinemeta_use() {
        let movies = Page::Catalog(MediaKind::Movie);
        let series = Page::Catalog(MediaKind::Series);
        assert_ne!(movies, series);
        assert_eq!(movies.title(), "Movies");
        assert_eq!(series.title(), "Series");
        for page in [movies, series] {
            assert!(page.description().contains("load from Cinemeta"));
        }
    }

    #[test]
    fn settings_and_help_do_not_claim_implemented_services() {
        assert!(Page::Help.description().contains("untested"));
        assert!(Page::Help.description().contains("not the complete MVP"));
    }
}
