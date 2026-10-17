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
    OpenSettingsAt(SettingsDestination),
    About,
}
impl EventEmitter<CoordinatorEvent> for AppCoordinator {}

/// Public, nonsecret transfer snapshot. URLs here are local server URLs only.
#[derive(Clone, Debug)]
pub struct SessionSnapshot {
    pub session: crate::media::PlaybackSession,
    pub file_name: String,
    pub origin: crate::media::SessionOrigin,
    pub title: Option<String>,
    pub target: Option<PlaybackTarget>,
    pub progress: Option<crate::media::SessionProgress>,
    pub error: Option<String>,
    pub can_reveal: bool,
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
    // ── detail: provider-native season resource and offline response injection ──
    pub(crate) tmdb_season: Resource<crate::tmdb::TmdbSeason>,
    season_request: Option<(TmdbQuery, u64)>,
    #[cfg(feature = "ui-probe")]
    pub(crate) fixture_seasons: std::collections::HashMap<(u64, u32), crate::tmdb::TmdbSeason>,
    #[cfg(feature = "ui-probe")]
    pub(crate) fixture_sources: std::collections::HashMap<(IndexerKind, PlaybackTarget), SourceSet>,
    // ── sessions: preparation and launch identity correlation ──
    session_identity: std::collections::HashMap<u64, (String, PlaybackTarget)>,
    // ── detail: provider-scoped recommendation page ──
    pub(crate) similar: Resource<crate::tmdb::TmdbPage>,
    similar_request: Option<(TmdbQuery, u64)>,
    #[cfg(feature = "ui-probe")]
    pub(crate) fixture_similar: std::collections::HashMap<(u64, u32), crate::tmdb::TmdbPage>,
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
            // ── sessions ──
            session_identity: Default::default(),
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
            // ── detail ──
            similar: Resource::default(),
            similar_request: None,
            #[cfg(feature = "ui-probe")]
            fixture_similar: Default::default(),
            tmdb_season: Resource::default(),
            season_request: None,
            #[cfg(feature = "ui-probe")]
            fixture_seasons: Default::default(),
            #[cfg(feature = "ui-probe")]
            fixture_sources: Default::default(),
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
            // ── sessions ──
            session_identity: Default::default(),
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
            // ── detail ──
            similar: Resource::default(),
            similar_request: None,
            #[cfg(feature = "ui-probe")]
            fixture_similar: Default::default(),
            tmdb_season: Resource::default(),
            season_request: None,
            fixture_seasons: Default::default(),
            fixture_sources: Default::default(),
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
                // ── detail: recommendations cannot consume the browse/season slot ──
                Polled::Tmdb(event)
                    if matches!(
                        event.action,
                        MetadataAction::Tmdb(TmdbQuery::Similar { .. })
                    ) =>
                {
                    self.apply_detail_similar(event);
                }
                // ── detail: season replies have their own identity and generation ──
                Polled::Tmdb(event)
                    if matches!(event.action, MetadataAction::Tmdb(TmdbQuery::Season { .. })) =>
                {
                    self.apply_detail_season(event);
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
            let g = self.similar.generation();
            changed |= self
                .similar
                .failed(g, "Catalog worker stopped. Restart Kinema.");
            let g = self.tmdb_season.generation();
            changed |= self
                .tmdb_season
                .failed(g, "Catalog worker stopped. Restart Kinema.");
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
            AppOutcome::Usage(_) | AppOutcome::SessionUpdates(_) | AppOutcome::Invalidated { .. }
        );
        let resolved = (!unsolicited).then(|| self.tracker.resolve(event.generation));
        match &event.outcome {
            AppOutcome::PlaybackStarted { session, file_name } => {
                self.sessions.retain(|s| s.session.id != session.id);
                self.sessions.push(SessionSnapshot {
                    session: session.clone(),
                    file_name: file_name.clone(),
                    origin: if resolved
                        .as_ref()
                        .is_some_and(|r| matches!(r.pending, Some(Pending::Play { remote: true })))
                    {
                        crate::media::SessionOrigin::Remote
                    } else {
                        crate::media::SessionOrigin::Local
                    },
                    title: self
                        .session_identity
                        .get(&event.generation)
                        .map(|v| v.0.clone()),
                    target: self.session_identity.remove(&event.generation).map(|v| v.1),
                    progress: None,
                    error: None,
                    can_reveal: true,
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
            AppOutcome::SessionUpdates(updates) => {
                for update in updates {
                    if let Some(row) = self.sessions.iter_mut().find(|s| s.session.id == update.id)
                    {
                        row.origin = update.origin;
                        row.progress = update.progress.clone();
                        row.can_reveal = update.can_reveal;
                    }
                }
            }
            AppOutcome::MediaFailed {
                session_id,
                message,
            } => self.session_error(session_id, message),
            AppOutcome::Failed { message } => {
                if let Some(id) = resolved.as_ref().and_then(|r| r.release.as_ref()) {
                    self.session_error(id, message);
                }
            }
            AppOutcome::SessionRelaunched { session_id } => {
                if let Some(row) = self
                    .sessions
                    .iter_mut()
                    .find(|s| s.session.id == *session_id)
                {
                    row.error = None;
                }
            }
            _ => {}
        }
        if !matches!(
            event.outcome,
            AppOutcome::Prepared { .. } | AppOutcome::PlaybackStarted { .. }
        ) {
            self.session_identity.remove(&event.generation);
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
            AppAction::ReplaySession { .. } => Pending::Replay,
            AppAction::RevealSession { .. } => Pending::Reveal,
            _ => Pending::Credential,
        };
        let generation = if let AppAction::ReleaseSession(id) = &action {
            self.tracker.issue_release(id.clone())
        } else if let AppAction::ReplaySession { session_id }
        | AppAction::RevealSession { session_id } = &action
        {
            self.tracker.issue_session(pending, session_id.clone())
        } else {
            self.tracker.issue(pending)
        };
        if let AppAction::PlayFile { preparation, .. } = &action
            && let Some(identity) = self.session_identity.get(preparation).cloned()
        {
            self.session_identity.insert(generation, identity);
        }
        let session_action = match &action {
            AppAction::ReleaseSession(id)
            | AppAction::ReplaySession { session_id: id }
            | AppAction::RevealSession { session_id: id } => Some(id.clone()),
            _ => None,
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
            self.session_identity.remove(&generation);
            if let Some(id) = session_action {
                self.session_error(&id, "Application service unavailable or busy. Retry.");
            }
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
        self.load_title_details(title, refresh, cx);
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
        self.load_detail_sources(indexer, target, false, cx);
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
    /// Route teardown: cancel title-owned metadata/source work, not playback.
    pub fn cancel_title_requests(&mut self, cx: &mut Context<Self>) {
        self.cancel_similar(cx);
        self.cancel_tmdb_season(cx);
        self.cancel_details();
        if let (Ok(worker), Some((ask, generation))) = (&self.worker, self.source_request.take()) {
            metadata_bridge::cancel(worker, ask, generation);
        }
        self.sources.cancel();
        cx.notify();
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

// ── detail: title/source refresh and provider-native lazy seasons ──
impl AppCoordinator {
    fn load_title_details(&mut self, title: &BrowseTitle, refresh: bool, cx: &mut Context<Self>) {
        let action = match title {
            BrowseTitle::Cinemeta(t) => MetadataAction::Details {
                kind: t.kind(),
                id: t.id().clone(),
            },
            BrowseTitle::Tmdb(t) => MetadataAction::Tmdb(TmdbQuery::Details {
                kind: t.kind,
                id: t.id,
            }),
        };
        let same = self
            .detail_request
            .as_ref()
            .is_some_and(|(old, _)| old == &action);
        if !same {
            self.cancel_tmdb_season(cx);
        }
        self.cancel_details();
        let generation = match title {
            BrowseTitle::Cinemeta(_) => self.details.begin(!(refresh && same)),
            BrowseTitle::Tmdb(_) => self.tmdb_details.begin(!(refresh && same)),
        };
        self.detail_request = Some((action.clone(), generation));
        let result = self
            .worker
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|worker| {
                if worker.submit_metadata(MetadataRequest {
                    generation,
                    action,
                    mode: if refresh { Mode::Refresh } else { Mode::Load },
                }) == Submit::Closed
                {
                    Err("Catalog worker stopped".to_owned())
                } else {
                    Ok(())
                }
            });
        if let Err(error) = result {
            match title {
                BrowseTitle::Cinemeta(_) => {
                    self.details.failed(generation, error);
                }
                BrowseTitle::Tmdb(_) => {
                    self.tmdb_details.failed(generation, error);
                }
            }
        }
        cx.notify();
    }

    /// Explicit source refresh bypasses cache freshness, preserving saved rows on failure.
    pub fn refresh_sources(
        &mut self,
        indexer: IndexerKind,
        target: PlaybackTarget,
        cx: &mut Context<Self>,
    ) {
        self.load_detail_sources(indexer, target, true, cx);
    }

    fn load_detail_sources(
        &mut self,
        indexer: IndexerKind,
        target: PlaybackTarget,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let ask = Ask::Sources {
            indexer,
            target: target.clone(),
        };
        let same = self
            .source_request
            .as_ref()
            .is_some_and(|(old, _)| old == &ask);
        if let (Ok(worker), Some((old, generation))) = (&self.worker, self.source_request.take()) {
            metadata_bridge::cancel(worker, old, generation);
        }
        let generation = self.sources.begin(!(refresh && same));
        self.source_request = Some((ask.clone(), generation));
        #[cfg(feature = "ui-probe")]
        if self.fixture.is_some() {
            if let Some(value) = self.fixture_sources.get(&(indexer, target)).cloned() {
                self.sources.fetched(generation, value, 1);
            } else {
                self.sources.failed(
                    generation,
                    "No source response injected for this offline fixture.",
                );
            }
            cx.notify();
            return;
        }
        let result = self
            .worker
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|worker| metadata_bridge::request(worker, ask, generation, refresh));
        if let Err(error) = result {
            self.sources.failed(generation, error);
        }
        cx.notify();
    }

    /// One selected season. Changing identity cancels obsolete work; refresh retains data.
    pub fn load_tmdb_season(
        &mut self,
        series_id: u64,
        season: u32,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let query = TmdbQuery::Season { series_id, season };
        let same = self
            .season_request
            .as_ref()
            .is_some_and(|(old, _)| old == &query);
        if same && !refresh && (self.tmdb_season.busy() || self.tmdb_season.value().is_some()) {
            return;
        }
        self.cancel_tmdb_season(cx);
        let generation = self.tmdb_season.begin(!(refresh && same));
        self.season_request = Some((query.clone(), generation));
        #[cfg(feature = "ui-probe")]
        if self.fixture.is_some() {
            if let Some(value) = self.fixture_seasons.get(&(series_id, season)).cloned() {
                self.tmdb_season.fetched(generation, value, 1);
            } else {
                self.tmdb_season.failed(
                    generation,
                    "No season response injected for this offline fixture.",
                );
            }
            cx.notify();
            return;
        }
        let result = self
            .worker
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|worker| {
                if worker.submit_metadata(MetadataRequest {
                    generation,
                    action: MetadataAction::Tmdb(query),
                    mode: if refresh { Mode::Refresh } else { Mode::Load },
                }) == Submit::Closed
                {
                    Err("Catalog worker stopped".to_owned())
                } else {
                    Ok(())
                }
            });
        if let Err(error) = result {
            self.tmdb_season.failed(generation, error);
        }
        cx.notify();
    }

    pub fn cancel_tmdb_season(&mut self, cx: &mut Context<Self>) {
        if let (Ok(worker), Some((query, generation))) = (&self.worker, self.season_request.take())
        {
            worker.cancel_metadata(&MetadataAction::Tmdb(query), generation);
        } else {
            self.season_request = None;
        }
        self.tmdb_season.cancel();
        cx.notify();
    }

    fn apply_detail_season(&mut self, event: crate::catalog_worker::MetadataEvent) -> bool {
        let Some((query, generation)) = &self.season_request else {
            return false;
        };
        if event.action != MetadataAction::Tmdb(query.clone()) || event.generation != *generation {
            return false;
        }
        let stage = match event.result {
            Ok(MetadataSnapshot::Tmdb(snapshot)) => match snapshot.data {
                TmdbData::Season(value)
                    if *query
                        == (TmdbQuery::Season {
                            series_id: value.series_id,
                            season: value.season,
                        }) =>
                {
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
                _ => Stage::Failed("Catalog returned a different season.".into()),
            },
            Err(error) => Stage::Failed(error),
            _ => Stage::Failed("Catalog returned an unexpected season response.".into()),
        };
        self.tmdb_season.apply(event.generation, stage)
    }
}

// ── sessions: playback identity, cancellation, and settings destinations ──
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsDestination {
    General,
    Account(kinema_core::sources::DebridKind),
    Storage,
}
impl AppCoordinator {
    pub fn prepare(
        &mut self,
        title: String,
        target: PlaybackTarget,
        candidate: kinema_core::sources::SourceCandidate,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let identity = (title, target.clone());
        let generation = self.command(AppAction::Prepare { target, candidate }, cx)?;
        // An admitted preparation supersedes the worker's retained preparation.
        // Keep identity still needed by outstanding launches/commands.
        self.session_identity
            .retain(|generation, _| self.tracker.is_pending(*generation));
        self.session_identity.insert(generation, identity);
        Some(generation)
    }
    pub fn cancel_preparation(&mut self, preparation: u64, cx: &mut Context<Self>) -> bool {
        self.tracker.cancel(preparation);
        self.session_identity.remove(&preparation);
        let canceled = self
            .app
            .as_ref()
            .is_ok_and(|a| a.cancel_request(preparation));
        cx.notify();
        canceled
    }
    pub fn cancel_launch(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        // Keep identity until the terminal event: a launch may already exist.
        self.tracker.cancel(generation);
        let canceled = self
            .app
            .as_ref()
            .is_ok_and(|a| a.cancel_request(generation));
        cx.notify();
        canceled
    }
    pub fn open_settings_at(&mut self, destination: SettingsDestination, cx: &mut Context<Self>) {
        cx.emit(CoordinatorEvent::OpenSettingsAt(destination));
    }
    fn session_error(&mut self, id: &str, message: &str) {
        if let Some(row) = self.sessions.iter_mut().find(|s| s.session.id == id) {
            row.error = Some(message.into());
        }
    }
}

// ── quit: workerless pending-launch interaction fixture ──
#[cfg(all(test, feature = "ui-probe-test"))]
impl AppCoordinator {
    pub(crate) fn fixture_pending_launch(&mut self, cx: &mut Context<Self>) {
        self.tracker.issue(Pending::Play { remote: false });
        cx.notify();
    }
}

// ── sessions: offline coordinator failure and launch-race tests ──
#[cfg(all(test, feature = "ui-probe-test"))]
mod session_tests {
    use super::{AppCoordinator, AppEvent, AppOutcome, ImdbId, Pending, PlaybackTarget};
    use crate::media::{PlaybackSession, SessionOrigin, SessionProgress, SessionUpdate};
    use gpui::AppContext;
    #[test]
    fn late_launch_is_accounted_and_session_failures_survive_telemetry() {
        let mut cx = gpui::TestAppContext::single();
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        coordinator.update(&mut cx, |c, cx| {
            let generation = c.tracker.issue(Pending::Play { remote: true });
            let target = PlaybackTarget::Movie(
                ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("fixture: {e}")),
            );
            c.session_identity
                .insert(generation, ("Fixture title".into(), target.clone()));
            c.cancel_launch(generation, cx);
            c.apply_app(
                AppEvent {
                    generation,
                    outcome: AppOutcome::PlaybackStarted {
                        session: PlaybackSession {
                            id: "late".into(),
                            url: "http://127.0.0.1:1234/media/late".into(),
                        },
                        file_name: "film.mkv".into(),
                    },
                },
                cx,
            );
            assert!(c.needs_quit_confirmation());
            assert_eq!(c.sessions.len(), 1);
            assert_eq!(c.sessions[0].title.as_deref(), Some("Fixture title"));
            assert_eq!(c.sessions[0].target.as_ref(), Some(&target));
            c.apply_app(
                AppEvent {
                    generation: 0,
                    outcome: AppOutcome::MediaFailed {
                        session_id: "late".into(),
                        message: "Acquisition failed".into(),
                    },
                },
                cx,
            );
            c.apply_app(
                AppEvent {
                    generation: 0,
                    outcome: AppOutcome::SessionUpdates(vec![SessionUpdate {
                        id: "late".into(),
                        origin: SessionOrigin::Remote,
                        progress: Some(SessionProgress {
                            downloaded_bytes: 12,
                            total_bytes: 100,
                        }),
                        can_reveal: true,
                    }]),
                },
                cx,
            );
            assert_eq!(c.sessions[0].error.as_deref(), Some("Acquisition failed"));
            assert_eq!(
                c.sessions[0].progress.as_ref().map(|p| p.downloaded_bytes),
                Some(12)
            );
            let release = c.tracker.issue_release("late".into());
            c.apply_app(
                AppEvent {
                    generation: release,
                    outcome: AppOutcome::Failed {
                        message: "Stop failed".into(),
                    },
                },
                cx,
            );
            assert_eq!(c.sessions[0].error.as_deref(), Some("Stop failed"));
            assert!(c.needs_quit_confirmation());
            let replay = c.tracker.issue_session(Pending::Replay, "late".into());
            c.apply_app(
                AppEvent {
                    generation: replay,
                    outcome: AppOutcome::Failed {
                        message: "Player unavailable".into(),
                    },
                },
                cx,
            );
            assert_eq!(c.sessions.len(), 1);
            assert_eq!(c.sessions[0].error.as_deref(), Some("Player unavailable"));
            c.apply_app(
                AppEvent {
                    generation: 0,
                    outcome: AppOutcome::Released { id: "late".into() },
                },
                cx,
            );
            assert!(!c.needs_quit_confirmation());
        });
    }
}

// ── detail: provider-scoped movie recommendations ──
impl AppCoordinator {
    #[cfg(feature = "ui-probe")]
    pub(crate) fn detail_is_fixture(&self) -> bool {
        self.fixture.is_some()
    }

    pub fn load_similar(&mut self, id: u64, page: u32, refresh: bool, cx: &mut Context<Self>) {
        let query = TmdbQuery::Similar { id, page };
        let same = self
            .similar_request
            .as_ref()
            .is_some_and(|(q, _)| q == &query);
        if same && !refresh && (self.similar.busy() || self.similar.value().is_some()) {
            return;
        }
        self.cancel_similar(cx);
        let generation = self.similar.begin(!(refresh && same));
        self.similar_request = Some((query.clone(), generation));
        #[cfg(feature = "ui-probe")]
        if self.fixture.is_some() {
            if let Some(value) = self.fixture_similar.get(&(id, page)).cloned() {
                self.similar.fetched(generation, value, 1);
            } else {
                self.similar.failed(
                    generation,
                    "No similar-title page injected for this offline fixture.",
                );
            }
            cx.notify();
            return;
        }
        let result = self.worker.as_ref().map_err(Clone::clone).and_then(|w| {
            if w.submit_metadata(MetadataRequest {
                generation,
                action: MetadataAction::Tmdb(query),
                mode: if refresh { Mode::Refresh } else { Mode::Load },
            }) == Submit::Closed
            {
                Err("Catalog worker stopped".into())
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            self.similar.failed(generation, error);
        }
        cx.notify();
    }
    pub fn cancel_similar(&mut self, cx: &mut Context<Self>) {
        if let (Ok(worker), Some((query, generation))) = (&self.worker, self.similar_request.take())
        {
            worker.cancel_metadata(&MetadataAction::Tmdb(query), generation);
        } else {
            self.similar_request = None;
        }
        self.similar.cancel();
        cx.notify();
    }
    fn apply_detail_similar(&mut self, event: crate::catalog_worker::MetadataEvent) -> bool {
        let Some((query, generation)) = &self.similar_request else {
            return false;
        };
        if event.generation != *generation || event.action != MetadataAction::Tmdb(query.clone()) {
            return false;
        }
        let stage = match event.result {
            Ok(MetadataSnapshot::Tmdb(snapshot)) => match snapshot.data {
                TmdbData::Page(value)
                    if matches!(query,TmdbQuery::Similar{page,..} if *page==value.page)
                        && value.titles.iter().all(|t| t.kind == MediaKind::Movie) =>
                {
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
                _ => Stage::Failed("Catalog returned an unexpected similar-title page.".into()),
            },
            Err(e) => Stage::Failed(e),
            _ => Stage::Failed("Catalog returned an unexpected similar-title response.".into()),
        };
        self.similar.apply(event.generation, stage)
    }
}

// ── detail: shared Browse append-state integration requested by grid-polish ──
impl AppCoordinator {
    /// Both providers store zero-based offsets internally. TMDB sends offset+1
    /// on the wire; its second page therefore has internal offset 1, not 2.
    pub(crate) fn append_in_flight(&self) -> bool {
        self.offset > 0 && self.collection.busy() && !self.exhausted
    }
    #[cfg(feature = "ui-probe")]
    pub(crate) fn fixture_pending_append(&mut self, cx: &mut Context<Self>) {
        self.offset = if self.provider() == MetadataProvider::Tmdb {
            1
        } else {
            100
        };
        self.exhausted = false;
        self.collection.begin(false);
        cx.notify();
    }
}

// ── detail: actual coordinator failure/stale-routing tests, without services ──
#[cfg(all(test, feature = "ui-probe-test"))]
mod detail_request_tests {
    use super::{
        AppCoordinator, BrowseTitle, MetadataAction, MetadataProvider, MetadataSnapshot, TmdbData,
        TmdbQuery,
    };
    use crate::{
        catalog::CachedTmdb,
        catalog_worker::MetadataEvent,
        metadata_bridge::SourceSet,
        tmdb::{TmdbPage, TmdbSeason},
    };
    use gpui::{AppContext, TestAppContext};
    use kinema_core::{
        MediaKind,
        catalog::{CatalogTitle, ImdbId, TitleDetails},
        sources::{IndexerKind, PlaybackTarget},
    };
    fn season(id: u64, n: u32) -> TmdbSeason {
        TmdbSeason {
            series_id: id,
            season: n,
            poster_url: None,
            episodes: vec![],
        }
    }
    fn reply(generation: u64, id: u64, n: u32) -> MetadataEvent {
        MetadataEvent {
            generation,
            action: MetadataAction::Tmdb(TmdbQuery::Season {
                series_id: id,
                season: n,
            }),
            result: Ok(MetadataSnapshot::Tmdb(CachedTmdb {
                data: TmdbData::Season(season(id, n)),
                fetched_at: 1,
            })),
            saved: false,
            refreshing: false,
        }
    }
    #[test]
    fn append_offsets_are_zero_based_for_both_providers() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        for (provider, offset) in [
            (MetadataProvider::Cinemeta, 100),
            (MetadataProvider::Tmdb, 1),
        ] {
            c.update(&mut cx, |c, cx| {
                c.settings
                    .as_mut()
                    .unwrap_or_else(|| panic!("settings"))
                    .metadata = provider;
                c.offset = 0;
                c.exhausted = false;
                c.collection.begin(false);
                assert!(!c.append_in_flight());
                c.offset = offset;
                assert!(c.append_in_flight());
                c.exhausted = true;
                assert!(!c.append_in_flight());
                c.exhausted = false;
                let g = c.collection.generation();
                c.collection.failed(g, "offline");
                assert!(!c.append_in_flight());
                c.fixture_pending_append(cx);
                assert!(c.append_in_flight());
            });
        }
    }
    #[test]
    fn season_switch_and_cancel_reject_late_results_and_refresh_retains_saved() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            c.fixture_seasons.insert((91, 1), season(91, 1));
            c.fixture_seasons.insert((91, 2), season(91, 2));
            c.load_tmdb_season(91, 1, false, cx);
            let old = c.tmdb_season.generation();
            c.load_tmdb_season(91, 2, false, cx);
            assert!(!c.apply_detail_season(reply(old, 91, 1)));
            assert_eq!(c.tmdb_season.value().map(|s| s.season), Some(2));
            c.fixture_seasons.remove(&(91, 2));
            c.load_tmdb_season(91, 2, true, cx);
            assert!(c.tmdb_season.error().is_some());
            assert_eq!(c.tmdb_season.value().map(|s| s.season), Some(2));
            let g = c.tmdb_season.generation();
            c.cancel_tmdb_season(cx);
            assert!(!c.apply_detail_season(reply(g, 91, 2)));
        });
    }
    #[test]
    fn source_and_title_refresh_failures_preserve_only_same_subject() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            let id = ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id:{e}"));
            let target = PlaybackTarget::Movie(id.clone());
            c.fixture_sources.insert(
                (IndexerKind::Torrentio, target.clone()),
                SourceSet {
                    candidates: vec![],
                    omitted: 7,
                },
            );
            c.sources(IndexerKind::Torrentio, target.clone(), cx);
            c.fixture_sources.clear();
            c.refresh_sources(IndexerKind::Torrentio, target.clone(), cx);
            assert_eq!(c.sources.value().map(|s| s.omitted), Some(7));
            assert!(c.sources.error().is_some());
            c.sources(IndexerKind::Peerflix, target, cx);
            assert!(c.sources.value().is_none());
            let title = CatalogTitle::new(id, MediaKind::Movie, "Saved title", None)
                .unwrap_or_else(|e| panic!("title:{e}"));
            let browse = BrowseTitle::Cinemeta(title.clone());
            c.title_details(&browse, false, cx);
            let g = c.details.begin(true);
            c.details.fetched(
                g,
                TitleDetails {
                    title,
                    poster_url: None,
                    background_url: None,
                    release_info: None,
                    genres: vec![],
                    episodes: vec![],
                },
                1,
            );
            c.title_details(&browse, true, cx);
            assert!(c.details.error().is_some());
            assert!(c.details.value().is_some());
            let other = BrowseTitle::Cinemeta(
                CatalogTitle::new(
                    ImdbId::new("tt7654321").unwrap_or_else(|e| panic!("id:{e}")),
                    MediaKind::Movie,
                    "Other",
                    None,
                )
                .unwrap_or_else(|e| panic!("title:{e}")),
            );
            c.title_details(&other, true, cx);
            assert!(c.details.value().is_none());
        });
    }
    #[test]
    fn similar_page_switch_rejects_late_and_failed_refresh_retains_page() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            c.fixture_similar.insert(
                (91, 1),
                TmdbPage {
                    titles: vec![],
                    page: 1,
                    total_pages: 2,
                },
            );
            c.fixture_similar.insert(
                (91, 2),
                TmdbPage {
                    titles: vec![],
                    page: 2,
                    total_pages: 2,
                },
            );
            c.load_similar(91, 1, false, cx);
            let old = c.similar.generation();
            c.load_similar(91, 2, false, cx);
            let mut event = reply(old, 91, 1);
            event.action = MetadataAction::Tmdb(TmdbQuery::Similar { id: 91, page: 1 });
            assert!(!c.apply_detail_similar(event));
            c.fixture_similar.clear();
            c.load_similar(91, 2, true, cx);
            assert_eq!(c.similar.value().map(|p| p.page), Some(2));
            assert!(c.similar.error().is_some());
        });
    }
}

// ── detail: column-only cancellation must not interrupt title refresh ──
impl AppCoordinator {
    pub(crate) fn cancel_detail_sources(&mut self, cx: &mut Context<Self>) {
        if let (Ok(worker), Some((ask, generation))) = (&self.worker, self.source_request.take()) {
            metadata_bridge::cancel(worker, ask, generation);
        }
        self.sources.cancel();
        cx.notify();
    }
}
