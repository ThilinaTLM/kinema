//! Production title detail. Route-local selection; the shared coordinator owns I/O.
use super::{
    coordinator::{AppCoordinator, BrowseTitle},
    play_sheet::{PlayContext, PlaySheet, format_bytes},
};
use crate::{
    artwork::{self, ArtworkKey, ArtworkKind, ArtworkState},
    widgets::{
        button::{Button, ButtonSize, ButtonVariant},
        icon::{Icon, IconName},
        icon_button::IconButton,
        menu::{ContextMenuExt, MenuItem, Select, SelectEvent},
        motion::{self, Motion},
        patterns::{
            Badge, BadgeTone, Carousel, ListRow, Section, Skeleton, SkeletonText, StateView,
            ViewSelection, menu_item, select_view,
        },
        segmented::Segmented,
        skeleton_card::SkeletonCard,
        text::TypeExt,
        tokens::{self, radius, space as s, typeface as t},
        tooltip::tooltip,
    },
};
use gpui::{prelude::*, *};
use kinema_core::{
    MediaKind,
    catalog::CatalogEpisode,
    sources::{IndexerKind, PlaybackTarget, SourceCandidate},
};
mod episodes;
#[cfg(feature = "ui-probe")]
mod fixtures;
mod hero;
#[cfg(all(test, feature = "ui-probe-test"))]
mod interaction_tests;
mod recommendations;
mod streams;
#[cfg(test)]
mod tests;
use std::{cell::Cell, collections::HashMap, rc::Rc};

// Detail-specific geometry. Shared spacing, row, typography and chrome
// dimensions remain in widgets::tokens.
mod geometry {
    use gpui::{Pixels, px};
    pub const NARROW_COLUMN: Pixels = px(340.);
    pub const MAX_COLUMN: Pixels = px(520.);
    pub const MIN_HERO: Pixels = px(620.);
    pub const NARROW_AT: Pixels = px(1100.);
    pub const STACK_AT: Pixels = px(960.);
    /// Share of the viewport width taken by the action column.
    pub const COLUMN_FRACTION: f32 = 0.375;
    /// Share of the viewport height given to the action column when stacked.
    pub const STACKED_COLUMN_FRACTION: f32 = 0.60;
    /// Fixed width of the resolution/HDR/codec badge stack in a stream row.
    pub const BADGE_STACK_WIDTH: Pixels = px(84.);
    /// Episode thumbnails are 16:9.
    pub const EPISODE_ART_WIDTH: Pixels = px(96.);
    pub const EPISODE_ART_HEIGHT: Pixels = px(54.);
    pub const SIMILAR_WIDTH: f32 = 132.;
    pub const SIMILAR_HEIGHT: f32 = 198.;
    pub const OVERVIEW_SKELETON_WIDTH: Pixels = px(480.);
    pub const OVERVIEW_MAX_WIDTH: Pixels = px(640.);
    /// The hero reaches at least this share of the viewport height; it grows
    /// further when nothing else fills the column.
    pub const HERO_MIN_FRACTION: f32 = 0.62;
    pub const STACKED_HERO_MIN_FRACTION: f32 = 0.5;
    pub const HERO_MIN_HEIGHT: Pixels = px(380.);
    /// Estimated heights of the sections under the hero (see `below_hero_height`).
    pub const INLINE_FACTS_HEIGHT: Pixels = px(84.);
    pub const DETAILS_PANEL_HEIGHT: Pixels = px(220.);
    pub const SIMILAR_STRIP_HEIGHT: Pixels = px(340.);
    pub const DETAILS_MAX_WIDTH: Pixels = px(720.);
    pub const DETAIL_LABEL_WIDTH: Pixels = px(120.);
    /// Up to this many seasons get "Season N" tabs; more use compact numbers.
    pub const FULL_SEASON_LABELS_MAX: usize = 4;
    /// Skeleton row counts, roughly one screenful.
    pub const STREAM_SKELETON_ROWS: usize = 6;
    pub const EPISODE_SKELETON_ROWS: usize = 5;
    pub const SIMILAR_SKELETON_CARDS: usize = 6;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Quality {
    All,
    Uhd,
    Hd,
    Sd,
    Other,
}
impl Quality {
    fn label(self) -> &'static str {
        match self {
            Quality::All => "All",
            Quality::Uhd => "4K",
            Quality::Hd => "1080p",
            Quality::Sd => "720p",
            Quality::Other => "Other",
        }
    }
}
fn quality(name: &str) -> Quality {
    let name = name.to_ascii_lowercase();
    let tokens: Vec<_> = name.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    if tokens.iter().any(|t| matches!(*t, "2160p" | "4k")) {
        Quality::Uhd
    } else if tokens.contains(&"1080p") {
        Quality::Hd
    } else if tokens.contains(&"720p") {
        Quality::Sd
    } else {
        Quality::Other
    }
}
fn source_quality(c: &SourceCandidate) -> Quality {
    quality(&format!(
        "{} {}",
        c.quality.as_deref().unwrap_or(""),
        c.name
    ))
}
/// Technical badges for one stream row: resolution, dynamic range, codec.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct StreamBadges {
    pub(super) resolution: Option<&'static str>,
    pub(super) dynamic_range: Option<&'static str>,
    pub(super) codec: Option<&'static str>,
}
/// Only resolution, dynamic range and codec become badges. Sizes and seed
/// counts are ordinary facts in the row title.
pub(super) fn stream_badges(c: &SourceCandidate) -> StreamBadges {
    let text = format!("{} {}", c.name, c.quality.as_deref().unwrap_or("")).to_ascii_lowercase();
    let tokens: Vec<_> = text.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let resolution = match source_quality(c) {
        Quality::Uhd => Some("4K"),
        Quality::Hd => Some("1080p"),
        Quality::Sd => Some("720p"),
        _ => None,
    };
    let dynamic_range = if tokens.contains(&"dv") {
        Some("DV")
    } else if tokens.contains(&"hdr") || tokens.contains(&"hdr10") {
        Some("HDR")
    } else {
        None
    };
    let codec = if tokens
        .iter()
        .any(|t| matches!(*t, "hevc" | "x265" | "h265"))
    {
        Some("HEVC")
    } else if tokens.iter().any(|t| matches!(*t, "x264" | "h264")) {
        Some("H.264")
    } else if tokens.contains(&"av1") {
        Some("AV1")
    } else {
        None
    };
    StreamBadges {
        resolution,
        dynamic_range,
        codec,
    }
}
/// Human one-line summary: size · seeders · source. Missing facts are skipped.
pub(super) fn stream_summary(c: &SourceCandidate, source: &str) -> String {
    let mut parts = Vec::new();
    if let Some(size) = c.size {
        parts.push(format_bytes(size));
    }
    if let Some(seeds) = c.seeders {
        parts.push(if seeds == 1 {
            "1 seeder".to_owned()
        } else {
            format!("{seeds} seeders")
        });
    }
    parts.push(source.to_owned());
    parts.join(" · ")
}
/// Index of the first (best-ranked) candidate inside the quality filter.
fn first_in_filter(candidates: &[SourceCandidate], filter: Quality) -> Option<usize> {
    candidates
        .iter()
        .position(|c| filter == Quality::All || source_quality(c) == filter)
}
fn indexer_name(indexer: IndexerKind) -> &'static str {
    match indexer {
        IndexerKind::Torrentio => "Torrentio",
        IndexerKind::Peerflix => "Peerflix",
    }
}
fn episode_marker(ep: &CatalogEpisode) -> String {
    format!("S{:02}E{:02}", ep.season, ep.number)
}
/// `2024-03-02T00:00:00Z` -> `2024-03-02`; unparseable values are not shown.
fn air_date(released: &str) -> Option<&str> {
    released
        .get(..10)
        .filter(|d| kinema_core::catalog_normalized::valid_date(d))
}
/// Coarse "N h ago" label for a Unix-second timestamp.
fn age_label(now: u64, at: u64) -> String {
    let secs = now.saturating_sub(at);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}
fn rating_label(score: f32) -> Option<String> {
    (score.is_finite() && score >= 0.).then(|| format!("{:.1}", score.min(10.)))
}
#[cfg(test)]
fn default_season(episodes: &[CatalogEpisode]) -> Option<u32> {
    choose_season(None, &episode_seasons(episodes))
}
fn episode_seasons(episodes: &[CatalogEpisode]) -> Vec<(u32, u32)> {
    let mut counts = std::collections::BTreeMap::new();
    for ep in episodes {
        *counts.entry(ep.season).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}
fn choose_season(current: Option<u32>, options: &[(u32, u32)]) -> Option<u32> {
    let available = |n| options.iter().any(|(s, count)| *s == n && *count > 0);
    current
        .filter(|n| available(*n))
        .or_else(|| {
            options
                .iter()
                .filter(|(n, count)| *n > 0 && *count > 0)
                .map(|(n, _)| *n)
                .min()
        })
        .or_else(|| {
            options
                .iter()
                .filter(|(_, count)| *count > 0)
                .map(|(n, _)| *n)
                .min()
        })
}
/// Parent window subscribes before navigation; recommendations never construct a
/// second coordinator or mutate Browse's saved collection context.
#[derive(Clone)]
pub(crate) struct OpenDetailTitle(pub BrowseTitle);
impl EventEmitter<OpenDetailTitle> for Detail {}

pub(crate) struct Detail {
    coordinator: Entity<AppCoordinator>,
    title: BrowseTitle,
    indexer: Entity<Select<IndexerKind>>,
    season_options: Vec<(u32, u32)>,
    season: Option<u32>,
    requested_season: Option<(u64, u32)>,
    episode: Option<CatalogEpisode>,
    target: Option<PlaybackTarget>,
    filter: Quality,
    expanded: bool,
    overview_open: bool,
    selected: Option<usize>,
    stream_focus: Vec<FocusHandle>,
    episode_focus: HashMap<(u32, u32), FocusHandle>,
    inline: Option<usize>,
    play: Option<Entity<PlaySheet>>,
    play_subscription: Option<Subscription>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    art_generation: u64,
    column_generation: u64,
    column_direction: f32,
    pending_column_focus: bool,
    episode_return_focus: Option<(u32, u32)>,
    back_focus: FocusHandle,
    #[cfg(feature = "ui-probe")]
    fixture_ambient: Option<artwork::ArtworkPalette>,
    _subscriptions: Vec<Subscription>,
}
impl Detail {
    pub(crate) fn new(
        coordinator: Entity<AppCoordinator>,
        title: BrowseTitle,
        cx: &mut Context<Self>,
    ) -> Self {
        let active = coordinator
            .read(cx)
            .settings
            .as_ref()
            .map(|s| s.indexer)
            .unwrap_or(IndexerKind::Torrentio);
        let indexer = cx.new(|cx| {
            Select::new(
                active,
                vec![
                    (IndexerKind::Torrentio, "Torrentio".into()),
                    (IndexerKind::Peerflix, "Peerflix".into()),
                ],
                cx,
            )
        });
        let subscriptions = vec![
            cx.observe(&coordinator, |this, _, cx| {
                this.reconcile_metadata(cx);
                cx.notify();
            }),
            cx.subscribe(&indexer, |this, _, event: &SelectEvent<IndexerKind>, cx| {
                let SelectEvent::Changed(indexer) = event;
                this.reset_stream_selection();
                if let Some(target) = this.target.clone() {
                    this.coordinator
                        .update(cx, |c, cx| c.sources(*indexer, target, cx));
                }
                cx.notify();
            }),
        ];
        coordinator.update(cx, |c, cx| c.title_details(&title, false, cx));
        let target = if title.kind() == MediaKind::Movie {
            title.imdb().cloned().map(PlaybackTarget::Movie)
        } else {
            None
        };
        if let Some(target) = target.clone() {
            coordinator.update(cx, |c, cx| c.sources(active, target, cx));
        }
        if let BrowseTitle::Tmdb(t) = &title
            && t.kind == MediaKind::Movie
        {
            coordinator.update(cx, |c, cx| c.load_similar(t.id, 1, false, cx));
        }
        Self {
            coordinator,
            title,
            indexer,
            season_options: Vec::new(),
            season: None,
            requested_season: None,
            episode: None,
            target,
            filter: Quality::All,
            expanded: false,
            overview_open: false,
            selected: None,
            stream_focus: Vec::new(),
            episode_focus: HashMap::new(),
            inline: None,
            play: None,
            play_subscription: None,
            bounds: Rc::new(Cell::new(None)),
            art_generation: artwork::begin_generation("v3-detail", cx),
            column_generation: 0,
            column_direction: 1.,
            pending_column_focus: false,
            episode_return_focus: None,
            back_focus: cx.focus_handle().tab_stop(true),
            #[cfg(feature = "ui-probe")]
            fixture_ambient: None,
            _subscriptions: subscriptions,
        }
    }
    pub(crate) fn name(&self) -> &str {
        self.title.name()
    }
    #[allow(dead_code, reason = "P3 reads the stable hero slot after layout")]
    pub(crate) fn hero_slot_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds.get()
    }
    /// Artwork lookup only queues bounded work; no decoding or palette sampling
    /// happens in the screen. Offline scenes never enqueue transport jobs.
    fn image(
        &self,
        url: Option<&str>,
        kind: ArtworkKind,
        fixture_key: &str,
        cx: &mut App,
    ) -> Option<std::sync::Arc<RenderImage>> {
        #[cfg(feature = "ui-probe")]
        if self.coordinator.read(cx).detail_is_fixture() {
            let c = self.coordinator.read(cx);
            return url
                .and_then(|u| c.fixture_art.get(u))
                .or_else(|| c.fixture_art.get(fixture_key))
                .cloned();
        }
        #[cfg(not(feature = "ui-probe"))]
        let _ = fixture_key;
        let key = ArtworkKey::new(kind, url?)?;
        if let Some(image) = artwork::get(&key, cx) {
            return Some(image);
        }
        artwork::request("v3-detail", self.art_generation, key, cx);
        None
    }
    /// Cinematic backdrop for the hero: the backdrop image when the provider has
    /// one, otherwise the poster. Only `Ready` is drawn; the request is queued
    /// under this screen's artwork generation.
    fn backdrop_state(
        &self,
        backdrop_url: Option<&str>,
        poster_url: Option<&str>,
        cx: &mut App,
    ) -> ArtworkState {
        #[cfg(feature = "ui-probe")]
        if self.coordinator.read(cx).detail_is_fixture() {
            let c = self.coordinator.read(cx);
            return backdrop_url
                .and_then(|u| c.fixture_art.get(u))
                .or_else(|| c.fixture_art.get(&format!("{}:backdrop", self.title.key())))
                .cloned()
                .map_or(ArtworkState::Failed, ArtworkState::Ready);
        }
        let key = backdrop_url
            .and_then(|url| ArtworkKey::new(ArtworkKind::Backdrop, url))
            .or_else(|| poster_url.and_then(|url| ArtworkKey::new(ArtworkKind::Poster, url)));
        match key {
            Some(key) => artwork::backdrop_or_request("v3-detail", self.art_generation, &key, cx),
            None => ArtworkState::Failed,
        }
    }
    /// Theme palette with this title's artwork wash applied to the surfaces.
    fn tinted_palette(&self, cx: &App) -> tokens::Palette {
        tokens::palette(cx).tinted(self.ambient_palette(cx), tokens::theme(cx).mode)
    }
    fn ambient_palette(&self, cx: &App) -> Option<artwork::ArtworkPalette> {
        #[cfg(feature = "ui-probe")]
        if self.coordinator.read(cx).detail_is_fixture() {
            return self.fixture_ambient;
        }
        let c = self.coordinator.read(cx);
        let backdrop = match &self.title {
            BrowseTitle::Cinemeta(_) => c.details.value().and_then(|d| d.background_url.as_deref()),
            BrowseTitle::Tmdb(_) => c
                .tmdb_details
                .value()
                .and_then(|d| d.backdrop_url.as_deref()),
        };
        backdrop
            .and_then(|url| ArtworkKey::new(ArtworkKind::Backdrop, url))
            .and_then(|key| artwork::ambient(&key, cx))
            .or_else(|| {
                self.display_title(cx)
                    .poster_url()
                    .and_then(|url| ArtworkKey::new(ArtworkKind::Poster, url))
                    .and_then(|key| artwork::ambient(&key, cx))
            })
    }
    fn display_title(&self, cx: &App) -> BrowseTitle {
        let c = self.coordinator.read(cx);
        match &self.title {
            BrowseTitle::Cinemeta(_) => c
                .details
                .value()
                .map(|d| BrowseTitle::Cinemeta(d.title.clone()))
                .unwrap_or_else(|| self.title.clone()),
            BrowseTitle::Tmdb(_) => c
                .tmdb_details
                .value()
                .map(|d| {
                    let mut t = d.title.clone();
                    t.imdb = d.imdb.clone();
                    BrowseTitle::Tmdb(t)
                })
                .unwrap_or_else(|| self.title.clone()),
        }
    }
    fn reset_stream_selection(&mut self) {
        self.filter = Quality::All;
        self.selected = None;
        self.inline = None;
    }
    fn reconcile_metadata(&mut self, cx: &mut Context<Self>) {
        let title = self.display_title(cx);
        if title.kind() == MediaKind::Movie {
            if self.target.is_none()
                && let Some(id) = title.imdb().cloned()
            {
                let target = PlaybackTarget::Movie(id);
                self.target = Some(target.clone());
                let indexer = *self.indexer.read(cx).value();
                self.coordinator
                    .update(cx, |c, cx| c.sources(indexer, target, cx));
            }
            return;
        }
        let options = match &self.title {
            BrowseTitle::Cinemeta(_) => self
                .coordinator
                .read(cx)
                .details
                .value()
                .map(|d| episode_seasons(&d.episodes)),
            BrowseTitle::Tmdb(_) => self.coordinator.read(cx).tmdb_details.value().map(|d| {
                let mut v = d
                    .seasons
                    .iter()
                    .filter(|s| s.episode_count > 0)
                    .map(|s| (s.number, s.episode_count))
                    .collect::<Vec<_>>();
                v.sort_unstable();
                v.dedup_by_key(|s| s.0);
                v
            }),
        };
        let Some(options) = options else {
            return;
        };
        let chosen = choose_season(self.season, &options);
        if self.season != chosen {
            self.coordinator.update(cx, |c, cx| {
                c.cancel_detail_sources(cx);
                if chosen.is_none() {
                    c.cancel_tmdb_season(cx);
                }
            });
            self.season = chosen;
            self.episode = None;
            self.target = None;
            self.reset_stream_selection();
            self.requested_season = None;
        }
        self.season_options = options;
        self.ensure_season(false, cx);
        if let Some(ep) = &self.episode {
            let episodes = self.episodes(cx);
            if !episodes
                .iter()
                .any(|e| e.season == ep.season && e.number == ep.number)
                && !self.coordinator.read(cx).tmdb_season.busy()
            {
                self.episode = None;
                self.target = None;
                self.coordinator
                    .update(cx, |c, cx| c.cancel_detail_sources(cx));
            }
        }
    }
    fn ensure_season(&mut self, refresh: bool, cx: &mut Context<Self>) {
        if let (BrowseTitle::Tmdb(t), Some(season)) = (&self.title, self.season) {
            let identity = (t.id, season);
            if refresh || self.requested_season != Some(identity) {
                self.requested_season = Some(identity);
                self.coordinator.update(cx, |c, cx| {
                    c.load_tmdb_season(identity.0, identity.1, refresh, cx)
                });
            }
        }
    }
    fn select_season(&mut self, n: u32, cx: &mut Context<Self>) {
        if self.season == Some(n) {
            return;
        }
        self.coordinator
            .update(cx, |c, cx| c.cancel_detail_sources(cx));
        self.season = Some(n);
        self.pending_column_focus = false;
        self.episode = None;
        self.target = None;
        self.reset_stream_selection();
        self.requested_season = None;
        self.column_generation += 1;
        self.column_direction = -1.;
        self.ensure_season(false, cx);
        cx.notify();
    }
    fn episodes(&self, cx: &App) -> Vec<CatalogEpisode> {
        let c = self.coordinator.read(cx);
        let episodes = match &self.title {
            BrowseTitle::Cinemeta(_) => c.details.value().map(|d| &d.episodes),
            BrowseTitle::Tmdb(t) => c
                .tmdb_season
                .value()
                .filter(|s| s.series_id == t.id && Some(s.season) == self.season)
                .map(|s| &s.episodes),
        };
        let mut episodes = episodes
            .into_iter()
            .flatten()
            .filter(|e| Some(e.season) == self.season)
            .cloned()
            .collect::<Vec<_>>();
        episodes.sort_by_key(|e| e.number);
        episodes
    }
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.art_generation = artwork::begin_generation("v3-detail", cx);
        self.coordinator
            .update(cx, |c, cx| c.title_details(&self.title, true, cx));
        self.ensure_season(true, cx);
        if let BrowseTitle::Tmdb(t) = &self.title
            && t.kind == MediaKind::Movie
        {
            let page = self
                .coordinator
                .read(cx)
                .similar
                .value()
                .map(|p| p.page)
                .unwrap_or(1);
            self.coordinator
                .update(cx, |c, cx| c.load_similar(t.id, page, true, cx));
        }
        if let Some(target) = self.target.clone() {
            let indexer = *self.indexer.read(cx).value();
            self.coordinator
                .update(cx, |c, cx| c.refresh_sources(indexer, target, cx));
        }
    }
    fn choose_episode(&mut self, episode: CatalogEpisode, cx: &mut Context<Self>) {
        if let Some(series) = self.display_title(cx).imdb().cloned() {
            let target = PlaybackTarget::Episode {
                series,
                season: episode.season,
                episode: episode.number,
            };
            self.target = Some(target.clone());
            self.episode_return_focus = Some((episode.season, episode.number));
            self.pending_column_focus = true;
            self.episode = Some(episode);
            self.reset_stream_selection();
            self.column_generation += 1;
            self.column_direction = 1.;
            let indexer = *self.indexer.read(cx).value();
            self.coordinator
                .update(cx, |c, cx| c.sources(indexer, target, cx));
            cx.notify();
        }
    }
    fn back_to_episodes(&mut self, cx: &mut Context<Self>) {
        self.pending_column_focus = true;
        self.episode = None;
        self.target = None;
        self.column_generation += 1;
        self.column_direction = -1.;
        self.coordinator
            .update(cx, |c, cx| c.cancel_detail_sources(cx));
        // Loaded season data is retained by cancellation. A still-loading season
        // must be reissued, without accepting its obsolete completion.
        self.requested_season = None;
        self.ensure_season(false, cx);
        cx.notify();
    }
    fn open_recommendation(&mut self, title: crate::tmdb::TmdbTitle, cx: &mut Context<Self>) {
        let intent = OpenDetailTitle(BrowseTitle::Tmdb(title));
        // Preserve provider scope even for externally injected fixture results.
        if matches!(&intent.0,BrowseTitle::Tmdb(t) if t.kind==MediaKind::Movie) {
            cx.emit(intent);
        }
    }
    fn open_stream(&mut self, index: usize, cx: &mut Context<Self>) {
        let candidate = self
            .coordinator
            .read(cx)
            .sources
            .value()
            .and_then(|s| s.candidates.get(index))
            .cloned();
        if let (Some(target), Some(candidate)) = (self.target.clone(), candidate) {
            let coordinator = self.coordinator.clone();
            let title = self.display_title(cx).name().to_owned();
            let context = PlayContext {
                episode: self.episode.as_ref().map(|ep| match ep.name.as_deref() {
                    Some(name) if !name.trim().is_empty() => {
                        format!("{} · {name}", episode_marker(ep))
                    }
                    _ => episode_marker(ep),
                }),
                poster: self
                    .display_title(cx)
                    .poster_url()
                    .map(str::to_owned)
                    .and_then(|url| {
                        self.image(Some(&url), ArtworkKind::Poster, &self.title.key(), cx)
                    }),
            };
            let sheet =
                cx.new(|cx| PlaySheet::new(coordinator, title, context, target, candidate, cx));
            self.play_subscription = Some(cx.subscribe(&sheet, |this, _, _: &DismissEvent, cx| {
                this.play = None;
                cx.notify();
            }));
            self.play = Some(sheet);
            self.selected = Some(index);
            cx.notify();
        }
    }
}
/// What the hero's primary button does and whether it is available.
enum PrimaryAction {
    /// Open the play sheet for this candidate index.
    Play(usize),
    /// Move to the episodes column.
    Episodes,
    /// Unavailable, with the reason shown as a tooltip.
    Blocked(&'static str),
}

impl Detail {
    fn is_series(&self) -> bool {
        self.title.kind() == MediaKind::Series
    }
    /// Loading / failure / emptiness of the title metadata resource in use.
    fn meta_status(&self, cx: &App) -> MetaStatus {
        let c = self.coordinator.read(cx);
        if matches!(self.title, BrowseTitle::Tmdb(_)) {
            MetaStatus {
                loaded: c.tmdb_details.value().is_some(),
                busy: c.tmdb_details.busy(),
                error: c.tmdb_details.error().map(str::to_owned),
                origin: c.tmdb_details.origin(),
                fetched_at: c.tmdb_details.fetched_at(),
            }
        } else {
            MetaStatus {
                loaded: c.details.value().is_some(),
                busy: c.details.busy(),
                error: c.details.error().map(str::to_owned),
                origin: c.details.origin(),
                fetched_at: c.details.fetched_at(),
            }
        }
    }
    fn primary_action(&self, cx: &App) -> PrimaryAction {
        if self.is_series() && self.episode.is_none() {
            return PrimaryAction::Episodes;
        }
        if self.target.is_none() {
            return PrimaryAction::Blocked(if self.meta_status(cx).busy {
                "Looking for streams…"
            } else {
                "Streams aren’t available for this title"
            });
        }
        let sources = &self.coordinator.read(cx).sources;
        match sources.value() {
            Some(set) if !set.candidates.is_empty() => {
                match first_in_filter(&set.candidates, self.filter) {
                    Some(index) => PrimaryAction::Play(index),
                    None => PrimaryAction::Blocked("No streams match the selected quality"),
                }
            }
            Some(_) if !sources.busy() => PrimaryAction::Blocked("No streams found"),
            _ if sources.error().is_some() => {
                PrimaryAction::Blocked("Streams couldn’t be loaded — try again")
            }
            _ => PrimaryAction::Blocked("Looking for streams…"),
        }
    }
    fn run_primary(&mut self, cx: &mut Context<Self>) {
        match self.primary_action(cx) {
            PrimaryAction::Play(index) => self.open_stream(index, cx),
            PrimaryAction::Episodes => self.focus_episodes(cx),
            PrimaryAction::Blocked(_) => {}
        }
    }
    /// Ask the next render to focus the first (or last visited) episode.
    fn focus_episodes(&mut self, cx: &mut Context<Self>) {
        if self.episode.is_some() {
            self.back_to_episodes(cx);
        } else {
            self.pending_column_focus = true;
            cx.notify();
        }
    }
    fn retry_metadata(&mut self, cx: &mut Context<Self>) {
        self.coordinator
            .update(cx, |c, cx| c.title_details(&self.title, true, cx));
        self.ensure_season(true, cx);
    }
    fn retry_streams(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.target.clone() {
            let indexer = *self.indexer.read(cx).value();
            self.coordinator
                .update(cx, |c, cx| c.refresh_sources(indexer, target, cx));
        } else {
            self.retry_metadata(cx);
        }
    }
    fn retry_similar(&mut self, cx: &mut Context<Self>) {
        if let BrowseTitle::Tmdb(t) = &self.title {
            let id = t.id;
            self.coordinator
                .update(cx, |c, cx| c.load_similar(id, 1, true, cx));
        }
    }

    /// Body of the action column: episodes, streams, or the unmapped state.
    fn action_body(&mut self, title: &BrowseTitle, cx: &mut Context<Self>) -> Div {
        if title.kind() == MediaKind::Series && self.episode.is_none() {
            self.episode_column(cx)
        } else if title.imdb().is_none() {
            self.unmapped_column(cx)
        } else {
            self.stream_column(cx)
        }
    }
}

/// Snapshot of the title-metadata resource, whichever provider backs it.
struct MetaStatus {
    loaded: bool,
    busy: bool,
    error: Option<String>,
    origin: crate::app_state::Origin,
    fetched_at: Option<u64>,
}

/// Selection policy shared by the async regions of this screen.
fn selection(has_value: bool, empty: bool, error: bool, busy: bool) -> ViewSelection {
    select_view(has_value, empty, error, busy)
}
/// A `StateView` positioned by `selection`; callers add skeleton/content/etc.
fn state_view(id: &'static str, sel: ViewSelection) -> StateView {
    StateView::new(id, sel.state)
        .refreshing(sel.refreshing)
        .stale_error(sel.stale_error)
}

impl Render for Detail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ambient = self.ambient_palette(cx);
        let mode = tokens::theme(cx).mode;
        // Surfaces that belong to this screen follow the artwork wash; shared
        // widgets keep the theme palette so ink contrast is unchanged.
        let p = self.tinted_palette(cx);
        let title = self.display_title(cx);
        let viewport = window.viewport_size();
        let stacked = viewport.width < geometry::STACK_AT;
        let share = viewport.height
            * if stacked {
                geometry::STACKED_HERO_MIN_FRACTION
            } else {
                geometry::HERO_MIN_FRACTION
            };
        // Wide layouts let the hero soak up whatever the column would leave empty.
        let fill = if stacked {
            Pixels::ZERO
        } else {
            viewport.height - s::TOOLBAR_HEIGHT - self.below_hero_height()
        };
        let hero_min = share.max(fill).max(geometry::HERO_MIN_HEIGHT);
        let hero = self.hero(hero_min, cx);
        let body = self.action_body(&title, cx);
        if self.pending_column_focus {
            let target = if self.episode.is_none() && title.kind() == MediaKind::Series {
                self.episode_return_focus
                    .and_then(|key| self.episode_focus.get(&key).cloned())
                    .or_else(|| {
                        self.episodes(cx)
                            .first()
                            .and_then(|e| self.episode_focus.get(&(e.season, e.number)).cloned())
                    })
            } else if !self.coordinator.read(cx).sources.busy() {
                if self
                    .coordinator
                    .read(cx)
                    .sources
                    .value()
                    .is_some_and(|s| !s.candidates.is_empty())
                {
                    self.stream_focus.first().cloned()
                } else {
                    Some(self.back_focus.clone())
                }
            } else {
                None
            };
            if let Some(target) = target {
                self.pending_column_focus = false;
                let weak = cx.entity().downgrade();
                let generation = self.column_generation;
                window.defer(cx, move |window, cx| {
                    let _ = weak.update(cx, |s, cx| {
                        if s.column_generation == generation {
                            target.focus(window, cx);
                        }
                    });
                });
            }
        }
        let duration = motion::duration(Motion::Column, cx);
        let direction = self.column_direction;
        let body = if self.column_generation > 0 && !duration.is_zero() {
            body.relative()
                .with_animation(
                    ("column-navigation", self.column_generation),
                    Animation::new(duration).with_easing(motion::ease_out_cubic),
                    move |el, t| el.left(s::S24 * direction * (1. - t)).opacity(t),
                )
                .into_any_element()
        } else {
            body.into_any_element()
        };
        let column = div()
            .id("detail-actions")
            .flex_none()
            .w(if viewport.width < geometry::NARROW_AT {
                geometry::NARROW_COLUMN
            } else {
                (viewport.width * geometry::COLUMN_FRACTION)
                    .min(geometry::MAX_COLUMN)
                    .min(viewport.width - geometry::MIN_HERO)
            })
            .h_full()
            .bg(p.bg_surface)
            .border_l_1()
            .border_color(p.line_hairline)
            .overflow_y_scroll()
            .when(stacked, |el| {
                el.w_full()
                    .h(viewport.height * geometry::STACKED_COLUMN_FRACTION)
            })
            .child(body);
        // The hero sizes to its content; there is no fixed-height band.
        let left = div()
            .id("detail-main")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(p.bg_canvas)
            .when(!stacked, |el| el.h_full().overflow_y_scroll())
            .when(stacked, |el| el.flex_none())
            .child(hero);
        let content = div()
            .id("detail-content")
            .relative()
            .size_full()
            .flex()
            .when(stacked, |el| el.flex_col().overflow_y_scroll())
            .child(left)
            .child(column);
        let duration = motion::duration(Motion::Content, cx);
        let content = if duration.is_zero() {
            content.into_any_element()
        } else {
            content
                .with_animation(
                    "detail-open",
                    Animation::new(duration).with_easing(motion::ease_out_cubic),
                    |el, t| el.opacity(t).top(s::S8 * (1. - t)),
                )
                .into_any_element()
        };
        crate::widgets::ambient_background::AmbientBackground::new(ambient, mode).child(
            div()
                .relative()
                .size_full()
                .child(content)
                .when_some(self.play.clone(), |el, play| el.child(play)),
        )
    }
}
