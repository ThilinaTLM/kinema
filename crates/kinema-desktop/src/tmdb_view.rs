//! TMDB browse, search, detail and lazy season screens.
//!
//! [`TmdbState`] is pure: it owns the generations, the per-request loading
//! state and the navigation, and it only *describes* work as
//! [`MetadataRequest`]s. [`TmdbView`] renders it and emits [`TmdbIntent`]s; the
//! catalog worker (not this view) performs all I/O. TMDB identities are typed
//! TMDB IDs until a details response supplies an IMDb ID; sources are offered
//! only when that mapping exists and are never guessed from names.

use std::collections::{BTreeMap, HashMap};

use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme, IconName, IndexPath, Sizable as _,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    list::{List, ListDelegate, ListEvent, ListItem, ListState},
    tab::TabBar,
    tag::Tag,
};
use gpui_kit::*;
use kinema_core::{
    MediaKind,
    catalog::{CatalogEpisode, ImdbId, SearchQuery},
    sources::PlaybackTarget,
};

use crate::{
    app_state::{Origin, Phase, Stage},
    catalog_worker::{MetadataAction, MetadataEvent, MetadataRequest, MetadataSnapshot, Mode},
    tmdb::{TmdbData, TmdbDetails, TmdbPage, TmdbQuery, TmdbSeason, TmdbTitle},
    ui::{
        episodes::{self, EpisodeDelegate},
        strip::{StripModel, status_strip},
    },
};

/// TMDB's numeric title ID. Positive; it is not an IMDb ID.
type TmdbId = u64;

/// TMDB asks every application to show this exact statement.
pub(crate) const ATTRIBUTION: &str =
    "This product uses the TMDB API but is not endorsed or certified by TMDB.";
const TMDB_SITE: &str = "https://www.themoviedb.org";
const TMDB_API_DOCS: &str = "https://developer.themoviedb.org/docs";
/// TMDB serves at most this many pages for any listing.
const MAX_PAGES: u32 = 500;
/// Below this window width the detail page stacks instead of splitting.
const COMPACT_WIDTH: f32 = 1100.0;
const ROW_HEIGHT: f32 = 72.0;
/// Generations of one view live in `[base, base + 2^48)`; views of different
/// kinds never share a range, and the counter never goes backwards.
const GENERATION_BASE: u64 = 1 << 56;

actions!(kinema_tmdb, [TmdbBack, TmdbFocusSearch]);

/// Registers the view's keys; call once next to the shell's own bindings.
pub(crate) fn bind_keys(cx: &mut App) {
    let find = if cfg!(target_os = "macos") {
        "cmd-f"
    } else {
        "ctrl-f"
    };
    cx.bind_keys([
        KeyBinding::new(find, TmdbFocusSearch, Some("TmdbView")),
        KeyBinding::new("escape", TmdbBack, Some("TmdbView")),
        KeyBinding::new("alt-left", TmdbBack, Some("TmdbView")),
    ]);
}

/// What the view asks its host to do. The host owns the worker and the
/// sources flow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TmdbIntent {
    Request(MetadataRequest),
    /// Ask the worker to stop this exact request (`CatalogWorker::cancel_metadata`).
    /// Emitted once per request, before any request that supersedes it, and
    /// never for a request that already finished.
    Cancel {
        action: MetadataAction,
        generation: u64,
    },
    FindSources {
        target: PlaybackTarget,
    },
}

// ---- pure model ---------------------------------------------------------

/// A listing row, reduced from a normalized TMDB title.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Row {
    pub id: TmdbId,
    pub name: String,
    pub year: Option<String>,
    pub overview: Option<String>,
}

impl Row {
    fn from_title(title: &TmdbTitle) -> Self {
        Self {
            id: title.id,
            name: title.name.clone(),
            year: title
                .release_info
                .as_deref()
                .and_then(year_of)
                .map(str::to_owned),
            overview: title
                .overview
                .clone()
                .filter(|text| !text.trim().is_empty()),
        }
    }
}

/// The year of a `YYYY-MM-DD` (or `YYYY`) date; anything else shows no year.
pub(crate) fn year_of(date: &str) -> Option<&str> {
    let year = date.get(..4)?;
    let rest = &date[4..];
    let digits = year.bytes().all(|b| b.is_ascii_digit());
    (digits && (rest.is_empty() || rest.starts_with('-'))).then_some(year)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PageRows {
    rows: Vec<Row>,
    total_pages: u32,
}

fn page_rows(page: &TmdbPage) -> PageRows {
    PageRows {
        rows: page.titles.iter().map(Row::from_title).collect(),
        total_pages: page.total_pages.min(MAX_PAGES),
    }
}

/// Distinct season numbers TMDB lists for a series, ascending.
pub(crate) fn season_numbers(details: &TmdbDetails) -> Vec<u32> {
    let mut numbers: Vec<u32> = details.seasons.iter().map(|s| s.number).collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
}

fn episodes_of(season: &TmdbSeason) -> Vec<CatalogEpisode> {
    season.episodes.clone()
}

/// Maps a title (and, for series, a picked episode) onto a source target.
/// Only a mapped IMDb ID qualifies; names are never used to find one.
pub(crate) fn source_target(
    kind: MediaKind,
    imdb: Option<&ImdbId>,
    episode: Option<(u32, u32)>,
) -> Result<PlaybackTarget, &'static str> {
    let Some(imdb) = imdb else {
        return Err(
            "TMDB lists no IMDb ID for this title, so Kinema cannot look up sources. \
                    It never guesses a match by name.",
        );
    };
    match (kind, episode) {
        (MediaKind::Movie, _) => Ok(PlaybackTarget::Movie(imdb.clone())),
        (MediaKind::Series, Some((season, episode))) => Ok(PlaybackTarget::Episode {
            series: imdb.clone(),
            season,
            episode,
        }),
        (MediaKind::Series, None) => Err("Select an episode first."),
    }
}

/// Whether a worker error means the TMDB token is missing (not a service
/// failure), so the screen can point at setup instead of retrying.
pub(crate) fn is_missing_token(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("token")
        && ["missing", "configure", "not set", "required"]
            .iter()
            .any(|hint| lower.contains(hint))
}

/// One remote-backed value; generations come from the owner's single counter.
#[derive(Debug)]
struct Load<T> {
    generation: u64,
    value: Option<T>,
    fetched_at: Option<u64>,
    origin: Origin,
    phase: Phase,
    error: Option<String>,
}

impl<T> Load<T> {
    fn new(generation: u64) -> Self {
        Self {
            generation,
            value: None,
            fetched_at: None,
            origin: Origin::None,
            phase: Phase::Reading,
            error: None,
        }
    }

    /// A new request; earlier data stays visible while it runs.
    fn restart(&mut self, generation: u64) {
        self.generation = generation;
        self.error = None;
        self.phase = Phase::Reading;
    }

    fn busy(&self) -> bool {
        self.phase != Phase::Idle
    }

    fn cancel(&mut self) {
        self.phase = Phase::Idle;
    }

    /// False for stale or unsolicited results. Failure keeps earlier data.
    fn apply(&mut self, generation: u64, stage: Stage<T>) -> bool {
        if generation != self.generation || !self.busy() {
            return false;
        }
        match stage {
            Stage::CacheHit {
                value,
                fetched_at,
                remote_follows,
            } => {
                self.store(value, fetched_at, Origin::Saved);
                self.phase = if remote_follows {
                    Phase::Fetching
                } else {
                    Phase::Idle
                };
            }
            Stage::Fetched { value, fetched_at } => {
                self.store(value, fetched_at, Origin::Fetched);
                self.error = None;
                self.phase = Phase::Idle;
            }
            Stage::Failed(message) => {
                self.error = Some(message);
                self.phase = Phase::Idle;
            }
        }
        true
    }

    fn store(&mut self, value: T, fetched_at: u64, origin: Origin) {
        self.value = Some(value);
        self.fetched_at = Some(fetched_at);
        self.origin = origin;
    }

    fn strip<'a>(&'a self, service: &'a str) -> StripModel<'a> {
        StripModel {
            phase: self.phase,
            error: self.error.as_deref(),
            has_data: self.value.is_some(),
            origin: self.origin,
            fetched_at: self.fetched_at,
            service,
        }
    }
}

fn convert<T, U>(stage: Stage<T>, f: impl FnOnce(T) -> Option<U>) -> Stage<U> {
    const UNEXPECTED: &str = "The catalog returned an unexpected TMDB result.";
    match stage {
        Stage::CacheHit {
            value,
            fetched_at,
            remote_follows,
        } => match f(value) {
            Some(value) => Stage::CacheHit {
                value,
                fetched_at,
                remote_follows,
            },
            None => Stage::Failed(UNEXPECTED.to_owned()),
        },
        Stage::Fetched { value, fetched_at } => match f(value) {
            Some(value) => Stage::Fetched { value, fetched_at },
            None => Stage::Failed(UNEXPECTED.to_owned()),
        },
        Stage::Failed(message) => Stage::Failed(message),
    }
}

/// A worker event reduced to what this view needs; `None` if it is not TMDB's.
fn into_stage(event: MetadataEvent) -> Option<(u64, TmdbQuery, Stage<TmdbData>)> {
    let MetadataEvent {
        generation,
        action,
        result,
        saved,
        refreshing,
    } = event;
    let MetadataAction::Tmdb(query) = action else {
        return None;
    };
    let stage = match result {
        Ok(MetadataSnapshot::Tmdb(found)) if saved => Stage::CacheHit {
            value: found.data,
            fetched_at: found.fetched_at,
            remote_follows: refreshing,
        },
        Ok(MetadataSnapshot::Tmdb(found)) => Stage::Fetched {
            value: found.data,
            fetched_at: found.fetched_at,
        },
        Ok(_) => Stage::Failed("The catalog returned an unexpected result.".to_owned()),
        Err(message) => Stage::Failed(message),
    };
    Some((generation, query, stage))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Listing {
    Popular,
    Search(SearchQuery),
}

struct DetailState {
    id: TmdbId,
    /// Shown while details load.
    row: Row,
    load: Load<TmdbDetails>,
    season: Option<u32>,
    seasons: HashMap<u32, Load<TmdbSeason>>,
    selected: Option<(u32, u32)>,
}

impl DetailState {
    fn imdb(&self) -> Option<&ImdbId> {
        self.load.value.as_ref().and_then(|d| d.imdb.as_ref())
    }
}

/// Navigation, loading state and generations; no I/O and no GPUI.
pub(crate) struct TmdbState {
    kind: MediaKind,
    next_generation: u64,
    listing: Listing,
    pages: BTreeMap<u32, Load<PageRows>>,
    /// Set by Refresh: the first page, once freshly fetched, replaces the rest.
    refresh_head: bool,
    detail: Option<DetailState>,
    /// Requests handed out and not yet finished (cache hit + pending remote
    /// still counts), so each is cancelled exactly once.
    inflight: Vec<(TmdbQuery, u64)>,
    /// Cancellations waiting to be emitted; see [`Self::take_cancels`].
    outbox: Vec<(MetadataAction, u64)>,
}

/// Result of applying a worker event.
pub(crate) struct Applied {
    pub consumed: bool,
    pub follow_up: Option<MetadataRequest>,
}

impl TmdbState {
    pub(crate) fn new(kind: MediaKind) -> Self {
        let slot = match kind {
            MediaKind::Movie => 0,
            MediaKind::Series => 1,
        };
        Self {
            kind,
            next_generation: GENERATION_BASE + (slot << 48),
            listing: Listing::Popular,
            pages: BTreeMap::new(),
            refresh_head: false,
            detail: None,
            inflight: Vec::new(),
            outbox: Vec::new(),
        }
    }

    /// Cancellations to forward to the worker, oldest first. Drain before
    /// emitting the request that caused them.
    pub(crate) fn take_cancels(&mut self) -> Vec<(MetadataAction, u64)> {
        std::mem::take(&mut self.outbox)
    }

    fn cancel_where(&mut self, doomed: impl Fn(&TmdbQuery) -> bool) {
        let (gone, kept) = std::mem::take(&mut self.inflight)
            .into_iter()
            .partition(|(query, _)| doomed(query));
        self.inflight = kept;
        self.outbox.extend(
            gone.into_iter()
                .map(|(query, generation)| (MetadataAction::Tmdb(query), generation)),
        );
    }

    fn generation(&mut self) -> u64 {
        self.next_generation += 1;
        self.next_generation
    }

    fn request(&mut self, query: TmdbQuery, generation: u64, refresh: bool) -> MetadataRequest {
        // A newer request for the same query supersedes the older one.
        self.cancel_where(|old| *old == query);
        self.inflight.push((query.clone(), generation));
        MetadataRequest {
            generation,
            action: MetadataAction::Tmdb(query),
            mode: if refresh { Mode::Refresh } else { Mode::Load },
        }
    }

    fn page_query(&self, page: u32) -> TmdbQuery {
        match &self.listing {
            Listing::Popular => TmdbQuery::Browse {
                kind: self.kind,
                page,
            },
            Listing::Search(query) => TmdbQuery::Search {
                query: query.clone(),
                page,
            },
        }
    }

    pub(crate) fn kind(&self) -> MediaKind {
        self.kind
    }

    pub(crate) fn searching(&self) -> Option<&SearchQuery> {
        match &self.listing {
            Listing::Search(query) => Some(query),
            Listing::Popular => None,
        }
    }

    pub(crate) fn detail_open(&self) -> bool {
        self.detail.is_some()
    }

    /// Cancels everything in flight; late results are rejected afterwards.
    pub(crate) fn cancel_all(&mut self) {
        self.cancel_where(|_| true);
        self.pages.values_mut().for_each(Load::cancel);
        if let Some(detail) = &mut self.detail {
            detail.load.cancel();
            detail.seasons.values_mut().for_each(Load::cancel);
        }
    }

    /// Switches kind (dropping all state) and starts the popular listing if
    /// nothing usable is loaded yet.
    pub(crate) fn activate(&mut self, kind: MediaKind) -> Option<MetadataRequest> {
        if kind != self.kind {
            self.cancel_all();
            let counter = self.next_generation;
            let outbox = std::mem::take(&mut self.outbox);
            *self = Self::new(kind);
            self.outbox = outbox;
            // Never reuse a generation, even across kinds.
            self.next_generation = self.next_generation.max(counter);
        }
        if self.detail.is_some() {
            return None;
        }
        match self.pages.get(&1) {
            Some(first) if first.value.is_some() || first.busy() => None,
            _ => self.load_page(1, false),
        }
    }

    fn load_page(&mut self, page: u32, refresh: bool) -> Option<MetadataRequest> {
        if page == 0 || page > MAX_PAGES {
            return None;
        }
        let generation = self.generation();
        match self.pages.get_mut(&page) {
            Some(load) => load.restart(generation),
            None => {
                self.pages.insert(page, Load::new(generation));
            }
        }
        Some(self.request(self.page_query(page), generation, refresh))
    }

    /// Starts a search; invalid text is a message, not a request.
    pub(crate) fn search(&mut self, text: &str) -> Result<MetadataRequest, String> {
        let query = SearchQuery::new(self.kind, text).map_err(|e| e.to_string())?;
        self.cancel_all();
        self.detail = None;
        self.pages.clear();
        self.refresh_head = false;
        self.listing = Listing::Search(query);
        self.load_page(1, false)
            .ok_or_else(|| "The search could not start.".to_owned())
    }

    pub(crate) fn clear_search(&mut self) -> Option<MetadataRequest> {
        self.cancel_all();
        self.pages.clear();
        self.refresh_head = false;
        self.listing = Listing::Popular;
        self.load_page(1, false)
    }

    /// The next page to request, if the last one finished and more exist.
    pub(crate) fn next_page(&self) -> Option<u32> {
        let (&last, load) = self.pages.last_key_value()?;
        let total = load.value.as_ref()?.total_pages;
        (!self.listing_busy() && last < total.min(MAX_PAGES)).then_some(last + 1)
    }

    pub(crate) fn load_more(&mut self) -> Option<MetadataRequest> {
        let page = self.next_page()?;
        self.load_page(page, false)
    }

    pub(crate) fn listing_busy(&self) -> bool {
        self.pages.values().any(Load::busy)
    }

    /// Rows of all loaded pages in order, without duplicates (pages can shift
    /// between requests).
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut seen = std::collections::HashSet::new();
        self.pages
            .values()
            .filter_map(|load| load.value.as_ref())
            .flat_map(|page| page.rows.iter())
            .filter(|row| seen.insert(row.id))
            .cloned()
            .collect()
    }

    pub(crate) fn open(&mut self, row: Row) -> MetadataRequest {
        let generation = self.generation();
        let id = row.id;
        self.detail = Some(DetailState {
            id,
            row,
            load: Load::new(generation),
            season: None,
            seasons: HashMap::new(),
            selected: None,
        });
        self.request(
            TmdbQuery::Details {
                kind: self.kind,
                id,
            },
            generation,
            false,
        )
    }

    pub(crate) fn close_detail(&mut self) {
        if let Some(mut detail) = self.detail.take() {
            self.cancel_where(|q| {
                matches!(q, TmdbQuery::Details { .. } | TmdbQuery::Season { .. })
            });
            detail.load.cancel();
            detail.seasons.values_mut().for_each(Load::cancel);
        }
    }

    /// Chooses a season and requests it lazily unless it is loaded or loading.
    pub(crate) fn select_season(&mut self, season: u32) -> Option<MetadataRequest> {
        let detail = self.detail.as_mut()?;
        if !detail
            .load
            .value
            .as_ref()
            .is_some_and(|d| season_numbers(d).contains(&season))
        {
            return None;
        }
        if detail.season != Some(season) {
            detail.season = Some(season);
            detail.selected = None;
        }
        let series = detail.id;
        if detail
            .seasons
            .get(&season)
            .is_some_and(|load| load.busy() || load.value.is_some())
        {
            return None;
        }
        self.next_generation += 1;
        let generation = self.next_generation;
        let detail = self.detail.as_mut()?;
        match detail.seasons.get_mut(&season) {
            Some(load) => load.restart(generation),
            None => {
                detail.seasons.insert(season, Load::new(generation));
            }
        }
        Some(self.request(
            TmdbQuery::Season {
                series_id: series,
                season,
            },
            generation,
            false,
        ))
    }

    pub(crate) fn select_episode(&mut self, episode: Option<(u32, u32)>) {
        if let Some(detail) = &mut self.detail {
            detail.selected = episode;
        }
    }

    /// Explicit Refresh of what is on screen; always `Mode::Refresh`.
    pub(crate) fn refresh(&mut self) -> Vec<MetadataRequest> {
        let mut out = Vec::new();
        if let Some(detail) = &mut self.detail {
            let (id, season) = (detail.id, detail.season);
            let generation = self.next_generation + 1;
            detail.load.restart(generation);
            let mut next = generation;
            let mut queries = vec![(
                generation,
                TmdbQuery::Details {
                    kind: self.kind,
                    id,
                },
            )];
            if let Some(season) = season
                && let Some(load) = detail.seasons.get_mut(&season)
            {
                next += 1;
                load.restart(next);
                queries.push((
                    next,
                    TmdbQuery::Season {
                        series_id: id,
                        season,
                    },
                ));
            }
            self.next_generation = next;
            for (generation, query) in queries {
                out.push(self.request(query, generation, true));
            }
        } else {
            self.refresh_head = true;
            out.extend(self.load_page(1, true));
        }
        out
    }

    /// Whether Refresh would do anything right now.
    pub(crate) fn busy(&self) -> bool {
        match &self.detail {
            Some(detail) => {
                detail.load.busy()
                    || detail
                        .season
                        .and_then(|s| detail.seasons.get(&s))
                        .is_some_and(Load::busy)
            }
            None => self.listing_busy(),
        }
    }

    pub(crate) fn source_target(&self) -> Result<PlaybackTarget, &'static str> {
        match &self.detail {
            Some(detail) => source_target(self.kind, detail.imdb(), detail.selected),
            None => Err("Open a title first."),
        }
    }

    /// Applies a worker event if it answers a current request of this view.
    pub(crate) fn apply(
        &mut self,
        generation: u64,
        query: &TmdbQuery,
        stage: Stage<TmdbData>,
    ) -> Applied {
        let mut applied = Applied {
            consumed: false,
            follow_up: None,
        };
        // A saved answer with a remote fetch to follow keeps the request alive.
        let finished = !matches!(
            stage,
            Stage::CacheHit {
                remote_follows: true,
                ..
            }
        );
        match query {
            TmdbQuery::Browse { kind, page } if *kind == self.kind => {
                if self.listing == Listing::Popular {
                    applied.consumed = self.apply_page(*page, generation, stage);
                }
            }
            TmdbQuery::Search { query, page } if query.kind() == self.kind => {
                if self.listing == Listing::Search(query.clone()) {
                    applied.consumed = self.apply_page(*page, generation, stage);
                }
            }
            TmdbQuery::Details { kind, id } if *kind == self.kind => {
                if let Some(detail) = self.detail.as_mut().filter(|d| d.id == *id) {
                    let stage = convert(stage, |data| match data {
                        TmdbData::Details(details) => Some(details),
                        _ => None,
                    });
                    applied.consumed = detail.load.apply(generation, stage);
                    if applied.consumed && self.kind == MediaKind::Series {
                        applied.follow_up = self.pick_default_season();
                    }
                }
            }
            TmdbQuery::Season {
                series_id: series,
                season,
            } => {
                if let Some(load) = self
                    .detail
                    .as_mut()
                    .filter(|d| d.id == *series)
                    .and_then(|d| d.seasons.get_mut(season))
                {
                    let number = *season;
                    let stage = convert(stage, |data| match data {
                        TmdbData::Season(found) if found.season == number => Some(found),
                        _ => None,
                    });
                    applied.consumed = load.apply(generation, stage);
                }
            }
            _ => {}
        }
        if applied.consumed && finished {
            self.inflight
                .retain(|(q, g)| !(q == query && *g == generation));
        }
        applied
    }

    fn apply_page(&mut self, page: u32, generation: u64, stage: Stage<TmdbData>) -> bool {
        let fetched = matches!(stage, Stage::Fetched { .. });
        let stage = convert(stage, |data| match data {
            TmdbData::Page(found) => Some(page_rows(&found)),
            _ => None,
        });
        let Some(load) = self.pages.get_mut(&page) else {
            return false;
        };
        let consumed = load.apply(generation, stage);
        if consumed && page == 1 && fetched && self.refresh_head {
            self.refresh_head = false;
            self.pages.retain(|number, _| *number == 1);
        }
        consumed
    }

    /// After details arrive for a series: opens the first regular season.
    fn pick_default_season(&mut self) -> Option<MetadataRequest> {
        let detail = self.detail.as_ref()?;
        if detail.season.is_some() {
            return None;
        }
        let numbers = season_numbers(detail.load.value.as_ref()?);
        let season = episodes::default_season(&numbers)?;
        self.select_season(season)
    }
}

// ---- list rows ----------------------------------------------------------

struct RowDelegate {
    rows: Vec<Row>,
    selected: Option<IndexPath>,
    empty: Option<SharedString>,
}

impl RowDelegate {
    fn set(&mut self, rows: Vec<Row>, empty: Option<SharedString>) {
        if self.rows != rows {
            self.rows = rows;
            self.selected = None;
        }
        self.empty = empty;
    }
}

impl ListDelegate for RowDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.rows.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.rows.get(ix.row)?;
        let muted = cx.theme().muted_foreground;
        let heading = match &row.year {
            Some(year) => format!("{} ({year})", row.name),
            None => row.name.clone(),
        };
        let mut body = div().flex().flex_col().gap_1().overflow_hidden().child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(heading.clone()),
        );
        if let Some(overview) = &row.overview {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .line_clamp(2)
                    .child(overview.clone()),
            );
        }
        Some(
            ListItem::new(("tmdb-title", ix.row))
                .h(px(ROW_HEIGHT))
                .px_3()
                .selected(self.selected == Some(ix))
                .accessibility_label(heading)
                .child(body),
        )
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .children(self.empty.clone())
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix;
        cx.notify();
    }
}

// ---- view ---------------------------------------------------------------

pub(crate) struct TmdbView {
    state: TmdbState,
    input: Entity<InputState>,
    list: Entity<ListState<RowDelegate>>,
    episode_list: Entity<ListState<EpisodeDelegate>>,
    input_error: Option<String>,
    /// Outcome of the last action, e.g. why sources are unavailable.
    notice: Option<String>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TmdbIntent> for TmdbView {}

impl TmdbView {
    pub(crate) fn new(kind: MediaKind, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let placeholder = match kind {
                MediaKind::Movie => "Search TMDB movies",
                MediaKind::Series => "Search TMDB series",
            };
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
            let list = cx.new(|cx| {
                ListState::new(
                    RowDelegate {
                        rows: Vec::new(),
                        selected: None,
                        empty: None,
                    },
                    window,
                    cx,
                )
            });
            let episode_list = cx.new(|cx| ListState::new(EpisodeDelegate::new(), window, cx));
            let subscriptions = vec![
                cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.search(cx);
                    }
                }),
                cx.subscribe_in(&list, window, |this, list, event: &ListEvent, _, cx| {
                    if let ListEvent::Confirm(ix) = event {
                        let row = list.read(cx).delegate().rows.get(ix.row).cloned();
                        if let Some(row) = row {
                            this.open(row, cx);
                        }
                    }
                }),
                cx.subscribe_in(
                    &episode_list,
                    window,
                    |this, list, event: &ListEvent, _, cx| {
                        let pick = |ix: &IndexPath, cx: &App| {
                            list.read(cx)
                                .delegate()
                                .episode(*ix)
                                .map(|e| (e.season, e.number))
                        };
                        match event {
                            ListEvent::Select(ix) => {
                                let picked = pick(ix, cx);
                                this.state.select_episode(picked);
                                this.notice = None;
                                cx.notify();
                            }
                            ListEvent::Confirm(ix) => {
                                let picked = pick(ix, cx);
                                this.state.select_episode(picked);
                                this.find_sources(cx);
                            }
                            ListEvent::Cancel => {}
                        }
                    },
                ),
            ];
            Self {
                state: TmdbState::new(kind),
                input,
                list,
                episode_list,
                input_error: None,
                notice: None,
                focus: cx.focus_handle(),
                _subscriptions: subscriptions,
            }
        })
    }

    /// Shows this page: starts popular browse unless data is already loaded.
    pub(crate) fn activate(
        &mut self,
        kind: MediaKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if kind != self.state.kind() {
            self.input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.input_error = None;
            self.notice = None;
        }
        if let Some(request) = self.state.activate(kind) {
            self.emit_request(request, cx);
        }
        self.focus.focus(window, cx);
        self.sync(cx);
    }

    /// Leaves the page: cancels in-flight work; late results are rejected.
    pub(crate) fn deactivate(&mut self, cx: &mut Context<Self>) {
        self.state.cancel_all();
        self.flush_cancels(cx);
        cx.notify();
    }

    /// Feeds a worker event; false if it is not this view's (other kinds,
    /// other modules, or superseded/cancelled requests).
    pub(crate) fn handle_event(&mut self, event: MetadataEvent, cx: &mut Context<Self>) -> bool {
        let Some((generation, query, stage)) = into_stage(event) else {
            return false;
        };
        let applied = self.state.apply(generation, &query, stage);
        if let Some(request) = applied.follow_up {
            self.emit_request(request, cx);
        }
        if applied.consumed {
            self.sync(cx);
        }
        applied.consumed
    }

    /// Esc / Back: closes details, then a search. False if nothing was open.
    pub(crate) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.state.detail_open() {
            self.state.close_detail();
            self.notice = None;
            self.sync(cx);
            true
        } else if self.state.searching().is_some() {
            self.clear_search(window, cx);
            true
        } else {
            false
        }
    }

    /// Forwards pending cancellations; always call before emitting a request.
    fn emit_request(&mut self, request: MetadataRequest, cx: &mut Context<Self>) {
        self.flush_cancels(cx);
        cx.emit(TmdbIntent::Request(request));
    }

    fn flush_cancels(&mut self, cx: &mut Context<Self>) {
        for (action, generation) in self.state.take_cancels() {
            cx.emit(TmdbIntent::Cancel { action, generation });
        }
    }

    fn emit_all(
        &mut self,
        requests: impl IntoIterator<Item = MetadataRequest>,
        cx: &mut Context<Self>,
    ) {
        for request in requests {
            self.emit_request(request, cx);
        }
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        match self.state.search(&text) {
            Ok(request) => {
                self.input_error = None;
                self.notice = None;
                self.emit_request(request, cx);
            }
            Err(message) => self.input_error = Some(message),
        }
        self.sync(cx);
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.input_error = None;
        let request = self.state.clear_search();
        self.emit_all(request, cx);
        self.sync(cx);
    }

    fn open(&mut self, row: Row, cx: &mut Context<Self>) {
        self.notice = None;
        let request = self.state.open(row);
        self.emit_request(request, cx);
        self.sync(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let requests = self.state.refresh();
        self.emit_all(requests, cx);
        self.sync(cx);
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        let request = self.state.load_more();
        self.emit_all(request, cx);
        self.sync(cx);
    }

    fn select_season(&mut self, season: u32, cx: &mut Context<Self>) {
        let request = self.state.select_season(season);
        self.emit_all(request, cx);
        self.sync(cx);
    }

    fn find_sources(&mut self, cx: &mut Context<Self>) {
        match self.state.source_target() {
            Ok(target) => {
                self.notice = None;
                cx.emit(TmdbIntent::FindSources { target });
            }
            Err(message) => self.notice = Some(message.to_owned()),
        }
        cx.notify();
    }

    /// Pushes state into the list entities and repaints.
    fn sync(&mut self, cx: &mut Context<Self>) {
        self.flush_cancels(cx);
        let empty = if self.state.listing_busy() {
            None
        } else if self.state.pages.values().all(|p| p.value.is_none()) {
            // Nothing loaded: a failure is shown by the strip, not as "empty".
            None
        } else if self.state.searching().is_some() {
            Some(SharedString::from("TMDB found nothing for this search."))
        } else {
            Some(SharedString::from("TMDB returned no titles."))
        };
        let rows = self.state.rows();
        self.list.update(cx, |list, cx| {
            list.delegate_mut().set(rows, empty);
            cx.notify();
        });
        let episodes = self
            .state
            .detail
            .as_ref()
            .and_then(|d| d.season.and_then(|s| d.seasons.get(&s)))
            .and_then(|load| load.value.as_ref())
            .map(|season| {
                let mut rows = episodes_of(season);
                rows.sort_by_key(|e| e.number);
                rows
            })
            .unwrap_or_default();
        self.episode_list.update(cx, |list, cx| {
            list.delegate_mut().set(episodes);
            cx.notify();
        });
        cx.notify();
    }

    // ---- rendering ------------------------------------------------------

    fn error_hint(&self, error: Option<&str>, cx: &Context<Self>) -> Option<Div> {
        let message = error?;
        is_missing_token(message).then(|| {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(
                    "TMDB needs your own read access token. Add it in Settings, then press \
                     Refresh. Until then Kinema shows no TMDB titles.",
                )
        })
    }

    fn footer(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(ATTRIBUTION)
            .child(
                Button::new("tmdb-site")
                    .label("themoviedb.org")
                    .ghost()
                    .xsmall()
                    .on_click(|_, _, cx| cx.open_url(TMDB_SITE)),
            )
            .child("Metadata requests use the TMDB API with your own token.")
            .child(
                Button::new("tmdb-api")
                    .label("TMDB API documentation")
                    .ghost()
                    .xsmall()
                    .on_click(|_, _, cx| cx.open_url(TMDB_API_DOCS)),
            )
    }

    fn render_listing(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let busy = self.state.busy();
        let kind_label = match self.state.kind() {
            MediaKind::Movie => "movies",
            MediaKind::Series => "series",
        };
        let heading = match self.state.searching() {
            Some(query) => format!("TMDB results for “{}”", query.text()),
            None => format!("Popular {kind_label} on TMDB"),
        };
        let controls = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w(px(180.0))
                    .child(Input::new(&self.input)),
            )
            .child(
                Button::new("tmdb-search")
                    .icon(IconName::Search)
                    .label("Search")
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.search(cx))),
            )
            .child(
                Button::new("tmdb-refresh")
                    .icon(IconName::RefreshCw)
                    .label("Refresh")
                    .outline()
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            );
        let mut status = div().flex().flex_col().gap_2();
        if let Some(message) = &self.input_error {
            status = status.child(div().text_color(cx.theme().danger).child(message.clone()));
        }
        if self.state.searching().is_some() {
            status = status.child(
                div().flex().child(
                    Button::new("tmdb-clear-search")
                        .icon(IconName::Close)
                        .label("Clear search")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| this.clear_search(window, cx))),
                ),
            );
        }
        // Strip describes the first page (freshness) unless a later one is busy/failed.
        let focus = self
            .state
            .pages
            .values()
            .rev()
            .find(|p| p.busy() || p.error.is_some())
            .or_else(|| self.state.pages.values().next());
        let error = focus.and_then(|p| p.error.clone());
        if let Some(load) = focus {
            let mut model = load.strip("TMDB");
            if let Some(first) = self.state.pages.values().next() {
                model.has_data = self.state.pages.values().any(|p| p.value.is_some());
                model.fetched_at = first.fetched_at;
                model.origin = first.origin;
            }
            status = status.child(status_strip("tmdb-list-status", &model, cx));
        }
        status = status.children(self.error_hint(error.as_deref(), cx));

        let mut body = div().flex_1().min_h_0().flex().flex_col().gap_2().child(
            div()
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .child(List::new(&self.list)),
        );
        if self.state.next_page().is_some() {
            body = body.child(
                div().flex().justify_center().child(
                    Button::new("tmdb-more")
                        .icon(IconName::ArrowDown)
                        .label("Load more")
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| this.load_more(cx))),
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
            .child(div().text_color(muted).child(
                "Discovery from TMDB. Sources are offered only for titles TMDB maps to an IMDb ID.",
            ))
            .child(controls)
            .child(status)
            .child(body)
    }

    fn render_detail(&mut self, compact: bool, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let kind = self.state.kind();
        let Some(detail) = self.state.detail.as_ref() else {
            return div();
        };
        let details = detail.load.value.as_ref();
        let imdb = detail.imdb().cloned();
        let busy = self.state.busy();

        let mut head = div().flex().flex_col().gap_2().child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    Button::new("tmdb-back")
                        .icon(IconName::ArrowLeft)
                        .label("Back")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.go_back(window, cx);
                        })),
                )
                .child(div().text_2xl().truncate().child(detail.row.name.clone())),
        );
        let mut meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .text_color(muted);
        if let Some(year) = &detail.row.year {
            meta = meta.child(year.clone());
        }
        if let Some(details) = details {
            for genre in &details.genres {
                meta = meta.child(Tag::secondary().child(genre.clone()));
            }
        }
        meta = match (&imdb, details) {
            (Some(id), _) => meta.child(Tag::success().child(format!("IMDb {}", id.as_str()))),
            (None, Some(_)) => meta.child(Tag::secondary().child("No IMDb mapping")),
            (None, None) => meta,
        };
        head = head.child(meta);
        let overview = details
            .and_then(|d| d.title.overview.clone())
            .filter(|o| !o.trim().is_empty())
            .or_else(|| detail.row.overview.clone());
        if let Some(overview) = overview {
            head = head.child(
                div()
                    .id("tmdb-overview")
                    .max_h(px(160.0))
                    .overflow_y_scroll()
                    .text_color(muted)
                    .child(overview),
            );
        }

        let mut strip = div().flex().flex_col().gap_2().child(status_strip(
            "tmdb-detail-status",
            &detail.load.strip("TMDB"),
            cx,
        ));
        strip = strip.children(self.error_hint(detail.load.error.as_deref(), cx));

        let mut actions = div().flex().flex_wrap().items_center().gap_2().child(
            Button::new("tmdb-refresh-detail")
                .icon(IconName::RefreshCw)
                .label("Refresh")
                .outline()
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
        );
        // Sources only exist behind an IMDb mapping; never a name guess.
        let mut hint = None;
        match (details.is_some(), imdb.is_some(), kind) {
            (true, true, MediaKind::Movie) => {
                actions = actions.child(
                    Button::new("tmdb-sources")
                        .icon(IconName::Search)
                        .label("Find sources")
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.find_sources(cx))),
                );
            }
            (true, true, MediaKind::Series) => {
                actions = actions.child(
                    Button::new("tmdb-sources")
                        .icon(IconName::Search)
                        .label(match detail.selected {
                            Some((s, e)) => format!("Find sources for S{s:02}E{e:02}"),
                            None => "Select an episode to find sources".to_owned(),
                        })
                        .primary()
                        .disabled(detail.selected.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.find_sources(cx))),
                );
            }
            (true, false, _) => {
                hint = source_target(kind, None, None).err().map(str::to_owned);
            }
            (false, _, _) => {}
        }

        let mut left = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(head)
            .child(strip)
            .child(actions);
        if let Some(text) = hint.or_else(|| self.notice.clone()) {
            left = left.child(div().text_sm().text_color(muted).child(text));
        }

        let mut root = div().flex_1().min_h_0().flex().gap_4();
        root = if compact {
            root.flex_col()
        } else {
            root.flex_row()
        };
        if kind == MediaKind::Series {
            let seasons = self.render_seasons(cx);
            if compact {
                root = root.child(left).child(seasons);
            } else {
                root = root
                    .child(div().w(px(380.0)).flex_none().child(left))
                    .child(seasons);
            }
            root
        } else {
            root.child(left)
        }
    }

    fn render_seasons(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(detail) = self.state.detail.as_ref() else {
            return div();
        };
        let numbers = detail
            .load
            .value
            .as_ref()
            .map(season_numbers)
            .unwrap_or_default();
        let column = div().flex_1().min_h_0().min_w_0().flex().flex_col().gap_2();
        if numbers.is_empty() {
            let text = match (&detail.load.value, detail.load.busy()) {
                (Some(_), _) => "TMDB lists no seasons for this series.",
                (None, true) => "Loading seasons…",
                (None, false) => "Seasons are unavailable until details load.",
            };
            return column.child(div().text_color(muted).child(text));
        }
        let selected = detail
            .season
            .and_then(|s| numbers.iter().position(|n| *n == s))
            .unwrap_or(0);
        let labels: Vec<String> = numbers.iter().map(|s| episodes::season_label(*s)).collect();
        let for_click = numbers.clone();
        let tabs = TabBar::new("tmdb-seasons")
            .segmented()
            .small()
            .selected_index(selected)
            .on_click(cx.listener(move |this, ix: &usize, _, cx| {
                if let Some(season) = for_click.get(*ix) {
                    this.select_season(*season, cx);
                }
            }))
            .children(labels);
        let mut column = column.child(div().overflow_x_hidden().child(tabs));
        if let Some(load) = detail.season.and_then(|s| detail.seasons.get(&s)) {
            column = column.child(status_strip("tmdb-season-status", &load.strip("TMDB"), cx));
            column = column.children(self.error_hint(load.error.as_deref(), cx));
        }
        column.child(
            div()
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .child(List::new(&self.episode_list)),
        )
    }
}

impl Render for TmdbView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < px(COMPACT_WIDTH);
        let content = if self.state.detail_open() {
            self.render_detail(compact, cx)
        } else {
            self.render_listing(cx)
        };
        let footer = self.footer(cx);
        div()
            .id("tmdb-view")
            .key_context("TmdbView")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &TmdbBack, window, cx| {
                if !this.go_back(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &TmdbFocusSearch, window, cx| {
                if !this.state.detail_open() {
                    this.input.update(cx, |input, cx| input.focus(window, cx));
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .child(content)
            .child(footer)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    // `gpui_kit::*` also exports a `test` attribute; keep the standard one.
    use crate::{catalog::CachedTmdb, tmdb::TmdbSeasonSummary};
    use core::prelude::v1::test;

    fn id(n: u64) -> TmdbId {
        n
    }

    fn title(n: u64, kind: MediaKind) -> TmdbTitle {
        TmdbTitle {
            id: id(n),
            kind,
            name: format!("Title {n}"),
            overview: None,
            release_info: Some("1999-03-31".into()),
            poster_url: None,
        }
    }

    fn page(numbers: &[u64], page: u32, total: u32) -> TmdbData {
        TmdbData::Page(TmdbPage {
            page,
            total_pages: total,
            titles: numbers
                .iter()
                .map(|n| title(*n, MediaKind::Movie))
                .collect(),
        })
    }

    fn details(n: u64, kind: MediaKind, imdb: Option<&str>, seasons: &[u32]) -> TmdbData {
        TmdbData::Details(TmdbDetails {
            title: title(n, kind),
            imdb: imdb.map(|v| ImdbId::new(v).expect("imdb")),
            genres: Vec::new(),
            seasons: seasons
                .iter()
                .map(|number| TmdbSeasonSummary {
                    number: *number,
                    name: None,
                    episode_count: 1,
                })
                .collect(),
        })
    }

    fn fetched(data: TmdbData) -> Stage<TmdbData> {
        Stage::Fetched {
            value: data,
            fetched_at: 9,
        }
    }

    fn query_of(request: &MetadataRequest) -> TmdbQuery {
        match &request.action {
            MetadataAction::Tmdb(query) => query.clone(),
            other => panic!("not a TMDB request: {other:?}"),
        }
    }

    #[test]
    fn activate_requests_popular_once_and_generations_rise() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let first = state.activate(MediaKind::Movie).expect("request");
        assert_eq!(
            query_of(&first),
            TmdbQuery::Browse {
                kind: MediaKind::Movie,
                page: 1
            }
        );
        assert_eq!(first.mode, Mode::Load);
        // Already loading: no duplicate request.
        assert!(state.activate(MediaKind::Movie).is_none());
        state.cancel_all();
        let second = state
            .activate(MediaKind::Movie)
            .expect("retry after cancel");
        assert!(second.generation > first.generation);
        // The cancelled generation is rejected.
        let query = query_of(&first);
        assert!(
            !state
                .apply(first.generation, &query, fetched(page(&[1], 1, 1)))
                .consumed
        );
    }

    #[test]
    fn kind_switch_never_reuses_generations() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let movie = state.activate(MediaKind::Movie).expect("movie");
        let series = state.activate(MediaKind::Series).expect("series");
        assert!(series.generation > movie.generation);
        assert_eq!(state.kind(), MediaKind::Series);
        let query = query_of(&movie);
        assert!(
            !state
                .apply(movie.generation, &query, fetched(page(&[1], 1, 1)))
                .consumed
        );
    }

    #[test]
    fn pages_accumulate_dedupe_and_stop_at_total() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let one = state.activate(MediaKind::Movie).expect("one");
        let q1 = query_of(&one);
        assert!(
            state
                .apply(one.generation, &q1, fetched(page(&[1, 2], 1, 2)))
                .consumed
        );
        assert_eq!(state.next_page(), Some(2));
        let two = state.load_more().expect("two");
        assert!(state.next_page().is_none(), "busy");
        let q2 = query_of(&two);
        assert!(
            state
                .apply(two.generation, &q2, fetched(page(&[2, 3], 2, 2)))
                .consumed
        );
        let names: Vec<u64> = state.rows().iter().map(|r| r.id).collect();
        assert_eq!(names, [1, 2, 3]);
        assert_eq!(state.next_page(), None);
    }

    #[test]
    fn failure_keeps_content_and_empty_result_is_not_an_error() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let one = state.activate(MediaKind::Movie).expect("one");
        let q = query_of(&one);
        state.apply(one.generation, &q, fetched(page(&[], 1, 1)));
        assert!(state.pages[&1].error.is_none() && state.rows().is_empty());
        let refresh = state.refresh().remove(0);
        assert_eq!(refresh.mode, Mode::Refresh);
        assert!(
            state
                .apply(refresh.generation, &q, Stage::Failed("offline".into()))
                .consumed
        );
        assert_eq!(state.pages[&1].error.as_deref(), Some("offline"));
        assert!(state.pages[&1].value.is_some(), "earlier data retained");
    }

    #[test]
    fn refresh_replaces_tail_pages_only_after_a_fresh_first_page() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let one = state.activate(MediaKind::Movie).expect("one");
        state.apply(one.generation, &query_of(&one), fetched(page(&[1], 1, 2)));
        let two = state.load_more().expect("two");
        state.apply(two.generation, &query_of(&two), fetched(page(&[2], 2, 2)));
        let refresh = state.refresh().remove(0);
        let q = query_of(&refresh);
        let cached = Stage::CacheHit {
            value: page(&[1], 1, 2),
            fetched_at: 1,
            remote_follows: true,
        };
        assert!(state.apply(refresh.generation, &q, cached).consumed);
        assert_eq!(state.rows().len(), 2, "tail kept until the remote answer");
        assert!(
            state
                .apply(refresh.generation, &q, fetched(page(&[1], 1, 1)))
                .consumed
        );
        assert_eq!(state.rows().len(), 1);
    }

    #[test]
    fn search_validates_supersedes_and_scopes_results() {
        let mut state = TmdbState::new(MediaKind::Movie);
        assert!(state.search("   ").is_err());
        let popular = state.activate(MediaKind::Movie).expect("popular");
        let search = state.search("  matrix ").expect("search");
        let query = query_of(&search);
        assert!(matches!(&query, TmdbQuery::Search { query, page: 1 } if query.text() == "matrix"));
        // A late popular result no longer belongs here.
        let late = query_of(&popular);
        assert!(
            !state
                .apply(popular.generation, &late, fetched(page(&[9], 1, 1)))
                .consumed
        );
        // Nor does a result for an older search.
        let other = state.search("alien").expect("other");
        assert!(
            !state
                .apply(search.generation, &query, fetched(page(&[9], 1, 1)))
                .consumed
        );
        assert!(
            state
                .apply(
                    other.generation,
                    &query_of(&other),
                    fetched(page(&[5], 1, 1))
                )
                .consumed
        );
        let back = state.clear_search().expect("popular again");
        assert!(matches!(query_of(&back), TmdbQuery::Browse { .. }));
        assert!(state.rows().is_empty());
    }

    #[test]
    fn wrong_data_variant_is_a_failure_not_content() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let one = state.activate(MediaKind::Movie).expect("one");
        let q = query_of(&one);
        let wrong = fetched(details(1, MediaKind::Movie, None, &[]));
        assert!(state.apply(one.generation, &q, wrong).consumed);
        assert!(state.pages[&1].error.is_some() && state.rows().is_empty());
    }

    fn open_series(state: &mut TmdbState, imdb: Option<&str>, seasons: &[u32]) -> MetadataRequest {
        let _ = state.activate(MediaKind::Series);
        let row = Row {
            id: id(7),
            name: "Show".into(),
            year: None,
            overview: None,
        };
        let request = state.open(row);
        let applied = state.apply(
            request.generation,
            &query_of(&request),
            fetched(details(7, MediaKind::Series, imdb, seasons)),
        );
        assert!(applied.consumed);
        applied.follow_up.expect("first season requested lazily")
    }

    #[test]
    fn series_details_lazily_request_default_season_once() {
        let mut state = TmdbState::new(MediaKind::Series);
        let season = open_series(&mut state, Some("tt0903747"), &[0, 2, 1]);
        assert_eq!(
            query_of(&season),
            TmdbQuery::Season {
                series_id: 7,
                season: 1
            }
        );
        // Already loading: no duplicate; other seasons load on demand.
        assert!(state.select_season(1).is_none());
        let two = state.select_season(2).expect("season 2");
        assert_eq!(
            query_of(&two),
            TmdbQuery::Season {
                series_id: 7,
                season: 2
            }
        );
        assert!(state.select_season(5).is_none(), "unlisted season");
        // A stale season result is rejected; the live one applies.
        let data = TmdbData::Season(TmdbSeason {
            series_id: 7,
            season: 2,
            episodes: vec![CatalogEpisode {
                season: 2,
                number: 1,
                name: None,
                overview: None,
                released: None,
            }],
        });
        assert!(
            !state
                .apply(season.generation, &query_of(&two), fetched(data.clone()))
                .consumed
        );
        assert!(
            state
                .apply(two.generation, &query_of(&two), fetched(data))
                .consumed
        );
    }

    #[test]
    fn season_data_for_another_number_is_rejected() {
        let mut state = TmdbState::new(MediaKind::Series);
        let season = open_series(&mut state, None, &[1]);
        let wrong = TmdbData::Season(TmdbSeason {
            series_id: 7,
            season: 3,
            episodes: Vec::new(),
        });
        assert!(
            state
                .apply(season.generation, &query_of(&season), fetched(wrong))
                .consumed
        );
        let load = &state.detail.as_ref().expect("detail").seasons[&1];
        assert!(load.value.is_none() && load.error.is_some());
    }

    #[test]
    fn closing_details_cancels_late_results() {
        let mut state = TmdbState::new(MediaKind::Series);
        let _ = state.activate(MediaKind::Series);
        let row = Row {
            id: id(7),
            name: "S".into(),
            year: None,
            overview: None,
        };
        let request = state.open(row);
        state.close_detail();
        let data = fetched(details(7, MediaKind::Series, None, &[1]));
        assert!(
            !state
                .apply(request.generation, &query_of(&request), data)
                .consumed
        );
        assert!(state.activate(MediaKind::Series).is_none() || state.detail.is_none());
    }

    #[test]
    fn source_targets_require_a_mapping_and_use_series_coordinates() {
        let imdb = ImdbId::new("tt0903747").expect("imdb");
        assert_eq!(
            source_target(MediaKind::Movie, Some(&imdb), None),
            Ok(PlaybackTarget::Movie(imdb.clone()))
        );
        assert_eq!(
            source_target(MediaKind::Series, Some(&imdb), Some((2, 5))),
            Ok(PlaybackTarget::Episode {
                series: imdb.clone(),
                season: 2,
                episode: 5
            })
        );
        assert!(source_target(MediaKind::Series, Some(&imdb), None).is_err());
        let unmapped = source_target(MediaKind::Movie, None, None).expect_err("unmapped");
        assert!(unmapped.contains("never guesses"));
        assert!(source_target(MediaKind::Series, None, Some((1, 1))).is_err());
    }

    #[test]
    fn state_source_target_follows_mapping_and_selection() {
        let mut state = TmdbState::new(MediaKind::Series);
        let season = open_series(&mut state, Some("tt0903747"), &[1]);
        let _ = season;
        assert!(state.source_target().is_err());
        state.select_episode(Some((1, 3)));
        assert_eq!(
            state.source_target(),
            Ok(PlaybackTarget::Episode {
                series: ImdbId::new("tt0903747").expect("imdb"),
                season: 1,
                episode: 3
            })
        );
        let mut unmapped = TmdbState::new(MediaKind::Series);
        let _ = open_series(&mut unmapped, None, &[1]);
        unmapped.select_episode(Some((1, 3)));
        assert!(unmapped.source_target().is_err());
    }

    #[test]
    fn worker_events_translate_by_saved_flag_and_ignore_other_modules() {
        let query = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        let event = |saved, result| MetadataEvent {
            generation: 4,
            action: MetadataAction::Tmdb(query.clone()),
            result,
            saved,
            refreshing: true,
        };
        let ok = || {
            Ok(MetadataSnapshot::Tmdb(CachedTmdb {
                data: page(&[], 1, 1),
                fetched_at: 3,
            }))
        };
        let (generation, _, stage) = into_stage(event(true, ok())).expect("tmdb");
        assert_eq!(generation, 4);
        assert!(matches!(
            stage,
            Stage::CacheHit {
                remote_follows: true,
                fetched_at: 3,
                ..
            }
        ));
        assert!(matches!(
            into_stage(event(false, ok())),
            Some((_, _, Stage::Fetched { .. }))
        ));
        assert!(matches!(
            into_stage(event(false, Err("x".into()))),
            Some((_, _, Stage::Failed(_)))
        ));
        let foreign = MetadataEvent {
            generation: 1,
            action: MetadataAction::Browse {
                kind: MediaKind::Movie,
                offset: 0,
            },
            result: Err("x".into()),
            saved: false,
            refreshing: false,
        };
        assert!(into_stage(foreign).is_none());
    }

    fn cancelled(state: &mut TmdbState) -> Vec<(TmdbQuery, u64)> {
        state
            .take_cancels()
            .into_iter()
            .map(|(action, generation)| match action {
                MetadataAction::Tmdb(query) => (query, generation),
                other => panic!("not a TMDB cancel: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn deactivate_cancels_queued_request_once_and_late_result_is_ignored() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let one = state.activate(MediaKind::Movie).expect("one");
        assert!(cancelled(&mut state).is_empty());
        state.cancel_all();
        assert_eq!(cancelled(&mut state), [(query_of(&one), one.generation)]);
        state.cancel_all();
        assert!(cancelled(&mut state).is_empty(), "never cancelled twice");
        let late = state.apply(one.generation, &query_of(&one), fetched(page(&[1], 1, 1)));
        assert!(!late.consumed && state.rows().is_empty());
    }

    #[test]
    fn cache_hit_awaiting_remote_is_cancelled_once_under_the_same_generation() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let one = state.activate(MediaKind::Movie).expect("one");
        let q = query_of(&one);
        let hit = Stage::CacheHit {
            value: page(&[1], 1, 1),
            fetched_at: 1,
            remote_follows: true,
        };
        assert!(state.apply(one.generation, &q, hit).consumed);
        state.cancel_all();
        assert_eq!(cancelled(&mut state), [(q.clone(), one.generation)]);
        // Its remote answer arrives late: ignored, content from the hit stays.
        assert!(
            !state
                .apply(one.generation, &q, fetched(page(&[2], 1, 1)))
                .consumed
        );
        assert_eq!(state.rows().len(), 1);
    }

    #[test]
    fn finished_requests_are_not_cancelled() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let one = state.activate(MediaKind::Movie).expect("one");
        state.apply(one.generation, &query_of(&one), fetched(page(&[1], 1, 1)));
        state.cancel_all();
        assert!(cancelled(&mut state).is_empty());
    }

    #[test]
    fn superseding_search_cancels_previous_request_before_the_new_one() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let popular = state.activate(MediaKind::Movie).expect("popular");
        let first = state.search("matrix").expect("first");
        assert_eq!(
            cancelled(&mut state),
            [(query_of(&popular), popular.generation)]
        );
        let second = state.search("alien").expect("second");
        assert_eq!(
            cancelled(&mut state),
            [(query_of(&first), first.generation)]
        );
        let back = state.clear_search().expect("popular");
        assert_eq!(
            cancelled(&mut state),
            [(query_of(&second), second.generation)]
        );
        // Re-requesting the same query supersedes its own older generation.
        let again = state.refresh().remove(0);
        assert_eq!(cancelled(&mut state), [(query_of(&back), back.generation)]);
        assert!(again.generation > back.generation);
    }

    #[test]
    fn back_from_details_cancels_detail_and_season_requests_only() {
        let mut state = TmdbState::new(MediaKind::Series);
        let listing = state.activate(MediaKind::Series).expect("listing");
        let row = Row {
            id: id(7),
            name: "S".into(),
            year: None,
            overview: None,
        };
        let details_request = state.open(row);
        let q = query_of(&details_request);
        let applied = state.apply(
            details_request.generation,
            &q,
            fetched(details(7, MediaKind::Series, None, &[1])),
        );
        let season = applied.follow_up.expect("season");
        assert!(cancelled(&mut state).is_empty());
        state.close_detail();
        assert_eq!(
            cancelled(&mut state),
            [(query_of(&season), season.generation)]
        );
        // The listing request was never touched by navigation.
        state.cancel_all();
        assert_eq!(
            cancelled(&mut state),
            [(query_of(&listing), listing.generation)]
        );
        let late = fetched(TmdbData::Season(TmdbSeason {
            series_id: 7,
            season: 1,
            episodes: Vec::new(),
        }));
        assert!(
            !state
                .apply(season.generation, &query_of(&season), late)
                .consumed
        );
    }

    #[test]
    fn kind_switch_cancels_everything_in_flight_and_keeps_the_cancels() {
        let mut state = TmdbState::new(MediaKind::Movie);
        let movie = state.activate(MediaKind::Movie).expect("movie");
        let series = state.activate(MediaKind::Series).expect("series");
        assert_eq!(
            cancelled(&mut state),
            [(query_of(&movie), movie.generation)]
        );
        assert!(series.generation > movie.generation);
        assert!(
            !state
                .apply(
                    movie.generation,
                    &query_of(&movie),
                    fetched(page(&[1], 1, 1))
                )
                .consumed
        );
    }

    #[test]
    fn small_pure_helpers() {
        assert_eq!(year_of("1999-03-31"), Some("1999"));
        assert_eq!(year_of("1999"), Some("1999"));
        assert_eq!(year_of(""), None);
        assert_eq!(year_of("19x9-01-01"), None);
        assert_eq!(year_of("1999x"), None);
        assert!(is_missing_token(
            "Configure a TMDB API Read Access Token to fetch TMDB metadata."
        ));
        assert!(!is_missing_token("network unreachable"));
    }
}
