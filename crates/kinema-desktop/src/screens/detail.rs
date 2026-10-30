//! Production title detail. Route-local selection; the shared coordinator owns I/O.
use super::{
    coordinator::{AppCoordinator, BrowseTitle, PlaybackIntent},
    play_session::{PlaySession, PlaySessionEvent, format_bytes},
};
use crate::{
    artwork::{self, ArtworkKey, ArtworkKind, ArtworkState},
    widgets::{
        button::{Button, ButtonSize, ButtonVariant},
        icon::{Icon, IconName},
        icon_button::IconButton,
        menu::{ContextMenuExt, Select, SelectEvent},
        motion::{self, Motion},
        patterns::{
            Badge, BadgeTone, Section, Skeleton, StateView, ViewSelection, menu_item, select_view,
        },
        scroll_area::ScrollArea,
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
mod facts;
#[cfg(feature = "ui-probe")]
mod fixtures;
#[cfg(feature = "ui-probe")]
pub(crate) use fixtures::local_fixture_title;
mod hero;
#[cfg(all(test, feature = "ui-probe-test"))]
mod interaction_tests;
mod layout;
mod local;
mod personal;
mod progress;
mod recommendations;
mod streams;
mod streams_page;
#[cfg(test)]
mod tests;
use std::{cell::Cell, collections::HashMap, rc::Rc};

// Detail-specific geometry. Shared spacing, row, typography and chrome
// dimensions remain in widgets::tokens.
mod geometry {
    use gpui::{Pixels, px};
    /// Hero and body share one frame; it only centres beyond this width.
    pub const PAGE_MAX_WIDTH: Pixels = px(2400.);
    pub const EPISODE_CARD_MIN_WIDTH: Pixels = px(280.);
    pub const EPISODE_CARD_MAX_WIDTH: Pixels = px(360.);
    pub const EPISODE_GRID_GAP: Pixels = px(16.);
    pub const SIMILAR_MIN_WIDTH: Pixels = px(140.);
    pub const SIMILAR_MAX_WIDTH: Pixels = px(200.);
    pub const SIMILAR_GAP: Pixels = px(14.);
    pub const CAST_MIN_WIDTH: Pixels = px(112.);
    pub const CAST_MAX_WIDTH: Pixels = px(160.);
    pub const FACT_LABEL_WIDTH: Pixels = px(120.);
    /// Skeleton row counts, roughly one screenful.
    pub const STREAM_SKELETON_ROWS: usize = 6;
    /// Frame width from which the Streams inspector sits beside the list;
    /// narrower frames use the floating play dock.
    pub const STREAMS_INSPECTOR_MIN_FRAME: Pixels = px(1040.);
    pub const STREAMS_POSTER_WIDTH: Pixels = px(104.);
    pub const STREAMS_POSTER_WIDTH_NARROW: Pixels = px(80.);
    pub const EPISODE_SKELETON_CARDS: usize = 4;
}

pub(crate) use crate::settings::StreamQuality as Quality;
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

/// Which full-width page the detail route is showing. Streams is a real
/// navigation step: Back returns to `Overview` with everything intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Overview,
    Streams,
}

/// Columns and tile width of a grid whose rows fill the available width.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GridFit {
    cols: usize,
    tile: Pixels,
}
/// Fewest columns whose tiles are no wider than `max`, but never so many that
/// they get narrower than `min` (the `min` bound wins when the two conflict).
/// Tiles then share `available` exactly, minus the gaps.
fn fit_grid(available: Pixels, min: Pixels, max: Pixels, gap: Pixels) -> GridFit {
    let (a, lo, hi, g) = (
        f32::from(available),
        f32::from(min),
        f32::from(max),
        f32::from(gap),
    );
    let most = (((a + g) / (lo + g)).floor() as usize).max(1);
    let fewest = (((a + g) / (hi + g)).ceil() as usize).max(1);
    let cols = fewest.min(most);
    let tile = ((a - g * (cols as f32 - 1.)) / cols as f32).floor().max(1.);
    GridFit {
        cols,
        tile: px(tile),
    }
}
/// Side gutter of the page frame by window width.
fn gutter(viewport: Pixels) -> Pixels {
    let w = f32::from(viewport);
    px(match w {
        w if w < 720. => 12.,
        w if w < 1100. => 20.,
        w if w < 1500. => 28.,
        w if w < 2100. => 40.,
        _ => 48.,
    })
}
/// Left edge shared by the detail content and its floating toolbar.
pub(super) fn page_inset(viewport: Pixels) -> Pixels {
    (viewport - viewport.min(geometry::PAGE_MAX_WIDTH)) / 2. + gutter(viewport)
}
/// Width of page content inside the shared frame for a window of `viewport`.
fn content_width(viewport: Pixels) -> Pixels {
    viewport.min(geometry::PAGE_MAX_WIDTH) - gutter(viewport) * 2.
}

pub(crate) struct Detail {
    coordinator: Entity<AppCoordinator>,
    title: BrowseTitle,
    indexer: Entity<Select<IndexerKind>>,
    season_options: Vec<(u32, u32)>,
    season: Option<u32>,
    requested_season: Option<(u64, u32)>,
    episode: Option<CatalogEpisode>,
    target: Option<PlaybackTarget>,
    page: Page,
    filter: Quality,
    stream_page: streams_page::StreamPage,
    selected: Option<usize>,
    stream_focus: Vec<FocusHandle>,
    episode_focus: HashMap<(u32, u32), FocusHandle>,
    inline: Option<usize>,
    /// Inline playback session and the candidate index it plays; dropping
    /// it cancels its preparation/launch.
    play: Option<(usize, Entity<PlaySession>)>,
    play_subscriptions: Vec<Subscription>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    art_generation: u64,
    page_generation: u64,
    page_direction: f32,
    pending_stream_focus: bool,
    pending_return_focus: bool,
    /// "Play from beginning" chosen from a context menu, consumed by the next play.
    start_over_requested: bool,
    /// Pending Clear saved progress confirmation: `Some(None)` is the movie.
    clear_confirm: Option<Option<(u32, u32)>>,
    episode_return_focus: Option<(u32, u32)>,
    page_focus: FocusHandle,
    primary_focus: FocusHandle,
    scroll: ScrollHandle,
    streams_scroll: ScrollHandle,
    similar: recommendations::SimilarGrid,
    similar_cols: usize,
    /// Cast grid expanded past its two collapsed rows.
    cast_expanded: bool,
    /// Synchronous rejection: the exact rejected intent plus reason.
    write_error: Option<(personal::Write, String)>,
    /// Bulk season change waiting for its snapshot (applied immediately; no confirmation).
    season_confirm: Option<personal::SeasonConfirm>,
    /// The user picked a season (or acted on one); smart defaults no longer apply.
    season_chosen: bool,
    /// Season tile grid shows every season instead of the two-row window.
    season_expanded: bool,
    /// Season whose overview is shown in full (collapses on season change).
    season_overview_open: Option<u32>,
    /// Whole-series watched change: seasons still to write, the wanted value,
    /// and whether any season was written yet.
    series_queue: Vec<u32>,
    series_watched: bool,
    series_applied: bool,
    season_focus: HashMap<u32, FocusHandle>,
    /// Episodes whose personal state has been requested (per canonical show).
    episode_personal_requested:
        std::collections::HashSet<(kinema_core::library::TitleId, u32, u32)>,
    #[cfg(feature = "ui-probe")]
    fixture_ambient: Option<artwork::ArtworkPalette>,
    _subscriptions: Vec<Subscription>,
}
impl Detail {
    /// How far the overview body has scrolled under the floating header
    /// (zero on the Streams page, which is inset below the header instead).
    pub(crate) fn scrolled_under_header(&self) -> Pixels {
        if self.page == Page::Overview {
            (-self.scroll.offset().y).max(px(0.))
        } else {
            px(0.)
        }
    }
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
                this.reconcile_similar(cx);
                this.ensure_episode_personal(cx);
                this.apply_season_request(cx);
                cx.notify();
            }),
            cx.subscribe(&indexer, |this, _, event: &SelectEvent<IndexerKind>, cx| {
                let SelectEvent::Changed(indexer) = event;
                this.reset_stream_selection();
                this.pending_stream_focus = true;
                if let (Page::Streams, Some(target)) = (this.page, this.target.clone()) {
                    this.coordinator
                        .update(cx, |c, cx| c.sources(*indexer, target, cx));
                }
                cx.notify();
            }),
        ];
        coordinator.update(cx, |c, cx| c.title_details(&title, false, cx));
        // Streams are only requested once the Streams page opens.
        let target = if title.kind() == MediaKind::Movie {
            title.imdb().cloned().map(PlaybackTarget::Movie)
        } else {
            None
        };
        if let BrowseTitle::Tmdb(t) = &title {
            let (kind, id) = (t.kind, t.id);
            coordinator.update(cx, |c, cx| c.load_similar(kind, id, 1, false, cx));
        }
        let art_generation = artwork::begin_title_generation("v3-detail", &title.key(), cx);
        let mut this = Self {
            coordinator,
            title,
            indexer,
            season_options: Vec::new(),
            season: None,
            requested_season: None,
            episode: None,
            target,
            page: Page::Overview,
            filter: Quality::All,
            stream_page: streams_page::StreamPage::default(),
            selected: None,
            stream_focus: Vec::new(),
            episode_focus: HashMap::new(),
            inline: None,
            play: None,
            play_subscriptions: Vec::new(),
            bounds: Rc::new(Cell::new(None)),
            art_generation,
            page_generation: 0,
            page_direction: 1.,
            pending_stream_focus: false,
            pending_return_focus: false,
            start_over_requested: false,
            clear_confirm: None,
            episode_return_focus: None,
            page_focus: cx.focus_handle().tab_stop(true),
            primary_focus: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            streams_scroll: ScrollHandle::new(),
            similar: Default::default(),
            similar_cols: 6,
            cast_expanded: false,
            write_error: None,
            season_confirm: None,
            season_chosen: false,
            season_expanded: false,
            season_overview_open: None,
            series_queue: Vec::new(),
            series_watched: false,
            series_applied: false,
            season_focus: HashMap::new(),
            episode_personal_requested: Default::default(),
            #[cfg(feature = "ui-probe")]
            fixture_ambient: None,
            _subscriptions: subscriptions,
        };
        this.reconcile_similar(cx);
        this.ensure_personal(cx);
        this
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn name(&self) -> &str {
        self.title.name()
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn page(&self) -> Page {
        self.page
    }
    #[allow(dead_code, reason = "P3 reads the stable hero slot after layout")]
    pub(crate) fn hero_slot_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds.get()
    }
    /// True while the Streams sub-page is showing (header Back returns to details).
    pub(crate) fn on_streams_page(&self) -> bool {
        self.page == Page::Streams
    }
    /// Back from the Streams page to the detail page; false on the detail page
    /// itself so the route can leave Detail instead.
    pub(crate) fn go_back(&mut self, cx: &mut Context<Self>) -> bool {
        if self.page != Page::Streams {
            return false;
        }
        self.close_streams(cx);
        true
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
    /// Poster URL and ready backdrop image shared by the hero and the Streams
    /// page, so both draw the same artwork through one code path. A failed or
    /// pending backdrop is `None` (neutral canvas).
    fn hero_art(
        &self,
        title: &BrowseTitle,
        cx: &mut App,
    ) -> (Option<String>, Option<std::sync::Arc<RenderImage>>) {
        let (poster_url, backdrop_url) = {
            let c = self.coordinator.read(cx);
            let details = if matches!(self.title, BrowseTitle::Cinemeta(_)) {
                c.details.value()
            } else {
                None
            };
            let poster = details
                .and_then(|d| d.poster_url.as_deref())
                .or_else(|| title.poster_url())
                .map(str::to_owned);
            let backdrop = match &self.title {
                BrowseTitle::Cinemeta(_) => details.and_then(|d| d.background_url.clone()),
                BrowseTitle::Tmdb(_) => c.tmdb_details.value().and_then(|d| d.backdrop_url.clone()),
                BrowseTitle::Local(_) => self.local_backdrop(cx),
            };
            (poster, backdrop)
        };
        let backdrop = match self.backdrop_state(backdrop_url.as_deref(), poster_url.as_deref(), cx)
        {
            ArtworkState::Ready(image) => Some(image),
            _ => None,
        };
        (poster_url, backdrop)
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
            BrowseTitle::Local(_) => self
                .local_detail(cx)
                .and_then(|d| d.metadata.text("backdrop_url")),
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
            BrowseTitle::Local(_) => self
                .local_detail(cx)
                .map(|d| {
                    BrowseTitle::Local(super::coordinator::LocalBrowseTitle::new(
                        d.metadata.clone(),
                    ))
                })
                .unwrap_or_else(|| self.title.clone()),
        }
    }
    fn reset_stream_selection(&mut self) {
        self.filter = Quality::All;
        self.selected = None;
        self.inline = None;
    }
    /// Drop the chosen episode and, with it, any streams page for it.
    fn clear_episode(&mut self, cx: &mut Context<Self>) {
        self.episode = None;
        if self.is_series() {
            self.target = None;
        }
        if self.page == Page::Streams {
            self.page = Page::Overview;
            self.page_generation += 1;
            self.page_direction = -1.;
            self.play = None;
            self.play_subscriptions.clear();
        }
        self.coordinator
            .update(cx, |c, cx| c.cancel_detail_sources(cx));
    }
    fn reconcile_metadata(&mut self, cx: &mut Context<Self>) {
        let title = self.display_title(cx);
        if title.kind() == MediaKind::Movie {
            if self.target.is_none()
                && let Some(id) = title.imdb().cloned()
            {
                self.target = Some(PlaybackTarget::Movie(id));
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
            BrowseTitle::Local(_) => self.local_season_options(cx),
        };
        let Some(options) = options else {
            return;
        };
        // Explicit choices win; otherwise open on the next-up season. The default
        // locks once next-up is known so later toggling never moves the view.
        let next_up = self.next_up_in(&options, cx);
        let chosen = kinema_core::watch_progress::default_season(
            self.season.filter(|_| self.season_chosen),
            next_up,
            &options,
        )
        .or_else(|| choose_season(self.season, &options));
        if next_up.is_some() {
            self.season_chosen = true;
        }
        if self.season != chosen {
            self.coordinator.update(cx, |c, cx| {
                c.cancel_detail_sources(cx);
                if chosen.is_none() {
                    c.cancel_tmdb_season(cx);
                }
            });
            self.season = chosen;
            self.clear_episode(cx);
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
                self.clear_episode(cx);
            }
        }
    }
    /// Fold the coordinator's current similar-titles page into the grid.
    fn reconcile_similar(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = self.coordinator.read(cx).similar.value().cloned() {
            self.similar.merge(&page);
        }
    }
    fn ensure_season(&mut self, refresh: bool, cx: &mut Context<Self>) {
        if let (BrowseTitle::Local(_), Some(season)) = (&self.title, self.season)
            && let Some(id) = self.local_tmdb(cx)
        {
            let identity = (id, season);
            if refresh || self.requested_season != Some(identity) {
                // False when TMDB is inactive: saved local rows simply remain.
                let title = self.title.clone();
                if self
                    .coordinator
                    .update(cx, |c, cx| c.load_local_season(&title, season, refresh, cx))
                {
                    self.requested_season = Some(identity);
                }
            }
        }
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
        self.season = Some(n);
        self.season_confirm = None;
        self.clear_episode(cx);
        self.reset_stream_selection();
        self.requested_season = None;
        self.ensure_season(false, cx);
        self.ensure_episode_personal(cx);
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
            BrowseTitle::Local(_) => None,
        };
        let mut episodes = episodes
            .into_iter()
            .flatten()
            .filter(|e| Some(e.season) == self.season)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(season) = self.season.filter(|_| self.local_detail(cx).is_some()) {
            episodes.extend(self.local_season_episodes(season, cx));
        }
        episodes.sort_by_key(|e| e.number);
        episodes
    }
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.title.log_navigation("detail_refresh");
        artwork::refresh_misses("v3-detail", cx);
        self.art_generation = artwork::begin_title_generation("v3-detail", &self.title.key(), cx);
        self.coordinator
            .update(cx, |c, cx| c.title_details(&self.title, true, cx));
        self.ensure_season(true, cx);
        if let BrowseTitle::Tmdb(t) = &self.title {
            self.similar.reset();
            let (kind, id) = (t.kind, t.id);
            self.coordinator
                .update(cx, |c, cx| c.load_similar(kind, id, 1, true, cx));
        }
        if let (Page::Streams, Some(target)) = (self.page, self.target.clone()) {
            let indexer = *self.indexer.read(cx).value();
            self.coordinator
                .update(cx, |c, cx| c.refresh_sources(indexer, target, cx));
        }
    }
    /// Movies: open the Streams page for the movie.
    fn open_streams(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.go_to_streams(target, cx);
    }
    /// Series: clicking an episode opens the Streams page for it.
    fn open_episode(&mut self, episode: CatalogEpisode, cx: &mut Context<Self>) {
        if let Some(series) = self.display_title(cx).imdb().cloned() {
            let target = PlaybackTarget::Episode {
                series,
                season: episode.season,
                episode: episode.number,
            };
            self.episode_return_focus = Some((episode.season, episode.number));
            self.episode = Some(episode);
            self.go_to_streams(target, cx);
        }
    }
    fn go_to_streams(&mut self, target: PlaybackTarget, cx: &mut Context<Self>) {
        tracing::debug!(route = "streams", ?target, "UI route changed");
        self.stream_page = streams_page::StreamPage::default();
        let active = self
            .coordinator
            .read(cx)
            .settings
            .as_ref()
            .map(|s| s.indexer)
            .unwrap_or(IndexerKind::Torrentio);
        self.indexer
            .update(cx, |select, cx| select.set_value(active, cx));
        self.target = Some(target.clone());
        self.page = Page::Streams;
        self.reset_stream_selection();
        self.pending_stream_focus = true;
        self.page_generation += 1;
        self.page_direction = 1.;
        let indexer = *self.indexer.read(cx).value();
        self.coordinator
            .update(cx, |c, cx| c.sources(indexer, target, cx));
        cx.notify();
    }
    fn close_streams(&mut self, cx: &mut Context<Self>) {
        self.start_over_requested = false;
        self.stream_page = streams_page::StreamPage::default();
        tracing::debug!(route = "detail", "UI route changed");
        self.page = Page::Overview;
        self.play = None;
        self.play_subscriptions.clear();
        self.reset_stream_selection();
        self.episode = None;
        if self.is_series() {
            self.target = None;
        }
        self.pending_return_focus = true;
        self.page_generation += 1;
        self.page_direction = -1.;
        self.coordinator
            .update(cx, |c, cx| c.cancel_detail_sources(cx));
        // A season still loading when the page opened must be reissued,
        // without accepting its obsolete completion.
        self.requested_season = None;
        self.ensure_season(false, cx);
        cx.notify();
    }
    fn open_recommendation(&mut self, title: crate::tmdb::TmdbTitle, cx: &mut Context<Self>) {
        let intent = OpenDetailTitle(BrowseTitle::Tmdb(title));
        // Preserve provider scope even for externally injected fixture results.
        if matches!(&intent.0,BrowseTitle::Tmdb(t) if t.kind==self.title.kind()) {
            cx.emit(intent);
        }
    }
    /// "Show more" on the similar grid: reveal more rows, paging in more titles
    /// when the loaded ones run out.
    fn show_more_similar(&mut self, cx: &mut Context<Self>) {
        let cols = self.similar_cols;
        if let (Some(page), BrowseTitle::Tmdb(t)) = (self.similar.show_more(cols), &self.title) {
            let (kind, id) = (t.kind, t.id);
            self.coordinator
                .update(cx, |c, cx| c.load_similar(kind, id, page, false, cx));
        }
        cx.notify();
    }
    /// Start an inline play session, preserving the active-account save generation.
    fn open_stream(
        &mut self,
        index: usize,
        intent: PlaybackIntent,
        switch: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let candidate = self
            .coordinator
            .read(cx)
            .sources
            .value()
            .and_then(|s| s.candidates.get(index))
            .cloned();
        if let (Some(target), Some(candidate)) = (self.target.clone(), candidate) {
            tracing::debug!(indexer = ?self.indexer.read(cx).value(), ?intent, quality = super::coordinator::diagnostic_quality(candidate.quality.as_deref()), ?target, "UI stream chosen");
            self.stop_play(cx);
            let coordinator = self.coordinator.clone();
            let title = self.display_title(cx).name().to_owned();
            let episode_title = self.episode.as_ref().and_then(|e| e.name.clone());
            let session = cx.new(|cx| {
                PlaySession::new(
                    coordinator,
                    title,
                    episode_title,
                    target,
                    candidate,
                    intent,
                    cx,
                )
                .after_switch(switch)
            });
            self.play_subscriptions = vec![
                cx.subscribe(&session, |this, _, _: &DismissEvent, cx| this.stop_play(cx)),
                cx.subscribe(&session, move |this, _, event: &PlaySessionEvent, cx| {
                    let PlaySessionEvent::Retry = event;
                    this.play_stream_row(index, None, cx);
                }),
                cx.observe(&session, |_, _, cx| cx.notify()),
            ];
            self.play = Some((index, session));
            self.selected = Some(index);
            cx.notify();
        }
    }
    /// Drop the inline session; its release hook cancels owned work.
    fn stop_play(&mut self, cx: &mut Context<Self>) {
        self.play_subscriptions.clear();
        if self.play.take().is_some() {
            cx.notify();
        }
    }
}

/// What the hero's primary button does and whether it is available.
enum PrimaryAction {
    /// Open the Streams page (movies).
    Streams,
    /// Scroll to the episodes (series).
    Episodes,
    /// Open the Streams page for the next episode to watch (series).
    Continue(u32, u32),
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
        if matches!(self.title, BrowseTitle::Local(_)) {
            MetaStatus {
                loaded: c.local_details.value().is_some(),
                busy: c.local_details.busy() || c.local_acquisition_busy(),
                error: c
                    .local_details
                    .error()
                    .or(c.local_acquisition_error())
                    .map(str::to_owned),
                origin: c.local_details.origin(),
                fetched_at: c.local_details.fetched_at(),
            }
        } else if matches!(self.title, BrowseTitle::Tmdb(_)) {
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
        if self.is_series() {
            return match self.next_up(cx) {
                Some((season, episode)) if self.display_title(cx).imdb().is_some() => {
                    PrimaryAction::Continue(season, episode)
                }
                _ => PrimaryAction::Episodes,
            };
        }
        if self.target.is_none() {
            return PrimaryAction::Blocked(if self.meta_status(cx).busy {
                "Looking for streams…"
            } else {
                "Streams aren’t available for this title"
            });
        }
        PrimaryAction::Streams
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn run_primary_for_test(&mut self, cx: &mut Context<Self>) {
        self.run_primary(cx)
    }
    fn run_primary(&mut self, cx: &mut Context<Self>) {
        match self.primary_action(cx) {
            PrimaryAction::Streams => self.open_streams(cx),
            PrimaryAction::Episodes => self.focus_episodes(cx),
            PrimaryAction::Continue(season, number) => self.continue_episode(season, number, cx),
            PrimaryAction::Blocked(_) => {}
        }
    }
    /// Open the Streams page for an episode, like clicking its card. An episode
    /// not loaded yet (other season) still opens by its coordinates.
    fn continue_episode(&mut self, season: u32, number: u32, cx: &mut Context<Self>) {
        let episode = self
            .all_episodes(cx)
            .into_iter()
            .find(|e| e.season == season && e.number == number)
            .unwrap_or(CatalogEpisode {
                season,
                number,
                name: None,
                overview: None,
                released: None,
                runtime_minutes: None,
                still_url: None,
            });
        self.open_episode(episode, cx);
    }
    /// Scroll the page to the episodes section and focus its first card.
    fn focus_episodes(&mut self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_top_of_item(EPISODES_ITEM);
        self.pending_return_focus = true;
        cx.notify();
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
            let (kind, id) = (t.kind, t.id);
            let page = self.similar.next_page().unwrap_or(1);
            self.coordinator
                .update(cx, |c, cx| c.load_similar(kind, id, page, true, cx));
        }
    }
}

/// Index of the episodes section among the scroll area's children
/// (series: hero, episodes, about, similar, …).
const EPISODES_ITEM: usize = 1;

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

impl Detail {
    /// Move keyboard focus after a page change, once the target exists.
    fn apply_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = if self.page == Page::Streams {
            if !self.pending_stream_focus || self.coordinator.read(cx).sources.busy() {
                None
            } else {
                self.coordinator
                    .read(cx)
                    .sources
                    .value()
                    .and_then(|sources| {
                        self.stream_indices(sources, cx)
                            .first()
                            .and_then(|i| self.stream_focus.get(*i).cloned())
                    })
                    .or_else(|| Some(self.page_focus.clone()))
            }
        } else if !self.pending_return_focus {
            None
        } else if self.is_series() {
            self.episode_return_focus
                .and_then(|key| self.episode_focus.get(&key).cloned())
                .or_else(|| {
                    self.episodes(cx)
                        .first()
                        .and_then(|e| self.episode_focus.get(&(e.season, e.number)).cloned())
                })
        } else {
            Some(self.primary_focus.clone())
        };
        if let Some(target) = target {
            self.pending_stream_focus = false;
            self.pending_return_focus = false;
            let weak = cx.entity().downgrade();
            let generation = self.page_generation;
            window.defer(cx, move |window, cx| {
                let _ = weak.update(cx, |s, cx| {
                    if s.page_generation == generation {
                        target.focus(window, cx);
                    }
                });
            });
        }
    }
}

impl Render for Detail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ambient = self.ambient_palette(cx);
        let mode = tokens::theme(cx).mode;
        let page = match self.page {
            Page::Overview => self.overview_page(window, cx),
            Page::Streams => self.streams_page(window, cx),
        };
        self.apply_pending_focus(window, cx);
        let duration = motion::duration(Motion::Column, cx);
        let direction = self.page_direction;
        // Overview content runs under the transparent header (hero art reaches
        // the window top). The Streams page paints its own background to y=0 and
        // insets its content, so the header blends without a seam.
        let page = div().relative().size_full().child(page);
        let page = if self.page_generation > 0 && !duration.is_zero() {
            page.with_animation(
                ("detail-page", self.page_generation),
                Animation::new(duration).with_easing(motion::ease_out_cubic),
                move |el, t| el.left(s::S24 * direction * (1. - t)).opacity(t),
            )
            .into_any_element()
        } else {
            page.into_any_element()
        };
        let content = div()
            .id("detail-content")
            .relative()
            .size_full()
            .child(page);
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
        crate::widgets::ambient_background::AmbientBackground::new(ambient, mode)
            .child(div().relative().size_full().child(content))
    }
}
