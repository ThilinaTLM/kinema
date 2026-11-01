//! Detail episodes presentation.
use super::*;
use crate::{
    ui::blocks::ViewState,
    ui::blocks::media_card::MediaCard,
    ui::blocks::overlay::OverlayBadge,
    ui::blocks::state_toggle::ToggleStatus,
    ui::blocks::thumb::{Thumb, ThumbState},
    ui::blocks::watch_mark::{WatchMark, WatchMarkState},
    ui::blocks::watched_toggle::WatchedToggle,
    ui::motion::interaction::focus_ring,
    ui::primitives::notice::{Notice, NoticeKind},
    ui::theme::TypeStyle,
};
use kinema_core::watch_progress::{self, Tally, WatchedState};

/// Season tile grid geometry (episodes revision 2026-10): 12 px gaps.
const SEASON_TILE_MIN: Pixels = px(184.);
const SEASON_TILE_MAX: Pixels = px(300.);
const SEASON_TILE_GAP: Pixels = px(12.);
const SEASON_POSTER_W: Pixels = px(40.);
const SEASON_HEADER_POSTER_W: Pixels = px(72.);
/// "Season N" in the season header: 20/26 semibold.
const SEASON_TITLE: TypeStyle = TypeStyle {
    size: px(20.),
    line_height: px(26.),
    weight: FontWeight(600.),
    mono: false,
};
/// Longer overviews get a More/Less toggle; three lines hold roughly this much.
const OVERVIEW_TOGGLE_CHARS: usize = 220;
const POSTER_RATIO: f32 = 2. / 3.;
const STILL_RATIO: f32 = 16. / 9.;

/// Whether an overview is long enough to need More/Less.
pub(super) fn overview_needs_toggle(text: &str) -> bool {
    text.chars().count() > OVERVIEW_TOGGLE_CHARS
}

/// `2008-10-14` -> `14 Oct 2008`; anything unparseable yields `None`.
pub(super) fn display_date(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = iso.get(..10)?.split('-');
    let year: u32 = parts.next()?.parse().ok()?;
    let month: usize = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let month = MONTHS.get(month.checked_sub(1)?)?;
    (day >= 1).then(|| format!("{day} {month} {year}"))
}

/// Presentation state for a resolved artwork state. A load that has not been
/// pending long enough (`skeleton_due == false`) shows its fallback, or nothing,
/// so quick cache hits never flash a skeleton.
pub(super) fn thumb_state<T>(
    art: ArtState<T>,
    skeleton_due: bool,
    missing_label: impl Into<SharedString>,
) -> ThumbState<T> {
    match art {
        ArtState::Ready(image) => ThumbState::Ready(image),
        ArtState::Fallback(image) => ThumbState::Fallback(image),
        ArtState::Loading { fallback } if skeleton_due => ThumbState::Loading { fallback },
        ArtState::Loading { fallback } => fallback.map_or(ThumbState::Blank, ThumbState::Fallback),
        ArtState::Missing => ThumbState::Missing(missing_label.into()),
    }
}

/// `2013` or `2013–2015` from release dates; unparseable dates are skipped.
pub(super) fn year_span<'a>(released: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let years = released
        .into_iter()
        .filter_map(|d| air_date(d)?.get(..4)?.parse::<u16>().ok());
    let (lo, hi) = years.fold((u16::MAX, 0), |(lo, hi), y| (lo.min(y), hi.max(y)));
    match (lo <= hi, lo == hi) {
        (false, _) => None,
        (true, true) => Some(lo.to_string()),
        (true, false) => Some(format!("{lo}\u{2013}{hi}")),
    }
}

/// Display order of season tiles: regular seasons ascending, Specials last.
pub(super) fn tile_order(options: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut v = options.to_vec();
    v.sort_by_key(|(n, _)| (*n == 0, *n));
    v
}

/// One season tile's display data.
struct SeasonTile {
    number: u32,
    count: u32,
    year: Option<String>,
    poster: Option<String>,
    tally: Option<Tally>,
    next_up: bool,
}

impl SeasonTile {
    fn complete(&self) -> bool {
        self.tally.and_then(Tally::state) == Some(WatchedState::All)
    }
}

/// Placeholder shaped like the episode grid: 16:9 still and two text lines.
fn episode_skeleton(fit: GridFit) -> AnyElement {
    let (tile, cols) = (fit.tile, fit.cols);
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap(geometry::EPISODE_GRID_GAP)
        .children((0..geometry::EPISODE_SKELETON_CARDS.max(cols)).map(|_| {
            div()
                .w(tile)
                .flex()
                .flex_col()
                .gap(s::S8)
                .child(
                    Skeleton::block()
                        .w(tile)
                        .h(tile * 9. / 16.)
                        .rounded(radius::MD),
                )
                .child(Skeleton::line().w(relative(0.6)))
                .child(Skeleton::line().w(relative(0.9)))
                .into_any_element()
        }))
        .into_any_element()
}

/// `The Signal`, or a stand-in for unnamed episodes.
fn episode_name(ep: &CatalogEpisode) -> String {
    ep.name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Untitled episode")
        .to_owned()
}

/// `48 min · 2 Mar 2024` — absent parts skipped.
pub(super) fn episode_meta(ep: &CatalogEpisode) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(n) = ep.runtime_minutes {
        parts.push(format!("{n} min"));
    }
    if let Some(date) = ep.released.as_deref().and_then(air_date) {
        parts.push(display_date(date).unwrap_or_else(|| date.to_owned()));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

pub(super) fn episode_overview(ep: &CatalogEpisode) -> Option<String> {
    ep.overview
        .as_deref()
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(|o| o.replace('\n', " "))
}

// Artwork state plumbing: keep separate from card layout/restyling.
use kinema_core::artwork::{ArtState, resolve};
pub(super) type DetailArtState = ArtState<std::sync::Arc<gpui::RenderImage>>;

/// Candidate URL, its kind and (probe fixtures only) a lazily built fixture key.
type ArtChoice<'a> = (Option<&'a str>, ArtworkKind, &'a dyn Fn() -> String);

impl Detail {
    fn art_chain(&self, choices: &[ArtChoice<'_>], cx: &mut App) -> DetailArtState {
        resolve(choices.iter().map(|(url, kind, fixture)| {
            let Some(url) = url else {
                return ArtState::Missing;
            };
            #[cfg(feature = "ui-probe")]
            if self.coordinator.read(cx).detail_is_fixture() {
                return self
                    .image(Some(url), *kind, &fixture(), cx)
                    .map_or(ArtState::Missing, ArtState::Ready);
            }
            let _ = fixture;
            let Some(key) = ArtworkKey::new(*kind, url) else {
                return ArtState::Missing;
            };
            #[cfg(all(test, feature = "ui-probe-test"))]
            super::work::count(super::work::Section::ArtLookup, 1);
            if let Some(image) = artwork::get(&key, cx) {
                return ArtState::Ready(image);
            }
            if matches!(artwork::status(&key, cx), Some(Err(_))) {
                return ArtState::Missing;
            }
            artwork::request("v3-detail", self.art_generation, key, cx);
            ArtState::Loading { fallback: None }
        }))
    }

    pub(super) fn season_art_state(
        &self,
        season: u32,
        poster: Option<&str>,
        cx: &mut App,
    ) -> DetailArtState {
        let show = self.display_title(cx);
        self.art_chain(
            &[
                (poster, ArtworkKind::Poster, &|| format!("season:{season}")),
                (show.poster_url(), ArtworkKind::Poster, &|| self.title.key()),
            ],
            cx,
        )
    }

    fn episode_series_backdrop(&self, cx: &App) -> Option<String> {
        let c = self.coordinator.read(cx);
        match &self.title {
            BrowseTitle::Cinemeta(_) => c.details.value().and_then(|d| d.background_url.clone()),
            BrowseTitle::Tmdb(_) => c.tmdb_details.value().and_then(|d| d.backdrop_url.clone()),
            BrowseTitle::Local(_) => self.local_backdrop(cx),
        }
    }

    /// Inputs shared by every episode of one season, gathered once per
    /// section build. Artwork results themselves are not cached: they are
    /// looked up per card so completed loads show on the next repaint.
    pub(super) fn episode_art_ctx(&self, season: u32, cx: &App) -> EpisodeArtCtx {
        EpisodeArtCtx {
            season,
            poster: self.season_poster(season, cx),
            series_backdrop: self.episode_series_backdrop(cx),
            season_backdrop: self.local_detail(cx).and_then(|detail| {
                detail
                    .seasons
                    .iter()
                    .find(|s| s.number == season)?
                    .text("backdrop_url")
                    .map(str::to_owned)
            }),
        }
    }

    pub(super) fn episode_art_state(
        &self,
        art: &EpisodeArtCtx,
        ep: &CatalogEpisode,
        cx: &mut App,
    ) -> DetailArtState {
        let season = art.season;
        // TMDB exposes no season backdrop: prefer the series landscape before
        // the season portrait. Never use the series portrait as an episode still.
        self.art_chain(
            &[
                (ep.still_url.as_deref(), ArtworkKind::Still, &|| {
                    format!("episode:{}:{}", ep.season, ep.number)
                }),
                (
                    art.season_backdrop.as_deref(),
                    ArtworkKind::Backdrop,
                    &|| format!("season:{season}:backdrop"),
                ),
                (
                    art.series_backdrop.as_deref(),
                    ArtworkKind::Backdrop,
                    &|| format!("{}:backdrop", self.title.key()),
                ),
                (art.poster.as_deref(), ArtworkKind::Poster, &|| {
                    format!("season:{season}")
                }),
            ],
            cx,
        )
    }
}

/// See [`Detail::episode_art_ctx`].
pub(super) struct EpisodeArtCtx {
    season: u32,
    poster: Option<String>,
    series_backdrop: Option<String>,
    season_backdrop: Option<String>,
}

impl Detail {
    /// Whether any artwork behind this episode's chain has waited long enough
    /// to deserve a skeleton.
    fn episode_skeleton_due(&self, art: &EpisodeArtCtx, ep: &CatalogEpisode, cx: &mut App) -> bool {
        [
            (ep.still_url.as_deref(), ArtworkKind::Still),
            (art.series_backdrop.as_deref(), ArtworkKind::Backdrop),
            (art.season_backdrop.as_deref(), ArtworkKind::Backdrop),
            (art.poster.as_deref(), ArtworkKind::Poster),
        ]
        .into_iter()
        .any(|(url, kind)| {
            url.and_then(|u| ArtworkKey::new(kind, u))
                .is_some_and(|key| artwork::skeleton_due(&key, cx))
        })
    }

    /// The episode still as a [`ThumbState`] (label `E03` when nothing exists).
    fn episode_thumb_state(
        &self,
        art: &EpisodeArtCtx,
        ep: &CatalogEpisode,
        cx: &mut App,
    ) -> ThumbState {
        let state = self.episode_art_state(art, ep, cx);
        let due =
            matches!(state, ArtState::Loading { .. }) && self.episode_skeleton_due(art, ep, cx);
        thumb_state(state, due, format!("E{:02}", ep.number))
    }

    /// A season poster as a [`ThumbState`] (label is the season number).
    fn season_thumb_state(&self, season: u32, poster: Option<&str>, cx: &mut App) -> ThumbState {
        let art = self.season_art_state(season, poster, cx);
        let due = matches!(art, ArtState::Loading { .. }) && {
            let show = self.display_title(cx);
            [poster, show.poster_url()].into_iter().any(|url| {
                url.and_then(|u| ArtworkKey::new(ArtworkKind::Poster, u))
                    .is_some_and(|key| artwork::skeleton_due(&key, cx))
            })
        };
        let label = if season == 0 {
            "S".to_owned()
        } else {
            season.to_string()
        };
        thumb_state(art, due, label)
    }

    fn episode_streams_unavailable(&self, cx: &App) -> bool {
        // Use the enriched details, just like `open_episode`: a TMDB browse
        // result may not carry the IMDb ID resolved by the details request.
        self.display_title(cx).imdb().is_none() && !self.meta_status(cx).busy
    }

    /// Every episode the provider has listed so far, across seasons.
    pub(super) fn all_episodes(&self, cx: &App) -> Vec<CatalogEpisode> {
        #[cfg(all(test, feature = "ui-probe-test"))]
        super::work::count(super::work::Section::AllEpisodes, 1);
        let c = self.coordinator.read(cx);
        match &self.title {
            BrowseTitle::Cinemeta(_) => c
                .details
                .value()
                .map(|d| d.episodes.clone())
                .unwrap_or_default(),
            BrowseTitle::Tmdb(t) => c
                .tmdb_season
                .value()
                .filter(|s| s.series_id == t.id)
                .map(|s| s.episodes.clone())
                .unwrap_or_default(),
            BrowseTitle::Local(_) => self.local_all_episodes(cx),
        }
    }

    /// `(season, episode)` to continue with, from the episodes whose watched
    /// state is known. Shared with the hero ("Continue · S6 E9").
    #[allow(dead_code, reason = "consumed by the series hero Continue action")]
    pub(super) fn next_up(&self, cx: &App) -> Option<(u32, u32)> {
        self.next_up_in(&self.season_options, cx)
    }
    /// Continue / Next up target (resumed episode first) against explicit `(season, episode_count)` options.
    pub(super) fn next_up_in(&self, options: &[(u32, u32)], cx: &App) -> Option<(u32, u32)> {
        if options == self.season_options.as_slice() {
            return self.show_snapshot(cx).next_up;
        }
        self.continue_target_in(options, cx)
    }

    pub(super) fn season_poster(&self, season: u32, cx: &App) -> Option<String> {
        let c = self.coordinator.read(cx);
        match &self.title {
            BrowseTitle::Tmdb(_) | BrowseTitle::Local(_) => c
                .tmdb_season
                .value()
                .filter(|s| s.season == season)
                .and_then(|s| s.poster_url.clone()),
            BrowseTitle::Cinemeta(_) => None,
        }
        .or_else(|| self.tmdb_season_summary(season, cx)?.poster_url)
        .or_else(|| {
            self.local_detail(cx)?
                .seasons
                .iter()
                .find(|s| s.number == season)?
                .text("poster_url")
                .map(str::to_owned)
        })
    }

    fn season_overview(&self, season: u32, cx: &App) -> Option<String> {
        if let Some(text) = self
            .tmdb_season_summary(season, cx)
            .and_then(|s| s.overview)
            .map(|o| o.trim().replace('\n', " "))
            .filter(|o| !o.is_empty())
        {
            return Some(text);
        }
        self.local_detail(cx)?
            .seasons
            .iter()
            .find(|s| s.number == season)?
            .text("overview")
            .map(str::trim)
            .filter(|o| !o.is_empty())
            .map(|o| o.replace('\n', " "))
    }

    pub(super) fn season_years(&self, season: u32, cx: &App) -> Option<String> {
        let episodes = self.all_episodes(cx);
        year_span(
            episodes
                .iter()
                .filter(|e| e.season == season)
                .filter_map(|e| e.released.as_deref()),
        )
        .or_else(|| {
            self.tmdb_season_summary(season, cx)?
                .year()
                .map(|y| y.to_string())
        })
    }

    /// Wrapping grid of season tiles; no horizontal scrolling. Long series fold
    /// away finished seasons before the next-up one and cap at two rows.
    fn season_selector(
        &mut self,
        season: u32,
        width: Pixels,
        marks: &[watch_progress::EpisodeMark],
        next: Option<(u32, u32)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *theme::palette(cx);
        let fit = fit_grid(width, SEASON_TILE_MIN, SEASON_TILE_MAX, SEASON_TILE_GAP);
        let tiles: Vec<SeasonTile> = tile_order(&self.season_options)
            .into_iter()
            .map(|(number, count)| SeasonTile {
                number,
                count,
                year: self.season_years_cached(number, cx),
                poster: self.season_poster_cached(number, cx),
                tally: watch_progress::season_tally(marks, number, count),
                next_up: next.is_some_and(|(s, _)| s == number),
            })
            .collect();
        let complete: Vec<bool> = tiles.iter().map(SeasonTile::complete).collect();
        let anchor = tiles.iter().position(|t| t.next_up);
        let selected = tiles.iter().position(|t| t.number == season);
        let visible = watch_progress::visible_seasons(
            &complete,
            anchor,
            selected,
            fit.cols,
            self.season_expanded,
        );
        let can_collapse = tiles.len() > fit.cols * watch_progress::COLLAPSED_ROWS;
        let order: Rc<Vec<u32>> = Rc::new(visible.iter().map(|i| tiles[*i].number).collect());
        let hidden = tiles.len() - visible.len();
        let total = tiles.len();
        let mut grid = div()
            .id("season-tiles")
            .role(Role::RadioGroup)
            .aria_label("Seasons")
            .w_full()
            .flex()
            .flex_wrap()
            .gap(SEASON_TILE_GAP)
            .pt(s::S4);
        for (position, index) in visible.iter().enumerate() {
            let tile = &tiles[*index];
            let tile_el = self.season_tile(
                tile,
                tile.number == season,
                position,
                fit,
                order.clone(),
                cx,
            );
            grid = grid.child(tile_el);
        }
        let mut column = div().w_full().flex().flex_col().gap(s::S8).child(grid);
        if can_collapse {
            let label = if self.season_expanded {
                "Show fewer".to_owned()
            } else {
                format!("Show all {total} seasons")
            };
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S12)
                    .child(
                        Button::new("season-expand", label)
                            .variant(ButtonVariant::Ghost)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.season_expanded = !this.season_expanded;
                                cx.notify();
                            })),
                    )
                    .when(hidden > 0 && !self.season_expanded, |el| {
                        el.child(
                            div()
                                .type_style(t::SMALL)
                                .text_color(p.text_muted)
                                .child(format!("Showing {} of {total}", total - hidden)),
                        )
                    }),
            );
        }
        column.into_any_element()
    }

    fn season_tile(
        &mut self,
        tile: &SeasonTile,
        selected: bool,
        position: usize,
        fit: GridFit,
        order: Rc<Vec<u32>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *theme::palette(cx);
        let n = tile.number;
        let focus = self
            .season_focus
            .entry(n)
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        let click_focus = focus.clone();
        let cols = fit.cols;
        let special = n == 0;
        let complete = tile.complete();
        let title = personal::season_label(n);
        let mut sub = format!("{} ep", tile.count);
        if let Some(year) = &tile.year {
            sub.push_str(&format!(" \u{b7} {year}"));
        }
        let progress_label = tile
            .tally
            .map(|t| format!("{} of {} watched", t.watched, t.total));
        let a11y = [
            Some(title.clone()),
            Some(sub.clone()),
            tile.next_up.then(|| "Next up".to_owned()),
            progress_label,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        let poster = Thumb::new(
            self.season_thumb_state(n, tile.poster.as_deref(), cx),
            POSTER_RATIO,
            radius::SM,
        )
        .width(SEASON_POSTER_W);
        // 2 px accent progress line.
        let bar = tile.tally.map(|t| {
            div()
                .mt(s::S6)
                .h(s::S2)
                .w_full()
                .rounded(s::S2)
                .bg(p.line_subtle)
                .overflow_hidden()
                .child(div().h_full().w(relative(t.fraction())).bg(p.primary))
        });
        let hover = p.ctl_hover;
        div()
            .id(("season-tile", n as usize))
            .debug_selector(|| format!("season-tile-{n}"))
            .track_focus(&focus)
            .tab_stop(true)
            .role(Role::RadioButton)
            .aria_label(a11y)
            .aria_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .relative()
            .w(fit.tile)
            .flex_none()
            .flex()
            .items_center()
            .gap(s::S12)
            .py(s::S8)
            .pl(s::S8)
            .pr(s::S12)
            .rounded(radius::LG)
            .border_1()
            .border_color(if selected { p.primary } else { p.line_subtle })
            .bg(if selected {
                p.accent_tint
            } else if special {
                p.bg_canvas.opacity(0.)
            } else {
                p.bg_raised
            })
            .cursor_pointer()
            .hover(move |el| if selected { el } else { el.bg(hover) })
            .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_focus.focus(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.choose_season(n, cx)))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let target = match event.keystroke.key.as_str() {
                    "enter" | "space" => {
                        this.choose_season(n, cx);
                        cx.stop_propagation();
                        return;
                    }
                    "left" => position.checked_sub(1),
                    "right" => Some(position + 1),
                    "up" => position.checked_sub(cols),
                    "down" => Some(position + cols),
                    _ => return,
                };
                cx.stop_propagation();
                if let Some(handle) = target
                    .and_then(|i| order.get(i))
                    .and_then(|n| this.season_focus.get(n))
                {
                    handle.focus(window, cx);
                }
            }))
            .child(poster)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(s::S6)
                            .child(
                                div()
                                    .min_w_0()
                                    .type_style(t::BODY_STRONG)
                                    .text_color(p.text_primary)
                                    .truncate()
                                    .child(title),
                            )
                            .when(complete, |el| el.child(WatchMark::new(WatchMarkState::All))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(s::S4)
                            .type_style(t::SMALL)
                            .child(div().text_color(p.text_muted).truncate().child(sub))
                            .when(tile.next_up, |el| {
                                el.child(
                                    div()
                                        .flex_none()
                                        .text_color(p.accent_text)
                                        .child("\u{b7} Next up"),
                                )
                            }),
                    )
                    .children(bar),
            )
            .into_any_element()
    }

    /// `Season N` with `years · N episodes · K watched`, a clamped overview with
    /// More/Less, the season poster and the season's watched toggle on the right.
    fn season_header(
        &mut self,
        season: u32,
        marks: &[watch_progress::EpisodeMark],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *theme::palette(cx);
        let count = self
            .season_options
            .iter()
            .find(|(n, _)| *n == season)
            .map_or(0, |(_, c)| *c);
        let tally = watch_progress::season_tally(marks, season, count);
        let mut meta = Vec::new();
        meta.extend(self.season_years_cached(season, cx));
        meta.push(format!(
            "{count} {}",
            if count == 1 { "episode" } else { "episodes" }
        ));
        if let Some(t) = tally.filter(|t| t.watched > 0) {
            meta.push(format!("{} watched", t.watched.min(t.total)));
        }
        let overview = self.season_overview(season, cx);
        let ready = self.title_id(cx).is_some();
        let pending = self.season_request_busy()
            || self.episodes(cx).iter().any(|e| {
                matches!(
                    self.episode_view(e.season, e.number, cx).status,
                    ToggleStatus::Known { pending: true, .. }
                )
            });
        let mut toggle = WatchedToggle::new(
            "season-watched",
            tally.unwrap_or(Tally {
                watched: 0,
                total: count as usize,
            }),
        )
        .subject(personal::season_label(season))
        .busy(pending)
        .on_change({
            let weak = cx.entity().downgrade();
            move |watched, _, cx| {
                let _ = weak.update(cx, |d, cx| d.request_season(season, watched, cx));
            }
        });
        if !ready {
            toggle = toggle.unavailable(personal::TRACKING_UNAVAILABLE);
        } else if tally.is_none() {
            toggle = toggle.unavailable("Watched status for this season isn\u{2019}t known yet.");
        }
        let poster = self.season_poster_cached(season, cx);
        let poster = Thumb::new(
            self.season_thumb_state(season, poster.as_deref(), cx),
            POSTER_RATIO,
            radius::MD,
        )
        .width(SEASON_HEADER_POSTER_W);
        let open = self.season_overview_open == Some(season);
        let long = overview.as_deref().is_some_and(overview_needs_toggle);
        let overview_el = overview.map(|text| {
            div()
                .type_style(t::BODY)
                .text_color(p.text_secondary)
                .max_w(px(880.))
                .when(!open, |el| el.clamp_lines(3))
                .child(text)
        });
        div()
            .w_full()
            .flex()
            .items_start()
            .gap(s::S20)
            .child(poster)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(s::S2)
                    .child(
                        div()
                            .type_style(SEASON_TITLE)
                            .text_color(p.text_primary)
                            .child(personal::season_label(season)),
                    )
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .pb(s::S6)
                            .child(meta.join(" \u{b7} ")),
                    )
                    .children(overview_el)
                    .when(long, |el| {
                        el.child(
                            div()
                                .id("season-overview-more")
                                .role(Role::Button)
                                .aria_label(if open {
                                    "Show less of the season overview"
                                } else {
                                    "Show more of the season overview"
                                })
                                .aria_toggled(if open { Toggled::True } else { Toggled::False })
                                .tab_stop(true)
                                .cursor_pointer()
                                .self_start()
                                .pt(s::S4)
                                .type_style(t::SMALL)
                                .text_color(p.accent_text)
                                .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.season_overview_open = (!open).then_some(season);
                                    cx.notify();
                                }))
                                .child(if open { "Less" } else { "More" }),
                        )
                    }),
            )
            .child(div().flex_none().child(toggle))
            .into_any_element()
    }

    /// Explicit pick (tile click/Enter): stays selected across metadata updates.
    fn choose_season(&mut self, n: u32, cx: &mut Context<Self>) {
        self.season_chosen = true;
        self.select_season(n, cx);
    }

    /// Episodes below the details: season tabs and a responsive card grid.
    /// Clicking a card opens the Streams page for that episode.
    pub(super) fn episodes_section(&mut self, width: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let fit = fit_grid(
            width,
            geometry::EPISODE_CARD_MIN_WIDTH,
            geometry::EPISODE_CARD_MAX_WIDTH,
            geometry::EPISODE_GRID_GAP,
        );
        let episodes = self.episodes(cx);
        let (loaded, busy, error) = {
            let c = self.coordinator.read(cx);
            match &self.title {
                BrowseTitle::Tmdb(t) => (
                    c.tmdb_season
                        .value()
                        .is_some_and(|s| s.series_id == t.id && Some(s.season) == self.season),
                    c.tmdb_details.busy() || c.tmdb_season.busy(),
                    c.tmdb_season
                        .error()
                        .or(c.tmdb_details.error())
                        .map(str::to_owned),
                ),
                BrowseTitle::Cinemeta(_) => (
                    c.details.value().is_some(),
                    c.details.busy(),
                    c.details.error().map(str::to_owned),
                ),
                BrowseTitle::Local(_) => {
                    let season_error = self
                        .local_tmdb(cx)
                        .is_some_and(|id| c.tmdb_season.value().is_none_or(|s| s.series_id != id))
                        .then(|| c.tmdb_season.error())
                        .flatten();
                    (
                        c.local_details.value().is_some(),
                        c.local_details.busy()
                            || c.tmdb_season.busy()
                            || c.local_acquisition_busy(),
                        c.local_details
                            .error()
                            .or(season_error)
                            .or(c.local_acquisition_error())
                            .map(str::to_owned),
                    )
                }
            }
        };
        let sel = selection(loaded, episodes.is_empty(), error.is_some(), busy);
        let mut header = Section::new("Episodes");
        // A count is only meaningful once the season is actually known.
        if sel.state == ViewState::Content {
            header = header.count(episodes.len());
        }
        let mut column = div()
            .id("detail-episodes")
            .w_full()
            .py(s::S16)
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(header);
        let snapshot = self.show_snapshot(cx);
        let marks = &snapshot.marks;
        let next = snapshot.next_up;
        if self.season.is_some() && !self.season_options.is_empty() {
            let season = self.season.unwrap_or_default();
            #[cfg(all(test, feature = "ui-probe-test"))]
            let _watch = super::work::stopwatch(super::work::Timed::SeasonSelector);
            column = column.child(self.season_selector(season, width, marks, next, cx));
        }
        if let Some(season) = self.season {
            // 20 px gap, a hairline, 24 px to the header, 20 px to the grid.
            column = column
                .child(
                    div()
                        .w_full()
                        .mt(s::S8)
                        .h(s::HAIRLINE)
                        .bg(theme::palette(cx).line_hairline),
                )
                .child(
                    div()
                        .w_full()
                        .mt(s::S12)
                        .pb(s::S8)
                        .child(self.season_header(season, marks, cx)),
                );
        }
        if self.episode_streams_unavailable(cx) {
            column = column.child(
                Notice::new(NoticeKind::Info, "Streams aren’t available for this title")
                    .body("You can still browse its episodes."),
            );
        }
        let weak = cx.entity().downgrade();
        let mut view = state_view("episodes", sel)
            .skeleton(episode_skeleton(fit))
            .message("Couldn’t load episodes")
            .empty(
                IconName::Tv,
                "No episodes yet",
                Some("Episodes for this season haven’t been announced.".into()),
            )
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_metadata(cx));
            });
        if let Some(error) = &error {
            view = view.hint(error.clone());
        }
        if sel.state == ViewState::Content {
            #[cfg(all(test, feature = "ui-probe-test"))]
            super::work::count(super::work::Section::EpisodeCards, episodes.len());
            let tile = fit.tile;
            #[cfg(all(test, feature = "ui-probe-test"))]
            let _watch = super::work::stopwatch(super::work::Timed::CardLoop);
            let art = self.episode_art_ctx(self.season.unwrap_or_default(), cx);
            let cards: Vec<_> = episodes
                .iter()
                .cloned()
                .map(|ep| self.episode_card(ep, tile, next, &art, cx))
                .collect();
            view = view.content(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap(geometry::EPISODE_GRID_GAP)
                    .children(cards),
            );
        }
        column.child(view).into_any_element()
    }

    fn episode_card(
        &mut self,
        ep: CatalogEpisode,
        tile: Pixels,
        next: Option<(u32, u32)>,
        art: &EpisodeArtCtx,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *theme::palette(cx);
        let focus = self
            .episode_focus
            .entry((ep.season, ep.number))
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        let click_focus = focus.clone();
        let name = episode_name(&ep);
        let marker = episode_marker(&ep);
        let click = ep.clone();
        let enter = ep.clone();
        let find = cx.entity().downgrade();
        let id = ep.number as usize;
        let view = self.episode_view(ep.season, ep.number, cx);
        let watched_now = matches!(view.status, ToggleStatus::Known { on: true, .. });
        let is_next = next == Some((ep.season, ep.number));
        let state = self.episode_thumb_state(art, &ep, cx);
        let mark = self.episode_mark(ep.season, ep.number, cx);
        let writable = matches!(view.status, ToggleStatus::Known { pending: false, .. });
        let write_ref = Some(self.episode_ref(ep.season, ep.number));
        let season = ep.season;
        let key_rest = episode_meta(&ep);
        let overview = episode_overview(&ep);
        let name_ink = if watched_now {
            p.text_secondary
        } else {
            p.text_primary
        };
        let mut still = Thumb::new(state, STILL_RATIO, px(0.))
            .muted(watched_now)
            .overlay(
                div()
                    .absolute()
                    .top(s::S8)
                    .left(s::S8)
                    .flex()
                    .gap(s::S4)
                    .when(is_next, |el| {
                        el.child(OverlayBadge::new("Next up").accent(true))
                    }),
            )
            .overlay(div().absolute().top(s::S8).right(s::S8).child(mark));
        if let Some(bar) = self.episode_progress_bar(ep.season, ep.number, cx) {
            still = still.overlay(bar);
        }
        let resume = self.visible_progress(Some((ep.season, ep.number)), cx);
        let embedded = self.embedded_player(cx);
        let body = div()
            .px(s::S12)
            .pt(s::S8)
            .pb(s::S12)
            .flex()
            .flex_col()
            .gap(s::S2)
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(s::S6)
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text_secondary)
                            .child(format!("E{:02}", ep.number)),
                    )
                    .children(key_rest.map(|rest| div().truncate().child(format!("\u{b7} {rest}"))))
                    .children(resume.map(|point| {
                        div()
                            .id(("episode-resume", id))
                            .flex_none()
                            .ml_auto()
                            .text_color(p.accent_text)
                            .tooltip(tooltip(point.spoken()))
                            .child(format!(
                                "{} {}",
                                if embedded { "Resume" } else { "Saved" },
                                point.timestamp()
                            ))
                    })),
            )
            .child(
                div()
                    .type_style(t::HEADING)
                    .text_color(name_ink)
                    .truncate()
                    .child(name.clone()),
            )
            .child(
                // Reserve two lines so cards in a row keep one height.
                div()
                    .min_h(s::S32)
                    .type_style(t::SMALL)
                    .text_color(p.text_secondary)
                    .clamp_lines(2)
                    .children(overview),
            );
        let progress_items = self.progress_menu(Some((ep.season, ep.number)), cx);
        let ring = BoxShadow::new(px(0.), px(0.), p.focus_ring).spread_radius(s::FOCUS_RING);
        let card = div()
            .id(("episode", id))
            .debug_selector(|| format!("episode-{id}"))
            .track_focus(&focus)
            .tab_stop(true)
            .role(Role::ListItem)
            .aria_label(format!("{marker} {name}"))
            .w(tile)
            .flex_none()
            .rounded(radius::LG)
            .cursor_pointer()
            .focus_visible(move |el| el.shadow(vec![ring.clone()]))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_focus.focus(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.open_episode(click.clone(), cx)))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                // Keys from the watched mark bubble here; only the card itself opens.
                let own_focus = this
                    .episode_focus
                    .get(&(enter.season, enter.number))
                    .is_some_and(|f| f.is_focused(window));
                if own_focus && matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.open_episode(enter.clone(), cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                MediaCard::new(("episode-card", ep.season as usize * 10_000 + id))
                    .accent_edge(is_next)
                    .child(still)
                    .child(body),
            )
            .context_menu(("episode-menu", id), move |_, _| {
                let mut items = vec![
                    menu_item("Find streams", {
                        let find = find.clone();
                        let ep = ep.clone();
                        move |_, cx| {
                            let _ = find.update(cx, |s, cx| s.open_episode(ep.clone(), cx));
                        }
                    }),
                    if watched_now {
                        menu_item("Mark episode unwatched", {
                            let find = find.clone();
                            let write_ref = write_ref.clone();
                            move |_, cx| {
                                if let (true, Some(r)) = (writable, write_ref.clone()) {
                                    let _ = find.update(cx, |s, cx| {
                                        s.perform(personal::Write::Episode(r, false), cx)
                                    });
                                }
                            }
                        })
                    } else {
                        menu_item("Mark episode watched", {
                            let find = find.clone();
                            let write_ref = write_ref.clone();
                            move |_, cx| {
                                if let (true, Some(r)) = (writable, write_ref.clone()) {
                                    let _ = find.update(cx, |s, cx| {
                                        s.perform(personal::Write::Episode(r, true), cx)
                                    });
                                }
                            }
                        })
                    },
                    menu_item("Mark season watched", {
                        let find = find.clone();
                        move |_, cx| {
                            let _ = find.update(cx, |s, cx| s.request_season(season, true, cx));
                        }
                    }),
                    menu_item("Mark season unwatched", {
                        let find = find.clone();
                        move |_, cx| {
                            let _ = find.update(cx, |s, cx| s.request_season(season, false, cx));
                        }
                    }),
                    menu_item("Copy episode title", {
                        let name = name.clone();
                        move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(name.clone()))
                    }),
                    menu_item("Copy episode number", {
                        let marker = marker.clone();
                        move |_, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(marker.clone()))
                        }
                    }),
                ];
                items.splice(1..1, progress_items.clone());
                items
            });
        card.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    fn ep(runtime: Option<u32>, released: Option<&str>, overview: Option<&str>) -> CatalogEpisode {
        CatalogEpisode {
            still_url: None,
            runtime_minutes: runtime,
            season: 1,
            number: 2,
            name: None,
            overview: overview.map(Into::into),
            released: released.map(Into::into),
        }
    }

    #[cfg(feature = "ui-probe-test")]
    #[gpui::test]
    fn stream_notice_uses_imdb_from_loaded_details(cx: &mut gpui::TestAppContext) {
        use crate::tmdb::{TmdbDetails, TmdbTitle};
        use kinema_core::catalog::ImdbId;

        cx.update(theme::init);
        let title = BrowseTitle::Tmdb(TmdbTitle {
            id: 1,
            kind: MediaKind::Series,
            name: "Series".into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        });
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let detail = cx.new(|cx| Detail::new(coordinator.clone(), title.clone(), cx));
        for imdb in [Some("tt1234567"), None] {
            coordinator.update(cx, |c, _| {
                let BrowseTitle::Tmdb(title) = &title else {
                    unreachable!()
                };
                let generation = c.tmdb_details.begin(true);
                c.tmdb_details.fetched(
                    generation,
                    TmdbDetails {
                        title: title.clone(),
                        imdb: imdb.map(|id| {
                            ImdbId::new(id).unwrap_or_else(|_| panic!("invalid fixture IMDb ID"))
                        }),
                        facts: Default::default(),
                        backdrop_url: None,
                        genres: vec![],
                        seasons: vec![],
                    },
                    1,
                );
            });
            detail.read_with(cx, |d, cx| {
                assert!(d.title.imdb().is_none());
                assert_eq!(d.episode_streams_unavailable(cx), imdb.is_none());
            });
        }
    }

    #[test]
    fn season_years_and_tile_order() {
        assert_eq!(
            year_span(["2013-09-01", "2013-12-01"]).as_deref(),
            Some("2013")
        );
        assert_eq!(
            year_span(["2015-01-01T00:00:00Z", "soon", "2013-05-05"]).as_deref(),
            Some("2013\u{2013}2015")
        );
        assert_eq!(year_span(["soon"]), None);
        assert_eq!(
            tile_order(&[(0, 3), (2, 8), (1, 10)]),
            vec![(1, 10), (2, 8), (0, 3)]
        );
    }

    #[test]
    fn dates_read_day_month_year() {
        assert_eq!(display_date("2008-10-14").as_deref(), Some("14 Oct 2008"));
        assert_eq!(
            display_date("2024-03-02T00:00:00Z").as_deref(),
            Some("2 Mar 2024")
        );
        assert_eq!(display_date("2024-13-02"), None);
        assert_eq!(display_date("soon"), None);
    }

    #[test]
    fn long_overviews_get_a_toggle() {
        assert!(!overview_needs_toggle("Short."));
        assert!(overview_needs_toggle(&"word ".repeat(80)));
    }

    #[test]
    fn thumb_state_hides_the_skeleton_until_due() {
        let loading = |fallback: Option<u8>| ArtState::Loading { fallback };
        assert_eq!(
            thumb_state(ArtState::Ready(1), false, "E1"),
            ThumbState::Ready(1)
        );
        assert_eq!(
            thumb_state(ArtState::Fallback(2), false, "E1"),
            ThumbState::Fallback(2)
        );
        assert_eq!(
            thumb_state(loading(Some(3)), true, "E1"),
            ThumbState::Loading { fallback: Some(3) }
        );
        assert_eq!(
            thumb_state(loading(None), true, "E1"),
            ThumbState::Loading { fallback: None }
        );
        assert_eq!(
            thumb_state(loading(Some(3)), false, "E1"),
            ThumbState::Fallback(3)
        );
        assert_eq!(thumb_state(loading(None), false, "E1"), ThumbState::Blank);
        assert_eq!(
            thumb_state(ArtState::<u8>::Missing, true, "E1"),
            ThumbState::Missing("E1".into())
        );
    }

    #[test]
    fn card_text_skips_missing_parts() {
        let full = ep(
            Some(48),
            Some("2024-03-02T00:00:00Z"),
            Some("  A\nsignal. "),
        );
        assert_eq!(episode_meta(&full).as_deref(), Some("48 min · 2 Mar 2024"));
        assert_eq!(episode_overview(&full).as_deref(), Some("A signal."));
        let bare = ep(None, Some("soon"), Some(" "));
        assert_eq!(episode_meta(&bare), None);
        assert_eq!(episode_overview(&bare), None);
        assert_eq!(episode_name(&bare), "Untitled episode");
    }

    #[test]
    fn cards_fill_the_width_within_their_bounds() {
        let fit = |w| {
            fit_grid(
                px(w),
                geometry::EPISODE_CARD_MIN_WIDTH,
                geometry::EPISODE_CARD_MAX_WIDTH,
                geometry::EPISODE_GRID_GAP,
            )
        };
        for (width, cols) in [(300., 1), (920., 3), (1224., 4), (1680., 5), (2400., 7)] {
            let f = fit(width);
            assert_eq!(f.cols, cols, "{width}");
            let used = f.tile * f.cols as f32 + geometry::EPISODE_GRID_GAP * (f.cols as f32 - 1.);
            assert!(
                used <= px(width) && used > px(width - f.cols as f32),
                "{width}"
            );
            if width >= 520. {
                assert!(f.tile >= geometry::EPISODE_CARD_MIN_WIDTH, "{width}");
                assert!(
                    f.tile <= geometry::EPISODE_CARD_MAX_WIDTH + px(1.),
                    "{width}"
                );
            }
        }
    }
}
