use std::{path::PathBuf, time::Duration};

use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme, IconName, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    list::{List, ListEvent, ListState},
    sidebar::{Sidebar, SidebarCollapsible, SidebarGroup, SidebarMenu, SidebarMenuItem},
    status_bar::StatusBar,
};
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
        catalog_view::TitleDelegate,
        detail_view::{self, DetailPane},
        episodes::EpisodeDelegate,
        source_picker::{self, SourcesPane},
        sources::{SortKey, SourceDelegate, SourceOptions},
        strip::{StripModel, status_strip},
    },
};

actions!(kinema, [FocusSearch, GoBack]);

/// Below this window width the sidebar collapses to icons.
const COMPACT_WIDTH: f32 = 900.0;

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

struct CatalogPane {
    screen: ScreenState,
    /// Popular titles, one resource per loaded page; shown when no search is active.
    browse: Vec<BrowsePage>,
    input: Entity<InputState>,
    list: Entity<ListState<TitleDelegate>>,
    detail: Option<DetailPane>,
    _subscriptions: Vec<Subscription>,
}

pub(crate) struct Shell {
    page: Page,
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
                input,
                list,
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
        for pane in &self.panes {
            let screen = &pane.screen;
            let (titles, empty): (Vec<CatalogTitle>, Option<SharedString>) =
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
            let changed = pane
                .list
                .update(cx, |list, _| list.delegate_mut().set(&titles, empty));
            if changed {
                pane.list.update(cx, |_, cx| cx.notify());
            }
        }
    }

    // ---- browse -------------------------------------------------------

    fn ensure_browse(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        if self.pane(kind).browse.is_empty() {
            self.load_browse_page(kind, 0, false, cx);
        }
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

    fn clear_search(&mut self, kind: MediaKind, window: &mut Window, cx: &mut Context<Self>) {
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
        self.page = page;
        match page {
            Page::Catalog(kind) if self.settings_loaded => match self.metadata {
                MetadataProvider::Cinemeta => {
                    self.tmdb.update(cx, |view, cx| view.deactivate(cx));
                    self.ensure_browse(kind, cx);
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
        self.pane_mut(kind).detail = Some(detail);
        self.open_sources(kind, window, cx);
    }

    // ---- search -------------------------------------------------------

    fn search(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let text = self.pane(kind).input.read(cx).value();
        let request = self.pane_mut(kind).screen.search(&text);
        self.dispatch(kind, request);
        self.sync_titles(cx);
        cx.notify();
    }

    fn refresh_search(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
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
        let detail = self.build_detail(kind, title, window, cx);
        self.pane_mut(kind).detail = Some(detail);
        self.request_details(kind, false, true, cx);
        cx.notify();
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

    fn render_list_page(&self, kind: MediaKind, cx: &mut Context<Self>) -> Div {
        let pane = self.pane(kind);
        let screen = &pane.screen;
        let muted = cx.theme().muted_foreground;
        let searching = screen.active().is_some();
        let browse_busy = pane.browse.iter().any(|p| p.res.busy());
        let busy = if searching {
            screen.busy()
        } else {
            browse_busy
        };

        let controls = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(Input::new(&pane.input)))
            .child(
                Button::new("search")
                    .icon(IconName::Search)
                    .label("Search")
                    .primary()
                    .on_click(cx.listener(move |this, _, _, cx| this.search(kind, cx))),
            )
            .child(
                Button::new("refresh")
                    .icon(IconName::RefreshCw)
                    .label("Refresh")
                    .outline()
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.pane(kind).screen.active().is_some() {
                            this.refresh_search(kind, cx);
                        } else {
                            this.refresh_browse(kind, cx);
                        }
                    })),
            );

        let mut status = div().flex().flex_col().gap_2();
        if let Some(message) = &screen.input_error {
            status = status.child(div().text_color(cx.theme().danger).child(message.clone()));
        }
        let heading = if let Some(query) = screen.active() {
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
            Page::Catalog(kind).title().to_owned()
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
            format!("Popular {}", Page::Catalog(kind).title().to_lowercase())
        };

        let mut body = div().flex_1().min_h_0().flex().flex_col().gap_2().child(
            div()
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .child(List::new(&pane.list)),
        );
        let can_load_more = !searching
            && !browse_busy
            && pane
                .browse
                .last()
                .and_then(|p| p.res.value())
                .is_some_and(|titles| !titles.is_empty());
        if can_load_more {
            body = body.child(
                div().flex().justify_center().child(
                    Button::new("load-more")
                        .icon(IconName::ArrowDown)
                        .label("Load more")
                        .outline()
                        .on_click(cx.listener(move |this, _, _, cx| this.load_more(kind, cx))),
                ),
            );
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_2xl().child(heading))
            .child(
                div()
                    .text_color(muted)
                    .child(Page::Catalog(kind).description()),
            )
            .child(controls)
            .child(status)
            .child(body)
    }

    fn render_sidebar(&self, compact: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = [
            ("Movies", IconName::Frame, Page::Catalog(MediaKind::Movie)),
            (
                "Series",
                IconName::GalleryVerticalEnd,
                Page::Catalog(MediaKind::Series),
            ),
            ("Transfers", IconName::HardDrive, Page::Transfers),
            ("Settings", IconName::Settings, Page::Settings),
            ("About", IconName::Info, Page::Help),
        ];
        let items: Vec<SidebarMenuItem> = entries
            .into_iter()
            .map(|(label, icon, page)| {
                SidebarMenuItem::new(label)
                    .icon(icon)
                    .active(self.page == page)
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.enter_page(page, window, cx);
                    }))
            })
            .collect();
        Sidebar::<SidebarGroup<SidebarMenu>>::new("navigation")
            .collapsible(SidebarCollapsible::Icon)
            .collapsed(compact)
            .header(
                div()
                    .px_2()
                    .py_1()
                    .font_weight(FontWeight::BOLD)
                    .child(if compact { "K" } else { "Kinema Next" }),
            )
            .child(SidebarGroup::new("Browse").child(SidebarMenu::new().children(items)))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < px(COMPACT_WIDTH);
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
                None => self.render_list_page(kind, cx),
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
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_sidebar(compact, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .p_4()
                            .overflow_hidden()
                            .children(notice)
                            .child(content),
                    ),
            )
            .child(
                StatusBar::new()
                    .left("Loopback media server only · external VLC/mpv playback · needs your debrid account")
                    .right(
                        Button::new("quit")
                            .label("Quit")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.may_close(window, cx) {
                                    cx.quit();
                                }
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::Page;
    use kinema_core::MediaKind;

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
