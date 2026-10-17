//! V3 composition root. Observe this entity for state changes and subscribe to
//! `CoordinatorEvent` for settings/playback outcomes. `collection` is the current
//! page Resource; `titles` is its deduplicated projection across loaded pages.
//! `load(kind, query, refresh)` replaces the subject, `next_page` appends. All
//! requests carry monotonically increasing generations. `details` / `sources`
//! expose Resources for P2. `command(AppAction)` covers preparation, explicit-file
//! and local playback, release, cleanup, settings and secure credential intents;
//! secrets are passed directly to the worker, never retained here. The entity
//! owns both workers and its polling task; no blocking work runs on the UI thread.
//! `needs_quit_confirmation` includes pending launches, not just active sessions.
use crate::{
    app::{AppAction, AppCommand, AppEvent, AppOutcome, AppWorker, CredentialState},
    app_state::{Resource, Stage},
    app_tracker::{Pending, Tracker},
    catalog_worker::{
        CatalogWorker, MetadataAction, MetadataRequest, MetadataSnapshot, Mode, Outcome, Poll,
        Request, Submit,
    },
    metadata_bridge::{self, Ask, MetaEvent, Polled, SourceSet},
    settings::{MetadataProvider, Settings},
    tmdb::{TmdbData, TmdbDetails, TmdbQuery, TmdbTitle},
};
use gpui::*;
use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, ImdbId, SearchQuery, TitleDetails},
    sources::{IndexerKind, PlaybackTarget},
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BrowseTitle {
    Cinemeta(CatalogTitle),
    Tmdb(TmdbTitle),
}
impl BrowseTitle {
    pub fn key(&self) -> String {
        match self {
            Self::Cinemeta(t) => format!("{:?}:{}", t.kind(), t.id().as_str()),
            Self::Tmdb(t) => format!("{:?}:tmdb:{}", t.kind, t.id),
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Cinemeta(t) => t.name(),
            Self::Tmdb(t) => &t.name,
        }
    }
    pub fn kind(&self) -> MediaKind {
        match self {
            Self::Cinemeta(t) => t.kind(),
            Self::Tmdb(t) => t.kind,
        }
    }
    pub fn year(&self) -> Option<u16> {
        match self {
            Self::Cinemeta(t) => t.year(),
            Self::Tmdb(t) => t.year(),
        }
    }
    pub fn rating(&self) -> Option<f32> {
        match self {
            Self::Cinemeta(t) => t.imdb_rating(),
            Self::Tmdb(t) => t.imdb_rating(),
        }
    }
    pub fn poster_url(&self) -> Option<&str> {
        match self {
            Self::Cinemeta(t) => t.poster_url(),
            Self::Tmdb(t) => t.poster_url(),
        }
    }
    pub fn overview(&self) -> Option<&str> {
        match self {
            Self::Cinemeta(t) => t.overview(),
            Self::Tmdb(t) => t.overview.as_deref(),
        }
    }
    pub fn genres(&self) -> &[String] {
        match self {
            Self::Cinemeta(t) => t.genres(),
            Self::Tmdb(t) => t.genres(),
        }
    }
    pub fn runtime(&self) -> Option<u16> {
        match self {
            Self::Cinemeta(t) => t.runtime_minutes(),
            Self::Tmdb(t) => t.runtime_minutes(),
        }
    }
    pub fn seasons(&self) -> Option<u32> {
        match self {
            Self::Cinemeta(t) => t.season_count(),
            Self::Tmdb(t) => t.season_count(),
        }
    }
    pub fn imdb(&self) -> Option<&ImdbId> {
        match self {
            Self::Cinemeta(t) => Some(t.id()),
            Self::Tmdb(t) => t.imdb.as_ref(),
        }
    }
}
#[derive(Clone)]
pub enum CoordinatorEvent {
    App(AppEvent),
    OpenSettings,
    About,
}
impl EventEmitter<CoordinatorEvent> for AppCoordinator {}

/// Public, nonsecret transfer snapshot. URLs here are local server URLs only.
#[derive(Clone, Debug)]
pub struct SessionSnapshot {
    pub session: crate::media::PlaybackSession,
    pub file_name: String,
}

pub struct AppCoordinator {
    worker: Result<CatalogWorker, String>,
    app: Result<AppWorker, String>,
    tracker: Tracker,
    pub settings: Option<Settings>,
    pub tmdb_credential: Option<CredentialState>,
    pub credentials: Vec<(kinema_core::sources::DebridKind, CredentialState)>,
    pub usage: Option<crate::media::UsageSnapshot>,
    pub sessions: Vec<SessionSnapshot>,
    pub(crate) collection: Resource<Vec<BrowseTitle>>,
    pub(crate) titles: Vec<BrowseTitle>,
    pub(crate) details: Resource<TitleDetails>,
    pub(crate) sources: Resource<SourceSet>,
    pub(crate) tmdb_details: Resource<TmdbDetails>,
    pub kind: MediaKind,
    pub query: String,
    pub notice: Option<String>,
    pub quit_requested: bool,
    generation: u64,
    offset: u32,
    page_start: usize,
    exhausted: bool,
    request: Option<(Ask, u64)>,
    tmdb_request: Option<TmdbQuery>,
    detail_request: Option<(MetadataAction, u64)>,
    source_request: Option<(Ask, u64)>,
    fixture: Option<Vec<BrowseTitle>>,
    fixture_failure: Option<String>,
    #[cfg(feature = "ui-probe")]
    pub(crate) fixture_art: std::collections::HashMap<String, std::sync::Arc<RenderImage>>,
    _poll: Task<()>,
}
impl AppCoordinator {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                if this.update(cx, |this, cx| this.drain(cx)).is_err() {
                    break;
                }
            }
        });
        let mut this = Self {
            worker: CatalogWorker::spawn_default().map_err(|e| e.to_string()),
            app: AppWorker::spawn().map_err(|e| e.to_string()),
            tracker: Tracker::default(),
            settings: None,
            tmdb_credential: None,
            credentials: Vec::new(),
            usage: None,
            sessions: Vec::new(),
            collection: Resource::default(),
            titles: Vec::new(),
            details: Resource::default(),
            sources: Resource::default(),
            tmdb_details: Resource::default(),
            kind: MediaKind::Movie,
            query: String::new(),
            notice: None,
            quit_requested: false,
            generation: 0,
            offset: 0,
            page_start: 0,
            exhausted: false,
            request: None,
            tmdb_request: None,
            detail_request: None,
            source_request: None,
            fixture: None,
            fixture_failure: None,
            #[cfg(feature = "ui-probe")]
            fixture_art: Default::default(),
            _poll: poll,
        };
        this.collection.begin(true);
        this.command(AppAction::LoadSettings, cx);
        this
    }
    /// In-memory data-source seam: no workers, database, vault or network.
    #[cfg(feature = "ui-probe")]
    pub(crate) fn fixture(
        titles: Vec<BrowseTitle>,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let failure = error.clone();
        let mut collection = Resource::default();
        let generation = collection.begin(true);
        if let Some(error) = error {
            collection.failed(generation, error);
        } else {
            collection.fetched(generation, titles.clone(), 0);
        }
        Self {
            worker: Err("Fixture source".into()),
            app: Err("Fixture source".into()),
            tracker: Tracker::default(),
            settings: Some(Settings::default()),
            tmdb_credential: None,
            credentials: Vec::new(),
            usage: None,
            sessions: Vec::new(),
            collection,
            titles: titles.clone(),
            details: Resource::default(),
            sources: Resource::default(),
            tmdb_details: Resource::default(),
            kind: MediaKind::Movie,
            query: String::new(),
            notice: None,
            quit_requested: false,
            generation: 0,
            offset: 0,
            page_start: 0,
            exhausted: true,
            request: None,
            tmdb_request: None,
            detail_request: None,
            source_request: None,
            fixture: Some(titles),
            fixture_failure: failure,
            fixture_art: Default::default(),
            _poll: cx.spawn(async |_, _| {}),
        }
    }
    pub fn load(&mut self, kind: MediaKind, query: String, refresh: bool, cx: &mut Context<Self>) {
        if let Some(source) = &self.fixture {
            self.kind = kind;
            self.query = query;
            self.titles = source
                .iter()
                .filter(|t| {
                    t.kind() == kind && t.name().to_lowercase().contains(&self.query.to_lowercase())
                })
                .cloned()
                .collect();
            let generation = self.collection.begin(true);
            if let Some(error) = &self.fixture_failure {
                self.collection.failed(generation, error);
            } else {
                self.collection.fetched(generation, self.titles.clone(), 0);
            }
            cx.notify();
            return;
        }
        if self.settings.is_none() {
            return;
        }
        if let (Ok(worker), Some((ask, generation))) = (&self.worker, self.request.take()) {
            metadata_bridge::cancel(worker, ask, generation);
        }
        if let Ok(worker) = &self.worker {
            worker.cancel_search(self.kind, self.collection.generation());
            if let Some(query) = self.tmdb_request.take() {
                worker.cancel_metadata(&MetadataAction::Tmdb(query), self.collection.generation());
            }
        }
        let replace = kind != self.kind || query != self.query;
        self.kind = kind;
        self.query = query;
        self.offset = 0;
        self.exhausted = false;
        if replace {
            self.titles.clear();
        }
        self.fetch(replace, refresh, cx);
    }
    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        if self.collection.busy()
            || self.collection.error().is_some()
            || self.exhausted
            || !self.query.is_empty() && self.provider() == MetadataProvider::Cinemeta
        {
            return;
        }
        self.offset += if self.provider() == MetadataProvider::Tmdb {
            1
        } else {
            100
        };
        self.fetch(true, false, cx);
    }
    fn provider(&self) -> MetadataProvider {
        self.settings
            .as_ref()
            .map(|s| s.metadata)
            .unwrap_or_default()
    }
    fn fetch(&mut self, replace: bool, refresh: bool, cx: &mut Context<Self>) {
        if self.offset == 0 {
            self.page_start = 0;
        } else if replace {
            self.page_start = self.titles.len();
        }
        self.generation += 1;
        self.collection = if replace {
            Resource::with_base(self.generation * 1_000_000)
        } else {
            std::mem::take(&mut self.collection)
        };
        let generation = self.collection.begin(replace);
        let mode = if refresh { Mode::Refresh } else { Mode::Load };
        let result = (|| {
            let worker = self.worker.as_ref().map_err(Clone::clone)?;
            if self.provider() == MetadataProvider::Tmdb {
                let query = if self.query.trim().is_empty() {
                    TmdbQuery::Browse {
                        kind: self.kind,
                        page: self.offset + 1,
                    }
                } else {
                    TmdbQuery::Search {
                        query: SearchQuery::new(self.kind, &self.query)
                            .map_err(|e| e.to_string())?,
                        page: self.offset + 1,
                    }
                };
                self.tmdb_request = Some(query.clone());
                if worker.submit_metadata(MetadataRequest {
                    generation,
                    action: MetadataAction::Tmdb(query),
                    mode,
                }) == Submit::Closed
                {
                    return Err("Catalog worker stopped".into());
                }
            } else if self.query.trim().is_empty() {
                let ask = Ask::Browse {
                    kind: self.kind,
                    offset: self.offset,
                };
                metadata_bridge::request(worker, ask.clone(), generation, refresh)?;
                self.request = Some((ask, generation));
            } else {
                let query = SearchQuery::new(self.kind, &self.query).map_err(|e| e.to_string())?;
                if worker.submit(Request {
                    generation,
                    query,
                    mode,
                }) == Submit::Closed
                {
                    return Err("Catalog worker stopped".into());
                }
            }
            Ok::<_, String>(())
        })();
        if let Err(e) = result {
            self.collection.failed(generation, e);
        }
        cx.notify();
    }
    fn apply_page(&mut self, generation: u64, stage: Stage<Vec<BrowseTitle>>) {
        if let Some(exhausted) = merge_page(
            &mut self.collection,
            &mut self.titles,
            self.page_start,
            generation,
            stage,
        ) {
            self.exhausted = exhausted;
        }
    }
    fn drain(&mut self, cx: &mut Context<Self>) {
        let mut search = Vec::new();
        let mut metadata = Vec::new();
        let mut stopped = false;
        if let Ok(worker) = &self.worker {
            loop {
                match worker.poll() {
                    Poll::Event(e) => search.push(e),
                    Poll::Empty => break,
                    Poll::Disconnected => {
                        stopped = true;
                        break;
                    }
                }
            }
            metadata.extend(std::iter::from_fn(|| metadata_bridge::poll(worker)));
        }
        let mut changed = !search.is_empty() || !metadata.is_empty();
        for event in search {
            if event.kind == self.kind {
                let stage = match event.outcome {
                    Outcome::CacheHit {
                        snapshot,
                        remote_follows,
                    } => Stage::CacheHit {
                        value: snapshot
                            .titles
                            .into_iter()
                            .map(BrowseTitle::Cinemeta)
                            .collect(),
                        fetched_at: snapshot.fetched_at,
                        remote_follows,
                    },
                    Outcome::Fetched(snapshot) => Stage::Fetched {
                        value: snapshot
                            .titles
                            .into_iter()
                            .map(BrowseTitle::Cinemeta)
                            .collect(),
                        fetched_at: snapshot.fetched_at,
                    },
                    Outcome::Failed(e) => Stage::Failed(e),
                    Outcome::CacheMiss => continue,
                };
                self.apply_page(event.generation, stage);
            }
        }
        for event in metadata {
            match event {
                Polled::Cinemeta(MetaEvent::Browse {
                    kind,
                    offset,
                    generation,
                    stage,
                }) if kind == self.kind && offset == self.offset => {
                    let stage = match stage {
                        Stage::CacheHit {
                            value,
                            fetched_at,
                            remote_follows,
                        } => Stage::CacheHit {
                            value: value.into_iter().map(BrowseTitle::Cinemeta).collect(),
                            fetched_at,
                            remote_follows,
                        },
                        Stage::Fetched { value, fetched_at } => Stage::Fetched {
                            value: value.into_iter().map(BrowseTitle::Cinemeta).collect(),
                            fetched_at,
                        },
                        Stage::Failed(e) => Stage::Failed(e),
                    };
                    self.apply_page(generation, stage);
                }
                Polled::Cinemeta(MetaEvent::Details {
                    generation, stage, ..
                }) => {
                    self.details.apply(generation, stage);
                }
                Polled::Cinemeta(MetaEvent::Sources {
                    generation, stage, ..
                }) => {
                    self.sources.apply(generation, stage);
                }
                Polled::Tmdb(event)
                    if matches!(
                        event.action,
                        MetadataAction::Tmdb(TmdbQuery::Details { .. })
                    ) =>
                {
                    let stage = match event.result {
                        Ok(MetadataSnapshot::Tmdb(snapshot)) => match snapshot.data {
                            TmdbData::Details(value) => {
                                if event.saved {
                                    Stage::CacheHit {
                                        value,
                                        fetched_at: snapshot.fetched_at,
                                        remote_follows: event.refreshing,
                                    }
                                } else {
                                    Stage::Fetched {
                                        value,
                                        fetched_at: snapshot.fetched_at,
                                    }
                                }
                            }
                            _ => continue,
                        },
                        Err(error) => Stage::Failed(error),
                        _ => continue,
                    };
                    self.tmdb_details.apply(event.generation, stage);
                }
                Polled::Tmdb(event)
                    if Some(&event.action)
                        == self
                            .tmdb_request
                            .as_ref()
                            .map(|q| MetadataAction::Tmdb(q.clone()))
                            .as_ref() =>
                {
                    let last_page = matches!(&event.result, Ok(MetadataSnapshot::Tmdb(snapshot)) if matches!(&snapshot.data, TmdbData::Page(page) if page.page >= page.total_pages));
                    let stage = match event.result {
                        Ok(MetadataSnapshot::Tmdb(snapshot)) => match snapshot.data {
                            TmdbData::Page(page) => {
                                let value =
                                    page.titles.into_iter().map(BrowseTitle::Tmdb).collect();
                                if event.saved {
                                    Stage::CacheHit {
                                        value,
                                        fetched_at: snapshot.fetched_at,
                                        remote_follows: event.refreshing,
                                    }
                                } else {
                                    Stage::Fetched {
                                        value,
                                        fetched_at: snapshot.fetched_at,
                                    }
                                }
                            }
                            _ => continue,
                        },
                        Err(e) => Stage::Failed(e),
                        _ => continue,
                    };
                    self.apply_page(event.generation, stage);
                    if event.generation == self.collection.generation() && last_page {
                        self.exhausted = true;
                    }
                }
                _ => {}
            }
        }
        if stopped {
            let g = self.collection.generation();
            changed |= self
                .collection
                .failed(g, "Catalog worker stopped. Restart Kinema.");
            let g = self.details.generation();
            changed |= self
                .details
                .failed(g, "Catalog worker stopped. Restart Kinema.");
            let g = self.sources.generation();
            changed |= self
                .sources
                .failed(g, "Catalog worker stopped. Restart Kinema.");
            let g = self.tmdb_details.generation();
            changed |= self
                .tmdb_details
                .failed(g, "Catalog worker stopped. Restart Kinema.");
        }
        if self.app.as_ref().is_ok_and(|a| a.is_stopped()) {
            let message = "Application worker stopped. Restart Kinema.";
            changed |= self.notice.as_deref() != Some(message);
            self.notice = Some(message.into());
        }
        let events: Vec<_> = self
            .app
            .as_ref()
            .ok()
            .map(|a| std::iter::from_fn(|| a.poll()).collect())
            .unwrap_or_default();
        changed |= !events.is_empty();
        for event in events {
            self.apply_app(event, cx);
        }
        if changed {
            cx.notify();
        }
    }
    fn apply_app(&mut self, event: AppEvent, cx: &mut Context<Self>) {
        // Usage/invalidation are unsolicited; don't consume command bookkeeping.
        let unsolicited = matches!(
            event.outcome,
            AppOutcome::Usage(_) | AppOutcome::Invalidated { .. }
        );
        let resolved = (!unsolicited).then(|| self.tracker.resolve(event.generation));
        match &event.outcome {
            AppOutcome::PlaybackStarted { session, file_name } => {
                self.sessions.retain(|s| s.session.id != session.id);
                self.sessions.push(SessionSnapshot {
                    session: session.clone(),
                    file_name: file_name.clone(),
                });
                self.tracker.session_started(
                    session.id.clone(),
                    resolved
                        .as_ref()
                        .is_some_and(|r| matches!(r.pending, Some(Pending::Play { remote: true }))),
                );
            }
            AppOutcome::Released { id } => {
                self.tracker.session_released(id);
                self.sessions.retain(|s| s.session.id != *id);
            }
            AppOutcome::Invalidated { released } => {
                for id in released {
                    self.tracker.session_released(id);
                    self.sessions.retain(|s| s.session.id != *id);
                }
            }
            AppOutcome::Usage(usage) => self.usage = Some(usage.clone()),
            _ => {}
        }
        if resolved.as_ref().is_some_and(|r| !r.current) {
            return;
        }
        match &event.outcome {
            AppOutcome::Settings {
                settings,
                tmdb_credential,
                credentials,
                ..
            }
            | AppOutcome::SettingsWarning {
                settings,
                tmdb_credential,
                credentials,
                ..
            } => {
                let reload = self
                    .settings
                    .as_ref()
                    .is_none_or(|s| s.metadata != settings.metadata);
                self.settings = Some(settings.clone());
                self.credentials = credentials.clone();
                self.tmdb_credential = Some(tmdb_credential.clone());
                if reload {
                    self.generation += 1;
                    self.collection = Resource::with_base(self.generation * 1_000_000);
                    self.titles.clear();
                    self.load(self.kind, self.query.clone(), false, cx);
                }
            }
            AppOutcome::MetadataCredential { status } => {
                self.tmdb_credential = Some(status.clone());
            }
            AppOutcome::Failed { message } => {
                self.notice = Some(message.clone());
                if resolved
                    .as_ref()
                    .is_some_and(|r| r.pending == Some(Pending::Load))
                {
                    let g = self.collection.generation();
                    self.collection.failed(g, message);
                }
            }
            _ => {}
        }
        if let AppOutcome::SettingsWarning { message, .. } = &event.outcome {
            self.notice = Some(message.clone());
        }
        cx.emit(CoordinatorEvent::App(event));
    }
    /// Typed application intent gateway. Returns its command generation, or None
    /// on backpressure/start failure. P2/P4 should subscribe before submitting.
    pub fn command(&mut self, action: AppAction, cx: &mut Context<Self>) -> Option<u64> {
        let pending = match &action {
            AppAction::LoadSettings => Pending::Load,
            AppAction::SaveSettings(_) => Pending::Settings,
            AppAction::Prepare { .. } => Pending::Prepare,
            AppAction::PlayFile { .. } => Pending::Play { remote: true },
            AppAction::PlayLocal { .. } => Pending::Play { remote: false },
            AppAction::ReleaseSession(_) => Pending::Release,
            AppAction::CleanupUnused => Pending::Cleanup,
            _ => Pending::Credential,
        };
        let generation = if let AppAction::ReleaseSession(id) = &action {
            self.tracker.issue_release(id.clone())
        } else {
            self.tracker.issue(pending)
        };
        if self
            .app
            .as_ref()
            .is_ok_and(|a| a.submit(AppCommand { generation, action }))
        {
            cx.notify();
            Some(generation)
        } else {
            self.tracker.withdraw(generation);
            let message = "Application service unavailable or busy. Retry.";
            self.notice = Some(message.into());
            if pending == Pending::Load {
                let g = self.collection.generation();
                self.collection.failed(g, message);
            }
            cx.notify();
            None
        }
    }
    fn cancel_details(&mut self) {
        if let (Ok(worker), Some((action, generation))) = (&self.worker, self.detail_request.take())
        {
            worker.cancel_metadata(&action, generation);
        }
        self.details.cancel();
        self.tmdb_details.cancel();
    }
    /// Opens or explicitly refreshes provider-native details; never remaps by name.
    pub(crate) fn title_details(
        &mut self,
        title: &BrowseTitle,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        self.cancel_details();
        match title {
            BrowseTitle::Cinemeta(title) => {
                let generation = self.details.begin(true);
                self.detail_request = Some((
                    MetadataAction::Details {
                        kind: title.kind(),
                        id: title.id().clone(),
                    },
                    generation,
                ));
                let result = self
                    .worker
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|worker| {
                        metadata_bridge::request(
                            worker,
                            Ask::Details {
                                kind: title.kind(),
                                id: title.id().clone(),
                            },
                            generation,
                            refresh,
                        )
                    });
                if let Err(error) = result {
                    self.details.failed(generation, error);
                }
            }
            BrowseTitle::Tmdb(title) => {
                let generation = self.tmdb_details.begin(true);
                self.detail_request = Some((
                    MetadataAction::Tmdb(TmdbQuery::Details {
                        kind: title.kind,
                        id: title.id,
                    }),
                    generation,
                ));
                let result = self
                    .worker
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(|worker| {
                        if worker.submit_metadata(MetadataRequest {
                            generation,
                            action: MetadataAction::Tmdb(TmdbQuery::Details {
                                kind: title.kind,
                                id: title.id,
                            }),
                            mode: if refresh { Mode::Refresh } else { Mode::Load },
                        }) == Submit::Closed
                        {
                            Err("Catalog worker stopped".into())
                        } else {
                            Ok(())
                        }
                    });
                if let Err(error) = result {
                    self.tmdb_details.failed(generation, error);
                }
            }
        }
        cx.notify();
    }
    pub fn details(&mut self, kind: MediaKind, id: ImdbId, cx: &mut Context<Self>) {
        self.cancel_details();
        let generation = self.details.begin(true);
        self.detail_request = Some((
            MetadataAction::Details {
                kind,
                id: id.clone(),
            },
            generation,
        ));
        let result = self
            .worker
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|worker| {
                metadata_bridge::request(worker, Ask::Details { kind, id }, generation, false)
            });
        if let Err(error) = result {
            self.details.failed(generation, error);
        }
        cx.notify();
    }
    pub fn sources(
        &mut self,
        indexer: IndexerKind,
        target: PlaybackTarget,
        cx: &mut Context<Self>,
    ) {
        if let (Ok(worker), Some((ask, generation))) = (&self.worker, self.source_request.take()) {
            metadata_bridge::cancel(worker, ask, generation);
        }
        let generation = self.sources.begin(true);
        let ask = Ask::Sources { indexer, target };
        self.source_request = Some((ask.clone(), generation));
        let result = self
            .worker
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|worker| metadata_bridge::request(worker, ask, generation, false));
        if let Err(error) = result {
            self.sources.failed(generation, error);
        }
        cx.notify();
    }
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        cx.emit(CoordinatorEvent::OpenSettings);
    }
    pub fn about(&mut self, cx: &mut Context<Self>) {
        cx.emit(CoordinatorEvent::About);
    }
    pub fn retry_page(&mut self, cx: &mut Context<Self>) {
        if !self.collection.busy() {
            self.fetch(false, true, cx);
        }
    }
    /// Returns true only when the caller can quit without interrupting media.
    pub fn request_quit(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.needs_quit_confirmation() {
            return true;
        }
        self.quit_requested = true;
        cx.notify();
        false
    }
    pub fn active_sessions(&self) -> usize {
        self.tracker.active_sessions()
    }
    pub fn needs_quit_confirmation(&self) -> bool {
        self.tracker.needs_quit_confirmation()
    }
}

/// Shared production policy, independently testable without creating workers.
fn merge_page(
    resource: &mut Resource<Vec<BrowseTitle>>,
    titles: &mut Vec<BrowseTitle>,
    page_start: usize,
    generation: u64,
    stage: Stage<Vec<BrowseTitle>>,
) -> Option<bool> {
    let failure = matches!(&stage, Stage::Failed(_));
    if !resource.apply(generation, stage) || failure {
        return None;
    }
    let page = resource.value()?;
    titles.truncate(page_start);
    for title in page {
        if let Some(existing) = titles.iter_mut().find(|t| t.key() == title.key()) {
            *existing = title.clone();
        } else {
            titles.push(title.clone());
        }
    }
    Some(page.is_empty() || page_start > 0 && titles.len() == page_start)
}
#[cfg(test)]
mod tests {
    use super::{BrowseTitle, merge_page};
    use crate::app_state::{Resource, Stage};
    fn title(id: &str, name: &str) -> BrowseTitle {
        use kinema_core::{
            MediaKind,
            catalog::{CatalogTitle, ImdbId},
        };
        let id = ImdbId::new(id).unwrap_or_else(|e| panic!("fixture id: {e}"));
        BrowseTitle::Cinemeta(
            CatalogTitle::new(id, MediaKind::Movie, name, None)
                .unwrap_or_else(|e| panic!("fixture title: {e}")),
        )
    }
    #[test]
    fn refreshed_page_replaces_membership_and_failure_keeps_loaded_titles() {
        let mut resource = Resource::<Vec<BrowseTitle>>::default();
        let a = title("tt0000001", "A");
        let b = title("tt0000002", "B");
        let c = title("tt0000003", "C");
        let mut titles = vec![a.clone()];
        let g = resource.begin(true);
        assert_eq!(
            merge_page(
                &mut resource,
                &mut titles,
                1,
                g,
                Stage::CacheHit {
                    value: vec![b.clone()],
                    fetched_at: 1,
                    remote_follows: true
                }
            ),
            Some(false)
        );
        assert_eq!(titles, vec![a.clone(), b]);
        assert_eq!(
            merge_page(
                &mut resource,
                &mut titles,
                1,
                g,
                Stage::Fetched {
                    value: vec![c.clone()],
                    fetched_at: 2
                }
            ),
            Some(false)
        );
        assert_eq!(titles, vec![a.clone(), c.clone()]);
        let g = resource.begin(false);
        assert_eq!(
            merge_page(
                &mut resource,
                &mut titles,
                0,
                g,
                Stage::Failed("offline".into())
            ),
            None
        );
        assert_eq!(titles, vec![a, c]);
    }
    #[test]
    fn superseded_page_cannot_replace_current_empty_success() {
        let mut resource = Resource::<Vec<BrowseTitle>>::default();
        let mut titles = Vec::new();
        let old = resource.begin(true);
        let current = resource.begin(true);
        assert_eq!(
            merge_page(
                &mut resource,
                &mut titles,
                0,
                old,
                Stage::Failed("obsolete failure".into())
            ),
            None
        );
        assert!(resource.busy());
        assert!(resource.error().is_none());
        assert_eq!(
            merge_page(
                &mut resource,
                &mut titles,
                0,
                current,
                Stage::Fetched {
                    value: vec![],
                    fetched_at: 1
                }
            ),
            Some(true)
        );
        assert!(resource.value().is_some_and(Vec::is_empty));
        assert!(resource.error().is_none());
        assert_eq!(
            merge_page(
                &mut resource,
                &mut titles,
                0,
                old,
                Stage::Failed("late".into())
            ),
            None
        );
    }
    #[test]
    fn refresh_failure_is_not_a_successful_empty_collection() {
        let mut resource = Resource::<Vec<BrowseTitle>>::default();
        let mut titles = Vec::new();
        let g = resource.begin(true);
        assert_eq!(
            merge_page(
                &mut resource,
                &mut titles,
                0,
                g,
                Stage::Failed("vault unavailable".into())
            ),
            None
        );
        assert!(resource.value().is_none());
        assert_eq!(resource.error(), Some("vault unavailable"));
    }
}
