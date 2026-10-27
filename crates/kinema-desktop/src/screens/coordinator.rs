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
mod personal;
use personal::PersonalStore;
#[allow(
    unused_imports,
    reason = "UI contract exports named DTOs even when callers infer their types"
)]
pub(crate) use personal::{
    EpisodePersonalView, EpisodeRef, LocalBrowseTitle, LocalDetail, PersonalEpisode, PersonalLoad,
    PersonalTitle, PersonalView, SeasonSnapshot, TitleRef,
};

use crate::{
    app::{AppAction, AppCommand, AppEvent, AppOutcome, AppWorker, CredentialState},
    app_state::{Resource, Stage},
    app_tracker::{Pending, Tracker},
    catalog_worker::{
        CatalogWorker, EnrichMode, MetadataAction, MetadataEvent, MetadataRequest,
        MetadataSnapshot, Mode, Outcome, Poll, Request, Submit,
    },
    metadata_bridge::{self, Ask, MetaEvent, Polled, SourceSet},
    screens::enrichment,
    settings::Settings,
    tmdb::{TmdbData, TmdbDetails, TmdbQuery, TmdbTitle},
};
use gpui::*;
use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, ImdbId, SearchQuery, TitleDetails},
    discover::DiscoverFilters,
    provider::ProviderId,
    sources::{IndexerKind, PlaybackTarget},
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BrowseTitle {
    Local(LocalBrowseTitle),
    Cinemeta(CatalogTitle),
    Tmdb(TmdbTitle),
}
impl BrowseTitle {
    pub(crate) fn personal_ref(&self) -> TitleRef {
        personal::title_ref(self)
    }
    pub fn key(&self) -> String {
        match self {
            Self::Local(t) => format!("canonical:{}", t.title_id.get()),
            Self::Cinemeta(t) => format!("{:?}:{}", t.kind(), t.id().as_str()),
            Self::Tmdb(t) => format!("{:?}:tmdb:{}", t.kind, t.id),
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Local(t) => t.metadata.text("name").unwrap_or("Untitled"),
            Self::Cinemeta(t) => t.name(),
            Self::Tmdb(t) => &t.name,
        }
    }
    pub fn kind(&self) -> MediaKind {
        match self {
            Self::Local(t) => t.metadata.kind,
            Self::Cinemeta(t) => t.kind(),
            Self::Tmdb(t) => t.kind,
        }
    }
    pub fn year(&self) -> Option<u16> {
        match self {
            Self::Local(t) => t
                .metadata
                .integer("release_year")
                .or_else(|| t.metadata.integer("first_air_year"))
                .and_then(|v| u16::try_from(v).ok()),
            Self::Cinemeta(t) => t.year(),
            Self::Tmdb(t) => t.year(),
        }
    }
    pub fn rating(&self) -> Option<f32> {
        match self {
            Self::Local(t) => t
                .metadata
                .ratings
                .iter()
                .find(|rating| {
                    rating.observation.source == "imdb" && rating.observation.metric == "score"
                })
                .and_then(|rating| rating.observation.value)
                .map(|value| value as f32),
            Self::Cinemeta(t) => t.imdb_rating(),
            Self::Tmdb(t) => t.imdb_rating(),
        }
    }
    pub fn poster_url(&self) -> Option<&str> {
        match self {
            Self::Local(t) => t.metadata.text("poster_url"),
            Self::Cinemeta(t) => t.poster_url(),
            Self::Tmdb(t) => t.poster_url(),
        }
    }
    pub fn overview(&self) -> Option<&str> {
        match self {
            Self::Local(t) => t.metadata.text("overview"),
            Self::Cinemeta(t) => t.overview(),
            Self::Tmdb(t) => t.overview.as_deref(),
        }
    }
    pub fn genres(&self) -> &[String] {
        match self {
            Self::Local(t) => &t.genres,
            Self::Cinemeta(t) => t.genres(),
            Self::Tmdb(t) => t.genres(),
        }
    }
    pub fn runtime(&self) -> Option<u16> {
        match self {
            Self::Local(t) => t
                .metadata
                .integer("runtime_minutes")
                .and_then(|v| u16::try_from(v).ok()),
            Self::Cinemeta(t) => t.runtime_minutes(),
            Self::Tmdb(t) => t.runtime_minutes(),
        }
    }
    pub fn seasons(&self) -> Option<u32> {
        match self {
            Self::Local(t) => t
                .metadata
                .integer("season_count")
                .and_then(|v| u32::try_from(v).ok()),
            Self::Cinemeta(t) => t.season_count(),
            Self::Tmdb(t) => t.season_count(),
        }
    }
    pub fn imdb(&self) -> Option<&ImdbId> {
        match self {
            Self::Local(t) => t.imdb.as_ref(),
            Self::Cinemeta(t) => Some(t.id()),
            Self::Tmdb(t) => t.imdb.as_ref(),
        }
    }
}
// Only normalized public quality buckets may be logged, never a provider label.
pub(crate) fn diagnostic_quality(value: Option<&str>) -> &'static str {
    match value {
        Some("2160p") => "2160p",
        Some("1080p") => "1080p",
        Some("720p") => "720p",
        Some("480p") => "480p",
        _ => "unknown",
    }
}
fn apply_stage<T>(
    name: &'static str,
    resource: &mut Resource<T>,
    generation: u64,
    stage: Stage<T>,
) -> bool {
    let stage_name = match &stage {
        Stage::CacheHit { .. } => "cache_hit",
        Stage::Fetched { .. } => "fetched",
        Stage::Failed(_) => "failed",
    };
    let current_generation = resource.generation();
    let applied = resource.apply(generation, stage);
    tracing::debug!(
        resource = name,
        generation,
        current_generation,
        stage = stage_name,
        outcome = if applied { "applied" } else { "rejected_stale" },
        "UI resource result received"
    );
    if applied && let Some(message) = resource.error() {
        // Resource errors originate from user-safe domain enums/static UI notices.
        tracing::warn!(resource = name, message, "UI resource failure displayed");
    }
    applied
}
impl BrowseTitle {
    pub(crate) fn log_navigation(&self, route: &'static str) {
        // key() is built only from a validated IMDb ID or a numeric TMDB ID.
        tracing::debug!(route, kind = ?self.kind(), id = self.key(), "UI title navigation");
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

/// The in-flight (or settled) Details enrichment for the open title.
struct DetailEnrichment {
    /// `None` when nothing can be requested (no IMDb ID or no active enricher).
    action: Option<MetadataAction>,
    generation: u64,
    source: ProviderId,
    contributors: Vec<ProviderId>,
    /// Last completed canonical record, re-applied after the primary is re-stored.
    overlay: Option<TitleDetails>,
}

pub struct AppCoordinator {
    personal_state: PersonalStore,
    pub(crate) library: Resource<Vec<BrowseTitle>>,
    pub(crate) library_titles: Vec<BrowseTitle>,
    pub(crate) library_has_more: bool,
    pub(crate) local_details: Resource<LocalDetail>,
    pub(crate) season_snapshot: Resource<SeasonSnapshot>,
    pub(crate) personal_notice: Option<String>,
    worker: Result<CatalogWorker, String>,
    app: Result<AppWorker, String>,
    tracker: Tracker,
    pub settings: Option<Settings>,
    pub tmdb_credential: Option<CredentialState>,
    pub credentials: Vec<(kinema_core::sources::DebridKind, CredentialState)>,
    pub usage: Option<crate::media::UsageSnapshot>,
    pub sessions: Vec<SessionSnapshot>,
    embedded_sessions: std::collections::HashSet<String>,
    // Tombstones live only until release acknowledgement and queued replays drain.
    closed_embedded_sessions: std::collections::HashMap<String, bool>,
    pending_embedded_releases: std::collections::HashSet<String>,
    embedded_replays: std::collections::HashMap<u64, String>,
    pub(crate) collection: Resource<Vec<BrowseTitle>>,
    pub(crate) titles: Vec<BrowseTitle>,
    pub(crate) details: Resource<TitleDetails>,
    pub(crate) sources: Resource<SourceSet>,
    pub(crate) tmdb_details: Resource<TmdbDetails>,
    pub kind: MediaKind,
    pub query: String,
    discover_filters: DiscoverFilters,
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
    // ── enrichment: optional, quiet, generation-checked ──
    detail_enrichment: Option<DetailEnrichment>,
    detail_enrich_generation: u64,
    card_enrichment: Option<(MetadataAction, u64)>,
    card_generation: u64,
    card_pending: Vec<ImdbId>,
    card_last: Vec<ImdbId>,
    card_done: Option<(Vec<ImdbId>, u64)>,
    card_task: Option<Task<()>>,
    #[cfg(feature = "ui-probe")]
    pub(crate) fixture_art: std::collections::HashMap<String, std::sync::Arc<RenderImage>>,
    worker_disconnect_reported: [bool; 2],
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
    rates: kinema_core::transfer_rate::RateTracker,
    rate_epoch: std::time::Instant,
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
            worker: CatalogWorker::spawn_default().map_err(|error| { tracing::error!(worker = "catalog", io_kind = ?error.kind(), "UI worker could not start"); error.to_string() }),
            app: AppWorker::spawn().map_err(|error| { tracing::error!(worker = "application", io_kind = ?error.kind(), "UI worker could not start"); error.to_string() }),
            tracker: Tracker::default(),
            personal_state: PersonalStore::default(),
            library: Resource::default(),
            library_titles: Vec::new(),
            library_has_more: false,
            local_details: Resource::default(),
            season_snapshot: Resource::default(),
            personal_notice: None,
            // ── sessions ──
            session_identity: Default::default(),
            rates: Default::default(),
            rate_epoch: std::time::Instant::now(),
            settings: None,
            tmdb_credential: None,
            credentials: Vec::new(),
            usage: None,
            sessions: Vec::new(),
            embedded_sessions: Default::default(),
            closed_embedded_sessions: Default::default(),
            pending_embedded_releases: Default::default(),
            embedded_replays: Default::default(),
            collection: Resource::default(),
            titles: Vec::new(),
            details: Resource::default(),
            sources: Resource::default(),
            tmdb_details: Resource::default(),
            kind: MediaKind::Movie,
            query: String::new(),
            discover_filters: DiscoverFilters::default(),
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
            // ── enrichment ──
            detail_enrichment: None,
            detail_enrich_generation: 0,
            card_enrichment: None,
            card_generation: 0,
            card_pending: Vec::new(),
            card_last: Vec::new(),
            card_done: None,
            card_task: None,
            #[cfg(feature = "ui-probe")]
            fixture_art: Default::default(),
            worker_disconnect_reported: [false; 2],
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
            personal_state: PersonalStore::default(),
            library: Resource::default(),
            library_titles: Vec::new(),
            library_has_more: false,
            local_details: Resource::default(),
            season_snapshot: Resource::default(),
            personal_notice: None,
            // ── sessions ──
            session_identity: Default::default(),
            rates: Default::default(),
            rate_epoch: std::time::Instant::now(),
            settings: Some(Settings::default()),
            tmdb_credential: None,
            credentials: Vec::new(),
            usage: None,
            sessions: Vec::new(),
            embedded_sessions: Default::default(),
            closed_embedded_sessions: Default::default(),
            pending_embedded_releases: Default::default(),
            embedded_replays: Default::default(),
            collection,
            titles: titles.clone(),
            details: Resource::default(),
            sources: Resource::default(),
            tmdb_details: Resource::default(),
            kind: MediaKind::Movie,
            query: String::new(),
            discover_filters: DiscoverFilters::default(),
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
            // ── enrichment ──
            detail_enrichment: None,
            detail_enrich_generation: 0,
            card_enrichment: None,
            card_generation: 0,
            card_pending: Vec::new(),
            card_last: Vec::new(),
            card_done: None,
            card_task: None,
            fixture_art: Default::default(),
            worker_disconnect_reported: [false; 2],
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
    pub fn discover_filters(&self) -> &DiscoverFilters {
        &self.discover_filters
    }
    /// Facets apply only to TMDB Discover, never search or Cinemeta.
    pub fn discover_filters_apply(&self) -> bool {
        self.query.trim().is_empty() && self.provider() == ProviderId::Tmdb
    }
    pub fn set_discover_filters(&mut self, mut filters: DiscoverFilters, cx: &mut Context<Self>) {
        filters.retain_genres_for(self.kind);
        if !filters.is_valid_for(self.kind) || self.discover_filters == filters {
            return;
        }
        self.discover_filters = filters;
        // Cancel against the old resource generation before replacing it.
        self.cancel_collection();
        self.reload_collection(cx);
    }
    fn cancel_collection(&mut self) {
        if let (Ok(worker), Some((ask, generation))) = (&self.worker, self.request.take()) {
            metadata_bridge::cancel(worker, ask, generation);
        }
        if let Ok(worker) = &self.worker {
            worker.cancel_search(self.kind, self.collection.generation());
            if let Some(query) = self.tmdb_request.take() {
                worker.cancel_metadata(&MetadataAction::Tmdb(query), self.collection.generation());
            }
        }
    }
    pub fn load(&mut self, kind: MediaKind, query: String, refresh: bool, cx: &mut Context<Self>) {
        tracing::debug!(
            ?kind,
            refresh,
            searching = !query.trim().is_empty(),
            "UI collection load requested"
        );
        self.discover_filters.retain_genres_for(kind);
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
                apply_stage(
                    "collection",
                    &mut self.collection,
                    generation,
                    Stage::Failed(error.to_string()),
                );
            } else {
                self.collection.fetched(generation, self.titles.clone(), 0);
            }
            cx.notify();
            return;
        }
        if self.settings.is_none() {
            return;
        }
        self.cancel_collection();
        let replace = kind != self.kind || query != self.query;
        self.kind = kind;
        self.query = query;
        self.offset = 0;
        self.exhausted = false;
        if replace {
            self.titles.clear();
            self.reset_card_enrichment();
        }
        self.fetch(replace, refresh, cx);
    }
    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        if self.collection.busy()
            || self.collection.error().is_some()
            || self.exhausted
            || !self.query.is_empty() && self.provider() == ProviderId::Cinemeta
        {
            return;
        }
        self.offset += if self.provider() == ProviderId::Tmdb {
            1
        } else {
            100
        };
        self.fetch(true, false, cx);
    }
    /// Discards the grid and loads it again from the effective primary provider.
    fn reload_collection(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.collection = Resource::with_base(self.generation * 1_000_000);
        self.titles.clear();
        self.reset_card_enrichment();
        self.load(self.kind, self.query.clone(), false, cx);
    }
    /// A changed token can change which provider is primary: reload only then.
    pub(crate) fn apply_credential_status(
        &mut self,
        status: CredentialState,
        cx: &mut Context<Self>,
    ) {
        let before = self.provider();
        self.tmdb_credential = Some(status);
        if self.provider() != before {
            self.reload_collection(cx);
        }
        self.refresh_detail_enrichment();
    }
    /// The effective primary provider: first in priority order that is active.
    pub(crate) fn provider(&self) -> ProviderId {
        let tmdb_active = matches!(self.tmdb_credential, Some(CredentialState::Present));
        self.settings
            .as_ref()
            .map_or(ProviderId::Cinemeta, |s| s.primary_provider(tmdb_active))
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
            if self.provider() == ProviderId::Tmdb {
                let query = if self.query.trim().is_empty() {
                    if self.discover_filters.is_default() {
                        TmdbQuery::Browse {
                            kind: self.kind,
                            page: self.offset + 1,
                        }
                    } else {
                        TmdbQuery::Discover {
                            kind: self.kind,
                            page: self.offset + 1,
                            filters: self.discover_filters.clone(),
                        }
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
            apply_stage(
                "collection",
                &mut self.collection,
                generation,
                Stage::Failed(e.to_string()),
            );
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
            // Page values replaced the shown cards; enrichment must run again.
            self.card_done = None;
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
        let canonical_changed = search.iter().any(|event| {
            matches!(
                event.outcome,
                Outcome::Fetched(_) | Outcome::CacheHit { .. }
            )
        }) || metadata.iter().any(|event| match event {
            Polled::Cinemeta(MetaEvent::Sources { .. }) => false,
            Polled::Cinemeta(MetaEvent::Browse { stage, .. }) => !matches!(stage, Stage::Failed(_)),
            Polled::Cinemeta(MetaEvent::Details { stage, .. }) => {
                !matches!(stage, Stage::Failed(_))
            }
            Polled::Tmdb(event) | Polled::Enrichment(event) => event.result.is_ok(),
        });
        let mut changed = self.drain_personal(cx) || !search.is_empty() || !metadata.is_empty();
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
            } else {
                tracing::debug!(generation = event.generation, kind = ?event.kind, outcome = "rejected_stale", "UI search result belongs to an inactive media kind");
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
                    let usable = !matches!(stage, Stage::Failed(_));
                    if apply_stage("details", &mut self.details, generation, stage) && usable {
                        self.start_detail_enrichment();
                    }
                }
                Polled::Cinemeta(MetaEvent::Sources {
                    generation, stage, ..
                }) => {
                    apply_stage("sources", &mut self.sources, generation, stage);
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
                    let usable = !matches!(stage, Stage::Failed(_));
                    if apply_stage(
                        "tmdb_details",
                        &mut self.tmdb_details,
                        event.generation,
                        stage,
                    ) && usable
                    {
                        self.start_detail_enrichment();
                    }
                }
                // ── enrichment: quiet; never a primary-view failure ──
                Polled::Enrichment(event) => self.apply_enrichment(event),
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
                _ => tracing::debug!(
                    outcome = "rejected_stale",
                    "UI metadata result no longer matches the active route"
                ),
            }
        }
        if canonical_changed {
            self.refresh_personal_after_catalog(cx);
        }
        if self.card_done.is_none()
            && self.card_pending.is_empty()
            && self.card_task.is_none()
            && !self.card_last.is_empty()
            && !self.collection.busy()
        {
            // A page replaced the shown cards: enrich the visible set again.
            let ids = self.card_last.clone();
            self.note_visible_titles(ids, cx);
        }
        if stopped {
            if !self.worker_disconnect_reported[0] {
                tracing::error!(worker = "catalog", "UI worker disconnected");
                self.worker_disconnect_reported[0] = true;
            }
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
            if !self.worker_disconnect_reported[1] {
                tracing::error!(worker = "application", "UI worker disconnected");
                self.worker_disconnect_reported[1] = true;
            }
            let message = "Application worker stopped. Restart Kinema.";
            changed |= self.notice.as_deref() != Some(message);
            if self.notice.as_deref() != Some(message) {
                tracing::warn!(message, "UI notice displayed");
            }
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
        self.retry_embedded_releases(cx);
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
        self.embedded_replays.remove(&event.generation);
        if let AppOutcome::EmbeddedPlaybackStarted { session, .. } = &event.outcome
            && self.closed_embedded_sessions.contains_key(&session.id)
        {
            self.session_identity.remove(&event.generation);
            self.prune_embedded_tombstones();
            return;
        }
        match &event.outcome {
            AppOutcome::PlaybackStarted { session, file_name }
            | AppOutcome::EmbeddedPlaybackStarted {
                session, file_name, ..
            } => {
                // Replay keeps the original route-independent metadata/progress.
                if resolved
                    .as_ref()
                    .is_some_and(|r| r.pending == Some(Pending::Replay))
                {
                    if let Some(row) = self
                        .sessions
                        .iter_mut()
                        .find(|s| s.session.id == session.id)
                    {
                        row.error = None;
                    }
                } else {
                    self.sessions.retain(|s| s.session.id != session.id);
                    self.sessions.push(SessionSnapshot {
                        session: session.clone(),
                        file_name: file_name.clone(),
                        origin: if resolved.as_ref().is_some_and(|r| {
                            matches!(r.pending, Some(Pending::Play { remote: true }))
                        }) {
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
                        resolved.as_ref().is_some_and(|r| {
                            matches!(r.pending, Some(Pending::Play { remote: true }))
                        }),
                    );
                }
            }
            AppOutcome::Released { id } => {
                self.close_embedded_session(id, cx);
                self.embedded_release_acknowledged(id);
                self.tracker.session_released(id);
                self.sessions.retain(|s| s.session.id != *id);
            }
            AppOutcome::Invalidated { released } => {
                for id in released {
                    self.close_embedded_session(id, cx);
                    self.embedded_release_acknowledged(id);
                    self.tracker.session_released(id);
                    self.sessions.retain(|s| s.session.id != *id);
                }
            }
            AppOutcome::Usage(usage) => self.usage = Some(usage.clone()),
            AppOutcome::SessionUpdates(updates) => {
                let now_ms = self.rate_epoch.elapsed().as_millis() as u64;
                for update in updates {
                    if let Some(row) = self.sessions.iter_mut().find(|s| s.session.id == update.id)
                    {
                        row.origin = update.origin;
                        row.progress = update.progress.clone();
                        row.can_reveal = update.can_reveal;
                        if let Some(progress) = &update.progress {
                            self.rates
                                .observe(&update.id, progress.downloaded_bytes, now_ms);
                        }
                    }
                }
                let sessions = &self.sessions;
                self.rates
                    .retain(|id| sessions.iter().any(|s| s.session.id == id));
            }
            AppOutcome::MediaFailed {
                session_id,
                message,
            } => self.session_error(session_id, message),
            AppOutcome::Failed { message } => {
                if let Some(id) = resolved.as_ref().and_then(|r| r.release.as_ref()) {
                    self.session_error(id, message);
                    if resolved
                        .as_ref()
                        .is_some_and(|r| r.pending == Some(Pending::Release))
                        && self.closed_embedded_sessions.contains_key(id)
                    {
                        self.pending_embedded_releases.insert(id.clone());
                    }
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
            AppOutcome::Prepared { .. }
                | AppOutcome::PlaybackStarted { .. }
                | AppOutcome::EmbeddedPlaybackStarted { .. }
        ) {
            self.session_identity.remove(&event.generation);
        }
        self.prune_embedded_tombstones();
        if let AppOutcome::EmbeddedPlaybackStarted {
            session,
            file_name,
            mode,
        } = &event.outcome
        {
            if resolved.as_ref().is_some_and(|r| !r.current) {
                // Unlike an external process, obsolete embedded work need not be shown.
                self.queue_embedded_release(session.id.clone(), cx);
            } else {
                self.embedded_sessions.insert(session.id.clone());
                let title = self
                    .sessions
                    .iter()
                    .find(|s| s.session.id == session.id)
                    .and_then(|s| s.title.clone())
                    .unwrap_or_else(|| file_name.clone());
                if crate::screens::player_window::open(
                    session.id.clone(),
                    mode.clone(),
                    session.url.clone(),
                    title,
                    cx.entity(),
                    cx,
                )
                .is_err()
                {
                    // Backend/window errors must not expose the capability URL.
                    self.notice = Some("Embedded player could not be opened.".into());
                    self.queue_embedded_release(session.id.clone(), cx);
                }
            }
        }
        if resolved.as_ref().is_some_and(|r| !r.current) {
            tracing::debug!(
                generation = event.generation,
                outcome = "rejected_stale",
                "UI application result not applied to current view; session bookkeeping retained"
            );
            return;
        }
        if !unsolicited {
            tracing::debug!(
                generation = event.generation,
                outcome = "applied",
                "UI application result applied"
            );
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
                let before = self.provider();
                let reload = self
                    .settings
                    .as_ref()
                    .is_none_or(|s| s.metadata_priority != settings.metadata_priority);
                self.settings = Some(settings.clone());
                self.credentials = credentials.clone();
                self.tmdb_credential = Some(tmdb_credential.clone());
                if reload || self.provider() != before {
                    self.reload_collection(cx);
                }
                self.refresh_detail_enrichment();
            }
            AppOutcome::MetadataCredential { status } => {
                self.apply_credential_status(status.clone(), cx);
            }
            AppOutcome::Failed { message } => {
                tracing::warn!(message, "UI notice displayed");
                self.notice = Some(message.clone());
                if resolved
                    .as_ref()
                    .is_some_and(|r| r.pending == Some(Pending::Load))
                {
                    let g = self.collection.generation();
                    apply_stage(
                        "collection",
                        &mut self.collection,
                        g,
                        Stage::Failed(message.to_string()),
                    );
                }
            }
            _ => {}
        }
        if let AppOutcome::SettingsWarning { message, .. } = &event.outcome {
            tracing::warn!(message, "UI notice displayed");
            self.notice = Some(message.clone());
        }
        cx.emit(CoordinatorEvent::App(event));
    }
    fn close_embedded_session(&mut self, session_id: &str, cx: &mut Context<Self>) {
        if self.embedded_sessions.remove(session_id) {
            self.closed_embedded_sessions
                .insert(session_id.to_owned(), false);
            crate::screens::player_window::close_session(session_id, cx);
        }
    }

    fn embedded_release_acknowledged(&mut self, session_id: &str) {
        self.pending_embedded_releases.remove(session_id);
        if let Some(acknowledged) = self.closed_embedded_sessions.get_mut(session_id) {
            *acknowledged = true;
        }
    }

    fn prune_embedded_tombstones(&mut self) {
        self.closed_embedded_sessions.retain(|id, acknowledged| {
            !*acknowledged || self.embedded_replays.values().any(|pending| pending == id)
        });
    }

    fn queue_embedded_release(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.embedded_sessions.remove(&session_id);
        self.closed_embedded_sessions
            .entry(session_id.clone())
            .or_insert(false);
        self.pending_embedded_releases.insert(session_id);
        self.retry_embedded_releases(cx);
    }

    // Native close/fatal failure cannot be dropped like a lossy user command.
    // The retained set is bounded by outstanding leases, not playback history.
    fn retry_embedded_releases(&mut self, cx: &mut Context<Self>) {
        if !self.app.as_ref().is_ok_and(|app| !app.is_stopped()) {
            return;
        }
        for id in self
            .pending_embedded_releases
            .iter()
            .cloned()
            .collect::<Vec<_>>()
        {
            let notice = self.notice.clone();
            let accepted = self
                .command(AppAction::ReleaseSession(id.clone()), cx)
                .is_some();
            // Retrying terminal cleanup must not replace a useful fatal-player
            // notice with a transient mailbox-busy warning.
            if notice.is_some() {
                self.notice = notice;
            }
            if accepted {
                self.pending_embedded_releases.remove(&id);
            } else {
                break;
            }
        }
    }

    /// A window close releases only its own lease, at most once. Never navigates
    /// or quits; queued replay completions cannot resurrect this closed window.
    pub fn embedded_player_closed(&mut self, session_id: String, cx: &mut Context<Self>) {
        if !self.embedded_sessions.contains(&session_id) {
            return;
        }
        self.queue_embedded_release(session_id, cx);
        cx.notify();
    }

    /// Fatal initialization/render failure. Only static UI text is surfaced;
    /// backend diagnostics and capability URLs never enter notices or logs.
    pub fn embedded_player_failed(&mut self, session_id: String, cx: &mut Context<Self>) {
        if !self.embedded_sessions.contains(&session_id) {
            return;
        }
        self.embedded_player_closed(session_id, cx);
        self.notice = Some("Embedded playback failed. Try another player in Settings.".into());
        cx.notify();
    }

    /// Typed application intent gateway. Returns its command generation, or None
    /// on backpressure/start failure. P2/P4 should subscribe before submitting.
    pub fn command(&mut self, action: AppAction, cx: &mut Context<Self>) -> Option<u64> {
        tracing::debug!(operation = action.operation(), "UI user action requested");
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
        let release_id = if let AppAction::ReleaseSession(id) = &action {
            Some(id.clone())
        } else {
            None
        };
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
            if pending == Pending::Replay
                && let Some(id) = &session_action
            {
                self.embedded_replays.insert(generation, id.clone());
            }
            if let Some(id) = release_id {
                self.close_embedded_session(&id, cx);
            }
            cx.notify();
            Some(generation)
        } else {
            self.tracker.withdraw(generation);
            self.session_identity.remove(&generation);
            if let Some(id) = session_action {
                self.session_error(&id, "Application service unavailable or busy. Retry.");
            }
            let message = "Application service unavailable or busy. Retry.";
            if self.notice.as_deref() != Some(message) {
                tracing::warn!(message, "UI notice displayed");
            }
            self.notice = Some(message.into());
            if pending == Pending::Load {
                let g = self.collection.generation();
                apply_stage(
                    "collection",
                    &mut self.collection,
                    g,
                    Stage::Failed(message.to_string()),
                );
            }
            cx.notify();
            None
        }
    }
    fn cancel_details(&mut self) {
        self.cancel_detail_enrichment();
        if let (Ok(worker), Some((action, generation))) = (&self.worker, self.detail_request.take())
        {
            worker.cancel_metadata(&action, generation);
        }
        self.details.cancel();
        self.tmdb_details.cancel();
        self.cancel_local_detail();
        self.local_details.cancel();
        self.season_snapshot.cancel();
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
            apply_stage(
                "details",
                &mut self.details,
                generation,
                Stage::Failed(error.to_string()),
            );
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
    /// Toolbar summary of held sessions: idle, serving-only or downloading.
    pub fn transfer_summary(&self) -> kinema_core::transfer_rate::TransferSummary {
        use kinema_core::transfer_rate::{SessionTally, TransferSummary};
        TransferSummary::from_sessions(self.sessions.iter().map(|s| {
            SessionTally {
                remote: s.origin == crate::media::SessionOrigin::Remote,
                progress: s
                    .progress
                    .as_ref()
                    .map(|p| (p.downloaded_bytes, p.total_bytes)),
            }
        }))
    }
    /// Smoothed combined download speed, sampled when telemetry arrives.
    pub fn transfer_bytes_per_second(&self) -> u64 {
        self.rates
            .total_bytes_per_second(self.rate_epoch.elapsed().as_millis() as u64)
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
    if !apply_stage("collection", resource, generation, stage) || failure {
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
    fn ui_diagnostics_redact_title_values_and_reject_stale_errors() {
        use crate::providers::test_logging::{capture, excludes, isolated};
        if !isolated(
            module_path!(),
            "ui_diagnostics_redact_title_values_and_reject_stale_errors",
        ) {
            return;
        }
        let mut resource = Resource::<String>::default();
        let old = resource.begin(true);
        let current = resource.begin(true);
        let (_, output) = capture(|| {
            title(
                "tt0000001",
                "https://private.invalid/?signature=title-secret",
            )
            .log_navigation("detail");
            tracing::debug!(
                quality = super::diagnostic_quality(Some(
                    "https://private.invalid/?token=quality-secret"
                )),
                "UI stream fixture"
            );
            assert!(!super::apply_stage(
                "fixture",
                &mut resource,
                old,
                Stage::Failed("https://private.invalid/?token=stale-error-secret".into())
            ));
            assert!(super::apply_stage(
                "fixture",
                &mut resource,
                current,
                Stage::Fetched {
                    value: "https://cdn.invalid/?token=value-secret".into(),
                    fetched_at: 1
                }
            ));
            let generation = resource.begin(false);
            assert!(super::apply_stage(
                "fixture",
                &mut resource,
                generation,
                Stage::Failed("Catalog worker unavailable".into())
            ));
        });
        assert!(output.contains("rejected_stale"));
        assert!(output.contains("applied"));
        assert!(output.contains("Catalog worker unavailable"));
        excludes(
            &output,
            &[
                "title-secret",
                "quality-secret",
                "stale-error-secret",
                "value-secret",
                "signature=",
                "token=",
            ],
        );
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
        title.log_navigation("details_request");
        let action = match title {
            BrowseTitle::Local(local) => {
                self.remember_local_title(local);
                match self.local_detail_action(local) {
                    Some(action) => action,
                    None => {
                        self.cancel_tmdb_season(cx);
                        // No provider route follows to replace the old season
                        // resource. Clear its value/error without resetting its
                        // generation, so late replies still remain stale.
                        self.tmdb_season.begin(true);
                        self.tmdb_season.cancel();
                        self.load_local_details(local.title_id, cx);
                        return;
                    }
                }
            }
            BrowseTitle::Cinemeta(title) => MetadataAction::Details {
                kind: title.kind(),
                id: title.id().clone(),
            },
            BrowseTitle::Tmdb(title) => MetadataAction::Tmdb(TmdbQuery::Details {
                kind: title.kind,
                id: title.id,
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
        let tmdb = matches!(action, MetadataAction::Tmdb(_));
        let generation = if tmdb {
            self.tmdb_details.begin(!(refresh && same))
        } else {
            self.details.begin(!(refresh && same))
        };
        self.detail_request = Some((action.clone(), generation));
        if let BrowseTitle::Local(local) = title {
            self.begin_local_projection(local, !(refresh && same), cx);
        }
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
            if tmdb {
                apply_stage(
                    "tmdb_details",
                    &mut self.tmdb_details,
                    generation,
                    Stage::Failed(error),
                );
            } else {
                apply_stage(
                    "details",
                    &mut self.details,
                    generation,
                    Stage::Failed(error),
                );
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
            apply_stage(
                "sources",
                &mut self.sources,
                generation,
                Stage::Failed(error.to_string()),
            );
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
            apply_stage(
                "tmdb_season",
                &mut self.tmdb_season,
                generation,
                Stage::Failed(error.to_string()),
            );
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
            tracing::debug!(
                generation = event.generation,
                outcome = "rejected_stale",
                resource = "apply_detail_season",
                "UI metadata result rejected by identity or generation"
            );
            return false;
        };
        if event.action != MetadataAction::Tmdb(query.clone()) || event.generation != *generation {
            tracing::debug!(
                generation = event.generation,
                outcome = "rejected_stale",
                resource = "apply_detail_season",
                "UI metadata result rejected by identity or generation"
            );
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
        apply_stage(
            "tmdb_season",
            &mut self.tmdb_season,
            event.generation,
            stage,
        )
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
        tracing::debug!(
            ?target,
            quality = diagnostic_quality(candidate.quality.as_deref()),
            "UI preparation requested"
        );
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
        tracing::warn!(message, "UI session error displayed");
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
    fn embedded_terminal_close_retries_backpressure_and_prunes_after_replay_drains() {
        let mut cx = gpui::TestAppContext::single();
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        coordinator.update(&mut cx, |c, cx| {
            c.app = Ok(crate::app::AppWorker::test_mailbox());
            c.embedded_sessions.insert("one".into());
            let replay = c.command(crate::app::AppAction::ReplaySession { session_id: "one".into() }, cx)
                .unwrap_or_else(|| panic!("fixture replay rejected"));
            for _ in 0..100 {
                if c.command(crate::app::AppAction::LoadSettings, cx).is_none() { break; }
            }
            c.embedded_player_failed("one".into(), cx);
            assert!(c.pending_embedded_releases.contains("one"));
            c.embedded_player_closed("one".into(), cx);
            assert_eq!(c.pending_embedded_releases.len(), 1);
            c.app.as_ref().unwrap_or_else(|_| panic!("fixture worker")).test_drain_requests();
            c.retry_embedded_releases(cx);
            assert!(c.pending_embedded_releases.is_empty());
            let requests = c.app.as_ref().unwrap_or_else(|_| panic!("fixture worker")).test_drain_requests();
            assert_eq!(requests.len(), 1);
            assert!(matches!(&requests[0].action, crate::app::AppAction::ReleaseSession(id) if id == "one"));
            c.apply_app(AppEvent { generation: requests[0].generation, outcome: AppOutcome::Released { id: "one".into() } }, cx);
            assert!(c.closed_embedded_sessions.contains_key("one"));
            c.apply_app(AppEvent { generation: replay, outcome: AppOutcome::EmbeddedPlaybackStarted {
                session: PlaybackSession { id: "one".into(), url: "http://127.0.0.1:1234/private-capability".into() },
                file_name: "fixture.mkv".into(),
                mode: crate::player::PlayerChoice::Embedded,
            } }, cx);
            assert!(c.closed_embedded_sessions.is_empty());
            assert!(c.embedded_replays.is_empty());
            assert!(c.embedded_sessions.is_empty());
            assert!(c.sessions.is_empty());
        });
    }

    #[test]
    fn embedded_close_is_matching_idempotent_and_rejects_queued_replay() {
        let mut cx = gpui::TestAppContext::single();
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        coordinator.update(&mut cx, |c, cx| {
            c.embedded_sessions.extend(["one".into(), "two".into()]);
            c.tracker.session_started("one".into(), false);
            c.tracker.session_started("two".into(), false);
            c.embedded_player_closed("unknown".into(), cx);
            assert_eq!(c.embedded_sessions.len(), 2);
            c.embedded_player_closed("one".into(), cx);
            assert!(!c.embedded_sessions.contains("one"));
            assert!(c.embedded_sessions.contains("two"));
            let notice = c.notice.clone();
            c.embedded_player_closed("one".into(), cx);
            assert_eq!(c.notice, notice);
            let generation = c.tracker.issue_session(Pending::Replay, "one".into());
            c.apply_app(
                AppEvent {
                    generation,
                    outcome: AppOutcome::EmbeddedPlaybackStarted {
                        session: PlaybackSession {
                            id: "one".into(),
                            url: "http://127.0.0.1:1234/private-capability".into(),
                        },
                        file_name: "fixture.mkv".into(),
                        mode: crate::player::PlayerChoice::Embedded,
                    },
                },
                cx,
            );
            assert!(c.sessions.is_empty());
            assert!(!c.embedded_sessions.contains("one"));
            c.apply_app(
                AppEvent {
                    generation: 0,
                    outcome: AppOutcome::Released { id: "one".into() },
                },
                cx,
            );
            assert_eq!(c.tracker.active_sessions(), 1);
            c.embedded_player_failed("two".into(), cx);
            assert_eq!(
                c.notice.as_deref(),
                Some("Embedded playback failed. Try another player in Settings.")
            );
            c.embedded_player_failed("two".into(), cx);
            assert_eq!(
                c.notice.as_deref(),
                Some("Embedded playback failed. Try another player in Settings.")
            );
            assert!(!c.quit_requested);
            c.apply_app(
                AppEvent {
                    generation: 0,
                    outcome: AppOutcome::Released { id: "two".into() },
                },
                cx,
            );
            assert_eq!(c.tracker.active_sessions(), 0);
        });
    }

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

    pub fn load_similar(
        &mut self,
        kind: MediaKind,
        id: u64,
        page: u32,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let query = TmdbQuery::Similar { kind, id, page };
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
            apply_stage(
                "similar",
                &mut self.similar,
                generation,
                Stage::Failed(error.to_string()),
            );
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
            tracing::debug!(
                generation = event.generation,
                outcome = "rejected_stale",
                resource = "apply_detail_similar",
                "UI metadata result rejected by identity or generation"
            );
            return false;
        };
        if event.generation != *generation || event.action != MetadataAction::Tmdb(query.clone()) {
            tracing::debug!(
                generation = event.generation,
                outcome = "rejected_stale",
                resource = "apply_detail_similar",
                "UI metadata result rejected by identity or generation"
            );
            return false;
        }
        let stage = match event.result {
            Ok(MetadataSnapshot::Tmdb(snapshot)) => match snapshot.data {
                TmdbData::Page(value)
                    if matches!(query,TmdbQuery::Similar{kind,page,..} if *page==value.page
                        && value.titles.iter().all(|t| t.kind == *kind)) =>
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
        apply_stage("similar", &mut self.similar, event.generation, stage)
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
        self.offset = if self.provider() == ProviderId::Tmdb {
            1
        } else {
            100
        };
        self.exhausted = false;
        self.collection.begin(false);
        cx.notify();
    }
}

// ── enrichment: details overlay, visible-card batches and attribution ──
impl AppCoordinator {
    fn provider_active(&self, id: ProviderId) -> bool {
        id != ProviderId::Tmdb || matches!(self.tmdb_credential, Some(CredentialState::Present))
    }
    /// Active providers in priority order, excluding the one that supplied the view.
    pub(crate) fn enrichers_for(&self, source: ProviderId) -> Vec<ProviderId> {
        self.settings.as_ref().map_or_else(Vec::new, |s| {
            s.metadata_priority
                .order()
                .iter()
                .copied()
                .filter(|id| *id != source && self.provider_active(*id))
                .collect()
        })
    }
    /// Provider, kind and IMDb ID of the open detail, when it can be enriched.
    fn detail_identity(&self) -> Option<(ProviderId, MediaKind, Option<ImdbId>)> {
        match &self.detail_request.as_ref()?.0 {
            MetadataAction::Details { kind, id } => {
                Some((ProviderId::Cinemeta, *kind, Some(id.clone())))
            }
            MetadataAction::Tmdb(TmdbQuery::Details { kind, .. }) => Some((
                ProviderId::Tmdb,
                *kind,
                self.tmdb_details.value().and_then(|d| d.imdb.clone()),
            )),
            _ => None,
        }
    }
    /// Runs after the primary details are shown or when the active enrichers
    /// change. Submits only when the (title, enrichers) request differs from the
    /// one already in flight or completed; a re-shown primary just gets the
    /// completed overlay applied again.
    fn start_detail_enrichment(&mut self) {
        let Some((source, kind, imdb)) = self.detail_identity() else {
            return;
        };
        let providers = self.enrichers_for(source);
        let Some(settings) = self.settings.as_ref() else {
            return;
        };
        let action = enrichment::detail_action(kind, imdb, providers, &settings.metadata_priority);
        let same = self
            .detail_enrichment
            .as_ref()
            .is_some_and(|e| e.source == source && e.action == action);
        if same {
            self.reapply_detail_overlay();
            return;
        }
        let kept = self.detail_enrichment.take();
        if let Some(old) = &kept
            && let (Some(old_action), Ok(worker)) = (&old.action, &self.worker)
        {
            worker.cancel_metadata(old_action, old.generation);
        }
        // Data already merged into the view stays attributed after a request ends.
        let (contributors, overlay) = kept
            .filter(|e| e.source == source)
            .map_or((vec![source], None), |e| (e.contributors, e.overlay));
        let Some(action) = action else {
            self.detail_enrichment = Some(DetailEnrichment {
                action: None,
                generation: 0,
                source,
                contributors,
                overlay,
            });
            return;
        };
        self.detail_enrich_generation += 1;
        let generation = self.detail_enrich_generation;
        if let Ok(worker) = &self.worker
            && worker.submit_metadata(MetadataRequest {
                generation,
                action: action.clone(),
                mode: Mode::Refresh,
            }) == Submit::Closed
        {
            return;
        }
        self.detail_enrichment = Some(DetailEnrichment {
            action: Some(action),
            generation,
            source,
            contributors,
            overlay,
        });
        self.reapply_detail_overlay();
    }
    /// Re-merges the completed overlay into a freshly stored primary value.
    fn reapply_detail_overlay(&mut self) {
        let Some(state) = &self.detail_enrichment else {
            return;
        };
        let Some(overlay) = &state.overlay else {
            return;
        };
        match state.source {
            ProviderId::Cinemeta => {
                if let Some(d) = self.details.value_mut() {
                    enrichment::merge_cinemeta_details(d, overlay);
                }
            }
            ProviderId::Tmdb => {
                if let Some(d) = self.tmdb_details.value_mut() {
                    enrichment::merge_tmdb_details(d, overlay);
                }
            }
        }
    }
    /// The enricher set changed (token saved/removed): refresh an open details page.
    fn refresh_detail_enrichment(&mut self) {
        let shown = self.details.value().is_some() || self.tmdb_details.value().is_some();
        if shown && self.detail_request.is_some() {
            self.start_detail_enrichment();
        }
    }
    fn cancel_detail_enrichment(&mut self) {
        if let Some(old) = self.detail_enrichment.take()
            && let Some(action) = old.action
            && let Ok(worker) = &self.worker
        {
            worker.cancel_metadata(&action, old.generation);
        }
    }
    fn apply_enrichment(&mut self, event: MetadataEvent) {
        let mode = match &event.action {
            MetadataAction::Enrich { mode, .. } => *mode,
            _ => return,
        };
        let current = match mode {
            EnrichMode::Details => self
                .detail_enrichment
                .as_ref()
                .and_then(|e| Some((e.action.as_ref()?, e.generation))),
            EnrichMode::CardFields => self.card_enrichment.as_ref().map(|(a, g)| (a, *g)),
        };
        if !enrichment::is_current(current, &event.action, event.generation) {
            tracing::debug!(
                generation = event.generation,
                outcome = "rejected_stale",
                "UI enrichment result no longer matches the active request"
            );
            return;
        }
        let snapshot = match event.result {
            Ok(MetadataSnapshot::Enrichment(snapshot)) => snapshot,
            Ok(_) => return,
            Err(_) => {
                // Optional data: the primary view stays exactly as it is.
                tracing::debug!(?mode, "UI enrichment unavailable");
                return;
            }
        };
        tracing::debug!(
            ?mode,
            titles = snapshot.titles.len(),
            failed = snapshot.failed.len(),
            inactive = snapshot.inactive.len(),
            "UI enrichment applied"
        );
        match mode {
            EnrichMode::Details => {
                let Some(state) = self.detail_enrichment.as_mut() else {
                    return;
                };
                let Some(MetadataAction::Enrich { imdb, .. }) = &state.action else {
                    return;
                };
                let Some(found) = snapshot
                    .titles
                    .into_iter()
                    .find(|t| imdb.first() == Some(t.title.id()))
                else {
                    return;
                };
                let source = state.source;
                state.contributors = found.contributors.clone();
                if !state.contributors.contains(&source) {
                    state.contributors.push(source);
                }
                state.overlay = Some(found.details.details);
                self.reapply_detail_overlay();
            }
            EnrichMode::CardFields => {
                for found in snapshot.titles {
                    for card in &mut self.titles {
                        if card.imdb() == Some(found.title.id())
                            && let Some(filled) = enrichment::fill_card(card, &found.title)
                        {
                            *card = filled;
                        }
                    }
                }
            }
        }
    }
    /// Offline fixture: pretend both providers contributed to the shown details.
    #[cfg(feature = "ui-probe")]
    pub(crate) fn fixture_enrichment_contributors(&mut self) {
        if let Some((source, ..)) = self.detail_identity() {
            self.detail_enrichment = Some(DetailEnrichment {
                action: None,
                generation: 0,
                source,
                contributors: vec![ProviderId::Tmdb, ProviderId::Cinemeta],
                overlay: None,
            });
        }
    }
    /// "Data from …" for the open details; at least the primary once it is shown.
    pub(crate) fn detail_attribution(&self) -> Option<String> {
        let settings = self.settings.as_ref()?;
        let (source, ..) = self.detail_identity()?;
        let shown = match source {
            ProviderId::Cinemeta => self.details.value().is_some(),
            ProviderId::Tmdb => self.tmdb_details.value().is_some(),
        };
        if !shown {
            return None;
        }
        let known = self
            .detail_enrichment
            .as_ref()
            .map_or_else(|| vec![source], |e| e.contributors.clone());
        kinema_core::provider::attribution(&known, &settings.metadata_priority)
    }
    /// Debounced entry point for the grid: a changed visible set is batched after
    /// it rests, so scrolling never submits a request per frame.
    pub(crate) fn note_visible_titles(&mut self, ids: Vec<ImdbId>, cx: &mut Context<Self>) {
        let ids = enrichment::visible_batch(ids);
        if ids.is_empty() {
            return;
        }
        self.card_last.clone_from(&ids);
        let generation = self.collection.generation();
        if self.card_pending == ids
            || self
                .card_done
                .as_ref()
                .is_some_and(|(done, at)| *done == ids && *at == generation)
        {
            return;
        }
        self.card_pending = ids;
        self.card_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(enrichment::CARD_DEBOUNCE)
                .await;
            let _ = this.update(cx, |this, _| this.submit_card_enrichment());
        }));
    }
    fn submit_card_enrichment(&mut self) {
        self.card_task = None;
        let ids = std::mem::take(&mut self.card_pending);
        if ids.is_empty() {
            return;
        }
        self.cancel_card_enrichment();
        let providers = self.enrichers_for(self.provider());
        let Some(settings) = self.settings.as_ref() else {
            return;
        };
        let Some(action) = enrichment::card_action(
            self.kind,
            ids.clone(),
            providers,
            &settings.metadata_priority,
        ) else {
            return;
        };
        self.card_generation += 1;
        let generation = self.card_generation;
        if let Ok(worker) = &self.worker
            && worker.submit_metadata(MetadataRequest {
                generation,
                action: action.clone(),
                mode: Mode::Refresh,
            }) == Submit::Closed
        {
            return;
        }
        self.card_enrichment = Some((action, generation));
        self.card_done = Some((ids, self.collection.generation()));
    }
    fn cancel_card_enrichment(&mut self) {
        if let Some((action, generation)) = self.card_enrichment.take()
            && let Ok(worker) = &self.worker
        {
            worker.cancel_metadata(&action, generation);
        }
    }
    /// The subject changed: drop pending and in-flight card work.
    fn reset_card_enrichment(&mut self) {
        self.card_task = None;
        self.card_pending.clear();
        self.card_last.clear();
        self.card_done = None;
        self.cancel_card_enrichment();
    }
}

// ── detail: actual coordinator failure/stale-routing tests, without services ──
#[cfg(all(test, feature = "ui-probe-test"))]
mod detail_request_tests {
    use super::{
        AppCoordinator, BrowseTitle, MetadataAction, MetadataSnapshot, ProviderId, TmdbData,
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
        for (provider, offset) in [(ProviderId::Cinemeta, 100), (ProviderId::Tmdb, 1)] {
            c.update(&mut cx, |c, cx| {
                c.tmdb_credential = Some(crate::app::CredentialState::Present);
                c.settings
                    .as_mut()
                    .unwrap_or_else(|| panic!("settings"))
                    .metadata_priority = kinema_core::provider::ProviderPriority::new([provider]);
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
                    facts: Default::default(),
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
            c.load_similar(MediaKind::Movie, 91, 1, false, cx);
            let old = c.similar.generation();
            c.load_similar(MediaKind::Movie, 91, 2, false, cx);
            let mut event = reply(old, 91, 1);
            event.action = MetadataAction::Tmdb(TmdbQuery::Similar {
                kind: MediaKind::Movie,
                id: 91,
                page: 1,
            });
            assert!(!c.apply_detail_similar(event));
            c.fixture_similar.clear();
            c.load_similar(MediaKind::Movie, 91, 2, true, cx);
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

// ── enrichment: coordinator-level behavior without services ──
#[cfg(all(test, feature = "ui-probe-test"))]
mod enrichment_tests {
    use super::*;
    use crate::{
        catalog::CachedDetails,
        catalog_worker::{EnrichedTitle, EnrichmentSnapshot},
    };
    use ::core::prelude::v1::test;
    use gpui::{AppContext, TestAppContext};

    fn id(n: u32) -> ImdbId {
        ImdbId::new(format!("tt{n:07}")).unwrap_or_else(|e| panic!("{e}"))
    }
    fn title(n: u32) -> CatalogTitle {
        CatalogTitle::new(id(n), MediaKind::Movie, "Name", None).unwrap_or_else(|e| panic!("{e}"))
    }
    fn details(n: u32) -> TitleDetails {
        TitleDetails {
            facts: Default::default(),
            title: title(n),
            poster_url: None,
            background_url: None,
            release_info: None,
            genres: vec![],
            episodes: vec![],
        }
    }
    fn open(c: &mut AppCoordinator, n: u32, cx: &mut Context<AppCoordinator>) {
        c.cancel_details();
        let action = MetadataAction::Details {
            kind: MediaKind::Movie,
            id: id(n),
        };
        let g = c.details.begin(true);
        c.detail_request = Some((action, g));
        c.details.fetched(g, details(n), 1);
        c.start_detail_enrichment();
        cx.notify();
    }
    fn reply(action: MetadataAction, generation: u64, n: u32, tagline: &str) -> MetadataEvent {
        let mut shown = details(n);
        shown.facts.tagline = Some(tagline.into());
        MetadataEvent {
            generation,
            action,
            result: Ok(MetadataSnapshot::Enrichment(EnrichmentSnapshot {
                titles: vec![EnrichedTitle {
                    title: title(n),
                    fields: Default::default(),
                    details: CachedDetails {
                        details: shown,
                        fetched_at: 2,
                    },
                    contributors: vec![ProviderId::Tmdb, ProviderId::Cinemeta],
                }],
                failed: vec![],
                inactive: vec![],
            })),
            saved: false,
            refreshing: false,
        }
    }
    fn current(c: &AppCoordinator) -> (MetadataAction, u64) {
        let e = c
            .detail_enrichment
            .as_ref()
            .unwrap_or_else(|| panic!("enrichment"));
        (
            e.action.clone().unwrap_or_else(|| panic!("action")),
            e.generation,
        )
    }

    #[test]
    fn details_enrichment_follows_title_and_ignores_stale_results() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            c.tmdb_credential = Some(CredentialState::Present);
            open(c, 1, cx);
            let (first, g1) = current(c);
            assert_eq!(
                c.detail_attribution().as_deref(),
                Some("Data from Cinemeta")
            );
            // Navigating to another title cancels and replaces the request.
            open(c, 2, cx);
            let (second, g2) = current(c);
            assert!(g2 > g1);
            // A late reply for the previous title never touches the new view.
            c.apply_enrichment(reply(first, g1, 1, "stale"));
            assert!(c.details.value().is_some_and(|d| d.facts.tagline.is_none()));
            assert_eq!(
                c.detail_attribution().as_deref(),
                Some("Data from Cinemeta")
            );
            // The current reply updates facts in place and the footer.
            c.apply_enrichment(reply(second.clone(), g2, 2, "fresh"));
            assert_eq!(
                c.details.value().and_then(|d| d.facts.tagline.as_deref()),
                Some("fresh")
            );
            assert_eq!(
                c.detail_attribution().as_deref(),
                Some("Data from Cinemeta and TMDB")
            );
            // Same generation, wrong action: ignored.
            c.apply_enrichment(reply(second, g2 + 1, 2, "wrong"));
            assert_eq!(
                c.details.value().and_then(|d| d.facts.tagline.as_deref()),
                Some("fresh")
            );
            c.cancel_details();
            assert!(c.detail_enrichment.is_none());
            assert_eq!(c.detail_attribution(), None);
        });
    }

    #[test]
    fn refetched_primary_does_not_resubmit_and_keeps_overlay() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            c.tmdb_credential = Some(CredentialState::Present);
            open(c, 1, cx);
            let (action, g) = current(c);
            c.apply_enrichment(reply(action, g, 1, "merged"));
            // A fetched stage after the cached one re-stores the primary value.
            let gen_details = c.details.begin(false);
            c.details.fetched(gen_details, details(1), 5);
            assert!(c.details.value().is_some_and(|d| d.facts.tagline.is_none()));
            c.start_detail_enrichment();
            assert_eq!(c.detail_enrich_generation, g);
            assert_eq!(
                c.details.value().and_then(|d| d.facts.tagline.as_deref()),
                Some("merged")
            );
        });
    }

    #[test]
    fn token_change_reruns_details_enrichment_with_new_generation() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            c.tmdb_credential = Some(CredentialState::Missing);
            open(c, 1, cx);
            assert_eq!(c.detail_enrich_generation, 0);
            assert_eq!(
                c.detail_attribution().as_deref(),
                Some("Data from Cinemeta")
            );
            c.apply_credential_status(CredentialState::Present, cx);
            let (action, g) = current(c);
            assert_eq!(g, 1);
            c.apply_enrichment(reply(action, g, 1, "tmdb"));
            assert_eq!(
                c.detail_attribution().as_deref(),
                Some("Data from Cinemeta and TMDB")
            );
            // Removing the token ends the request but keeps truthful attribution.
            c.apply_credential_status(CredentialState::Missing, cx);
            assert!(
                c.detail_enrichment
                    .as_ref()
                    .is_some_and(|e| e.action.is_none())
            );
            assert_eq!(
                c.detail_attribution().as_deref(),
                Some("Data from Cinemeta and TMDB")
            );
            assert_eq!(
                c.details.value().and_then(|d| d.facts.tagline.as_deref()),
                Some("tmdb")
            );
        });
    }

    #[test]
    fn no_active_enricher_keeps_primary_only_footer() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            c.tmdb_credential = Some(CredentialState::Missing);
            open(c, 1, cx);
            assert!(
                c.detail_enrichment
                    .as_ref()
                    .is_some_and(|e| e.action.is_none())
            );
            assert_eq!(
                c.detail_attribution().as_deref(),
                Some("Data from Cinemeta")
            );
        });
    }

    #[test]
    fn card_enrichment_fills_gaps_and_rejects_stale() {
        let mut cx = TestAppContext::single();
        let c =
            cx.new(|cx| AppCoordinator::fixture(vec![BrowseTitle::Cinemeta(title(1))], None, cx));
        c.update(&mut cx, |c, _| {
            c.tmdb_credential = Some(CredentialState::Present);
            let ids: Vec<_> = (0..30).map(id).collect();
            c.card_pending = enrichment::visible_batch(ids);
            assert_eq!(c.card_pending.len(), enrichment::CARD_BATCH);
            c.submit_card_enrichment();
            let (action, g) = c
                .card_enrichment
                .clone()
                .unwrap_or_else(|| panic!("card request"));
            let mut stale = reply(action.clone(), g, 1, "x");
            if let Ok(MetadataSnapshot::Enrichment(s)) = &mut stale.result {
                s.titles[0].title = title(1).with_display(kinema_core::catalog::CatalogDisplay {
                    year: Some(2001),
                    ..Default::default()
                });
            }
            let mut old = stale.clone();
            old.generation = g - 1;
            c.apply_enrichment(old);
            assert_eq!(c.titles.first().and_then(BrowseTitle::year), None);
            c.apply_enrichment(stale);
            assert_eq!(c.titles.first().and_then(BrowseTitle::year), Some(2001));
            // Resubmitting a new batch supersedes the previous generation.
            c.card_pending = vec![id(2)];
            c.submit_card_enrichment();
            assert!(c.card_enrichment.as_ref().is_some_and(|(_, n)| *n > g));
        });
    }

    #[test]
    fn credential_change_reloads_grid_only_when_primary_changes() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            if let Some(s) = c.settings.as_mut() {
                s.metadata_priority =
                    kinema_core::provider::ProviderPriority::new([ProviderId::Tmdb]);
            }
            c.tmdb_credential = Some(CredentialState::Missing);
            assert_eq!(c.provider(), ProviderId::Cinemeta);
            let before = c.generation;
            c.apply_credential_status(CredentialState::Missing, cx);
            assert_eq!(c.generation, before);
            c.apply_credential_status(CredentialState::Present, cx);
            assert_eq!(c.provider(), ProviderId::Tmdb);
            assert_eq!(c.generation, before + 1);
            c.apply_credential_status(CredentialState::Present, cx);
            assert_eq!(c.generation, before + 1);
        });
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
mod discover_filter_tests {
    use super::*;
    use ::core::prelude::v1::test;
    use gpui::{AppContext, TestAppContext};
    use kinema_core::discover::DiscoverSort;

    #[test]
    fn filter_changes_replace_generation_search_ignores_and_kind_prunes_genres() {
        let mut cx = TestAppContext::single();
        let entity = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        entity.update(&mut cx, |c, cx| {
            let settings = Settings {
                metadata_priority: kinema_core::provider::ProviderPriority::new([
                    ProviderId::Tmdb,
                    ProviderId::Cinemeta,
                ]),
                ..Default::default()
            };
            c.settings = Some(settings);
            c.tmdb_credential = Some(CredentialState::Present);
            let old = c.collection.generation();
            let filters = DiscoverFilters {
                sort: DiscoverSort::Newest,
                genres: [18, 28].into(),
                ..Default::default()
            };
            c.set_discover_filters(filters.clone(), cx);
            assert_eq!(c.discover_filters(), &filters);
            assert!(c.discover_filters_apply());
            assert!(c.collection.generation() > old);
            assert!(!c.collection.fetched(old, vec![], 0));
            let current = c.collection.generation();
            c.set_discover_filters(filters, cx);
            assert_eq!(c.collection.generation(), current);
            c.load(MediaKind::Series, " query ".into(), false, cx);
            assert!(!c.discover_filters_apply());
            assert_eq!(c.discover_filters().genres, [18].into());
            assert_eq!(c.discover_filters().sort, DiscoverSort::Newest);
            c.load(MediaKind::Series, "  ".into(), false, cx);
            assert!(c.discover_filters_apply());
            c.tmdb_credential = Some(CredentialState::Missing);
            assert!(!c.discover_filters_apply());
        });
    }
}
