//! Production title detail. Route-local selection; the shared coordinator owns I/O.
use super::{
    browse::menu_item,
    coordinator::{AppCoordinator, BrowseTitle},
    play_sheet::PlaySheet,
};
use crate::{
    artwork::{self, ArtworkKey, ArtworkKind},
    widgets::{
        chip::Chip,
        icon::{Icon, IconName},
        menu::{ContextMenuExt, Select, SelectEvent},
        motion::{self, Motion},
        segmented::Segmented,
        text::TypeExt,
        tokens::{self, space as s, typeface as t},
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

// Detail-specific geometry tokens. Shared spacing, row, typography and chrome
// dimensions remain in widgets::tokens; these follow §4.5's hero/breakpoints.
mod geometry {
    use gpui::{Pixels, px};
    pub const POSTER_WIDTH: Pixels = px(220.);
    pub const POSTER_HEIGHT: Pixels = px(330.);
    pub const SYNOPSIS_MEASURE: Pixels = px(640.);
    pub const NARROW_COLUMN: Pixels = px(340.);
    pub const MAX_COLUMN: Pixels = px(520.);
    pub const MIN_HERO: Pixels = px(620.);
    pub const EPISODE_ART_WIDTH: Pixels = px(64.);
    pub const EPISODE_ART_HEIGHT: Pixels = px(36.);
    pub const RECOMMENDATION_WIDTH: Pixels = px(120.);
    pub const RECOMMENDATION_HEIGHT: Pixels = px(180.);
    pub const NARROW_AT: Pixels = px(1100.);
    pub const STACK_AT: Pixels = px(960.);
    pub const HERO_FRACTION: f32 = 0.60;
    pub const FADE_FRACTION: f32 = 0.40;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Quality {
    All,
    Uhd,
    Hd,
    Sd,
    Other,
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
/// Only resolution/dynamic-range/codec are uppercase chips. Byte sizes and seed
/// counts are ordinary mono metadata; they must not pass through Chip::quality.
fn release_hints(c: &SourceCandidate) -> Vec<&'static str> {
    let text = format!("{} {}", c.name, c.quality.as_deref().unwrap_or("")).to_ascii_lowercase();
    let tokens: Vec<_> = text.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let mut hints = Vec::new();
    match source_quality(c) {
        Quality::Uhd => hints.push("2160P"),
        Quality::Hd => hints.push("1080P"),
        Quality::Sd => hints.push("720P"),
        _ => {}
    }
    if tokens.contains(&"dv") {
        hints.push("DV");
    } else if tokens.contains(&"hdr") {
        hints.push("HDR");
    }
    if tokens
        .iter()
        .any(|t| matches!(*t, "hevc" | "x265" | "h265"))
    {
        hints.push("HEVC");
    } else if tokens.iter().any(|t| matches!(*t, "x264" | "h264")) {
        hints.push("H.264");
    } else if tokens.contains(&"av1") {
        hints.push("AV1");
    }
    hints
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
fn title_facts(title: &BrowseTitle) -> String {
    let mut facts = Vec::new();
    if let Some(year) = title.year() {
        facts.push(year.to_string());
    }
    if title.kind() == MediaKind::Series {
        if let Some(n) = title.seasons() {
            facts.push(format!("{n} seasons"));
        }
    } else if let Some(n) = title.runtime() {
        facts.push(format!("{n} min"));
    }
    facts.extend(title.genres().iter().cloned());
    facts.join(" · ")
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
    more: bool,
    selected: Option<usize>,
    hovered: Option<usize>,
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
            more: false,
            selected: None,
            hovered: None,
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
        self.hovered = None;
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
            let sheet = cx.new(|cx| PlaySheet::new(coordinator, title, target, candidate, cx));
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
impl Render for Detail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let title = self.display_title(cx);
        let stacked = window.viewport_size().width < geometry::STACK_AT;
        let hero = self.hero(window, cx);
        let mut body = if title.kind() == MediaKind::Series && self.episode.is_none() {
            self.episode_column(cx)
        } else {
            self.stream_column(cx)
        };
        if title.imdb().is_none() {
            let c = self.coordinator.read(cx);
            let busy = if matches!(self.title, BrowseTitle::Tmdb(_)) {
                c.tmdb_details.busy()
            } else {
                c.details.busy()
            };
            let failed = if matches!(self.title, BrowseTitle::Tmdb(_)) {
                c.tmdb_details.error().is_some()
            } else {
                c.details.error().is_some()
            };
            // Episodes remain browsable without an IMDb mapping. Only stream
            // lookup is unavailable; don't hide the series season controls.
            let notice = if busy {
                "Looking up IMDb mapping…"
            } else if failed {
                "IMDb mapping could not be loaded. Refresh to retry."
            } else {
                "Streams unavailable — no IMDb ID for this title."
            };
            body = div()
                .flex()
                .flex_col()
                .child(div().p(s::S12).child(notice))
                .when(title.kind() == MediaKind::Series, |el| el.child(body));
        }
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
            .w(if window.viewport_size().width < geometry::NARROW_AT {
                geometry::NARROW_COLUMN
            } else {
                (window.viewport_size().width * 0.375)
                    .min(geometry::MAX_COLUMN)
                    .min(window.viewport_size().width - geometry::MIN_HERO)
            })
            .h_full()
            .bg(p.bg_surface)
            .border_l_1()
            .border_color(p.line_hairline)
            .overflow_y_scroll()
            .when(stacked, |el| {
                el.w_full()
                    .h(window.viewport_size().height * geometry::HERO_FRACTION)
            })
            .child(body);
        let content = div()
            .id("detail-content")
            .relative()
            .size_full()
            .flex()
            .when(stacked, |el| el.flex_col().overflow_y_scroll())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when(stacked, |el| {
                        el.flex_none().h(window.viewport_size().height)
                    })
                    .child(hero),
            )
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
        crate::widgets::ambient_background::AmbientBackground::new(
            self.ambient_palette(cx),
            tokens::theme(cx).mode,
        )
        .child(
            div()
                .relative()
                .size_full()
                .child(content)
                .when_some(self.play.clone(), |el, play| el.child(play)),
        )
    }
}
