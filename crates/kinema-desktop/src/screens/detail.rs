//! Production title detail. Route-local selection; shared coordinator owns I/O.
use super::{
    coordinator::{AppCoordinator, BrowseTitle},
    play_sheet::PlaySheet,
};
use crate::{
    artwork::{self, ArtworkKey, ArtworkKind},
    widgets::{
        button::Button,
        menu::{ContextMenuExt, Select, SelectEvent},
        motion::{self, Motion},
        segmented::Segmented,
        text::TypeExt,
        tokens::{self, space as s, typeface as t},
    },
};
use gpui::{prelude::*, *};
use kinema_core::{
    MediaKind,
    catalog::CatalogEpisode,
    sources::{IndexerKind, PlaybackTarget},
};
use std::{cell::Cell, rc::Rc};

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
fn source_quality(c: &kinema_core::sources::SourceCandidate) -> Quality {
    quality(&format!(
        "{} {}",
        c.quality.as_deref().unwrap_or(""),
        c.name
    ))
}
fn release_hints(candidate: &kinema_core::sources::SourceCandidate) -> Vec<String> {
    let text = format!(
        "{} {}",
        candidate.name,
        candidate.quality.as_deref().unwrap_or("")
    )
    .to_ascii_lowercase();
    let tokens: Vec<_> = text.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let mut hints = Vec::new();
    for token in [
        "2160p", "1080p", "720p", "hdr", "dv", "hevc", "x265", "x264", "av1",
    ] {
        if tokens.contains(&token) {
            hints.push(token.to_owned());
        }
    }
    if let Some(size) = candidate.size {
        hints.push(format!("{:.1} GiB", size as f64 / 1_073_741_824.));
    }
    if let Some(seeds) = candidate.seeders {
        hints.push(format!("{seeds} seeds"));
    }
    hints
}
fn default_season(episodes: &[CatalogEpisode]) -> Option<u32> {
    episodes
        .iter()
        .map(|e| e.season)
        .filter(|s| *s != 0)
        .min()
        .or_else(|| episodes.iter().map(|e| e.season).min())
}
pub(crate) struct Detail {
    coordinator: Entity<AppCoordinator>,
    title: BrowseTitle,
    indexer: Entity<Select<IndexerKind>>,
    season_select: Option<Entity<Select<u32>>>,
    season: Option<u32>,
    episode: Option<CatalogEpisode>,
    target: Option<PlaybackTarget>,
    filter: Quality,
    expanded: bool,
    more: bool,
    selected: Option<usize>,
    stream_focus: Vec<FocusHandle>,
    episode_focus: std::collections::HashMap<(u32, u32), FocusHandle>,
    inline: Option<usize>,
    play: Option<Entity<PlaySheet>>,
    play_subscription: Option<Subscription>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    art_generation: u64,
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
                this.resolve_mapping(cx);
                cx.notify();
            }),
            cx.subscribe(&indexer, |this, _, event: &SelectEvent<IndexerKind>, cx| {
                let SelectEvent::Changed(indexer) = event;
                this.filter = Quality::All;
                this.selected = None;
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
        let art_generation = artwork::begin_generation("v3-detail", cx);
        Self {
            coordinator,
            title,
            indexer,
            season_select: None,
            season: None,
            episode: None,
            target,
            filter: Quality::All,
            expanded: false,
            more: false,
            selected: None,
            stream_focus: Vec::new(),
            episode_focus: Default::default(),
            inline: None,
            play: None,
            play_subscription: None,
            bounds: Rc::new(Cell::new(None)),
            art_generation,
            _subscriptions: subscriptions,
        }
    }
    pub(crate) fn name(&self) -> &str {
        self.title.name()
    }
    fn resolve_mapping(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.title, BrowseTitle::Tmdb(_)) {
            return;
        }
        let details = self.coordinator.read(cx).tmdb_details.value().cloned();
        if let Some(details) = details {
            let mut title = details.title;
            title.imdb = details.imdb;
            self.title = BrowseTitle::Tmdb(title);
            if self.target.is_none()
                && self.title.kind() == MediaKind::Movie
                && let Some(id) = self.title.imdb().cloned()
            {
                let target = PlaybackTarget::Movie(id);
                self.target = Some(target.clone());
                let indexer = *self.indexer.read(cx).value();
                self.coordinator
                    .update(cx, |c, cx| c.sources(indexer, target, cx));
            }
        }
    }
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.art_generation = artwork::begin_generation("v3-detail", cx);
        self.coordinator
            .update(cx, |c, cx| c.title_details(&self.title, true, cx));
        if let Some(target) = self.target.clone() {
            let indexer = *self.indexer.read(cx).value();
            self.coordinator
                .update(cx, |c, cx| c.sources(indexer, target, cx));
        }
    }
    /// Stable `detail-hero-poster` slot, measured after layout for P3 flight.
    pub(crate) fn hero_slot_bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds.get()
    }
    fn choose_episode(&mut self, episode: CatalogEpisode, cx: &mut Context<Self>) {
        if let Some(series) = self.title.imdb().cloned() {
            let target = PlaybackTarget::Episode {
                series,
                season: episode.season,
                episode: episode.number,
            };
            self.target = Some(target.clone());
            self.episode = Some(episode);
            self.filter = Quality::All;
            let indexer = *self.indexer.read(cx).value();
            self.coordinator
                .update(cx, |c, cx| c.sources(indexer, target, cx));
            cx.notify();
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
            let title = self.title.name().to_owned();
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
    #[cfg(feature = "ui-probe")]
    pub(crate) fn fixture(
        coordinator: Entity<AppCoordinator>,
        title: BrowseTitle,
        streams: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::new(coordinator.clone(), title.clone(), cx);
        let episodes = (1..=8)
            .map(|n| CatalogEpisode {
                season: 1,
                number: n,
                name: Some(if n == 2 {
                    "The Signal".into()
                } else {
                    format!("Arrival {n}")
                }),
                overview: None,
                released: Some("2024-03-02".into()),
            })
            .collect::<Vec<_>>();
        if streams && title.kind() == MediaKind::Series {
            this.episode = Some(episodes[1].clone());
            this.target = title.imdb().cloned().map(|series| PlaybackTarget::Episode {
                series,
                season: 1,
                episode: 2,
            });
        }
        coordinator.update(cx, |c, cx| {
            use crate::{app_state::Stage, metadata_bridge::SourceSet};
            use kinema_core::sources::{InfoHash, SourceCandidate, TorrentSource};
            if let BrowseTitle::Cinemeta(title) = title {
                let generation = c.details.begin(true);
                c.details.apply(
                    generation,
                    Stage::Fetched {
                        value: kinema_core::catalog::TitleDetails {
                            title,
                            poster_url: None,
                            background_url: None,
                            release_info: Some("2024".into()),
                            genres: vec!["Drama".into()],
                            episodes,
                        },
                        fetched_at: 1,
                    },
                );
            }
            let generation = c.sources.begin(true);
            c.sources.apply(
                generation,
                Stage::Fetched {
                    value: SourceSet {
                        omitted: 0,
                        candidates: ["2160p.HDR", "1080p.WEB", "720p"]
                            .into_iter()
                            .map(|q| SourceCandidate {
                                name: format!("Quiet.Orbit.{q}.mkv"),
                                quality: Some(q.into()),
                                size: Some(4_200_000_000),
                                seeders: Some(412),
                                torrent: TorrentSource {
                                    hash: InfoHash::new("a".repeat(40))
                                        .unwrap_or_else(|e| panic!("fixture hash: {e}")),
                                    file_index: None,
                                    filename: None,
                                },
                            })
                            .collect(),
                    },
                    fetched_at: 1,
                },
            );
            cx.notify();
        });
        this
    }
}
impl Render for Detail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let c = self.coordinator.read(cx);
        let details = if matches!(self.title, BrowseTitle::Cinemeta(_)) {
            c.details.value().cloned()
        } else {
            None
        };
        let mut title = self.title.clone();
        if let Some(d) = &details {
            title = BrowseTitle::Cinemeta(d.title.clone());
        }
        if matches!(self.title, BrowseTitle::Tmdb(_))
            && let Some(d) = c.tmdb_details.value()
        {
            let mut mapped = d.title.clone();
            mapped.imdb = d.imdb.clone();
            title = BrowseTitle::Tmdb(mapped);
        }
        let sources = c.sources.value().cloned();
        let busy = c.sources.busy();
        let error = c.sources.error().map(str::to_owned);
        let (origin, fetched_at) = if matches!(self.title, BrowseTitle::Tmdb(_)) {
            (c.tmdb_details.origin(), c.tmdb_details.fetched_at())
        } else {
            (c.details.origin(), c.details.fetched_at())
        };
        let (detail_error, metadata_busy) = if matches!(self.title, BrowseTitle::Tmdb(_)) {
            (
                c.tmdb_details.error().map(str::to_owned),
                c.tmdb_details.busy(),
            )
        } else {
            (c.details.error().map(str::to_owned), c.details.busy())
        };
        let episodes = details
            .as_ref()
            .map(|d| d.episodes.clone())
            .unwrap_or_default();
        if self.season_select.is_none() && !episodes.is_empty() {
            self.season = default_season(&episodes);
            let mut seasons: Vec<_> = episodes.iter().map(|e| e.season).collect();
            seasons.sort_unstable();
            seasons.dedup();
            let select = cx.new(|cx| {
                Select::new(
                    self.season.unwrap_or(0),
                    seasons
                        .into_iter()
                        .map(|n| (n, format!("Season {n}").into()))
                        .collect(),
                    cx,
                )
            });
            self._subscriptions.push(cx.subscribe(
                &select,
                |this, _, event: &SelectEvent<u32>, cx| {
                    let SelectEvent::Changed(n) = event;
                    this.season = Some(*n);
                    this.episode = None;
                    this.target = None;
                    this.coordinator
                        .update(cx, |c, cx| c.cancel_title_requests(cx));
                    cx.notify();
                },
            ));
            self.season_select = Some(select);
        }
        let mut poster = div()
            .id("detail-hero-poster")
            .w(px(220.))
            .h(px(330.))
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .child(title.name().to_owned());
        if let Some(url) = title
            .poster_url()
            .and_then(|u| ArtworkKey::new(ArtworkKind::Poster, u))
        {
            artwork::request("v3-detail", self.art_generation, url.clone(), cx);
            if let Some(image) = artwork::get(&url, cx) {
                poster = poster.child(
                    img(image)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                );
            }
        }
        #[cfg(feature = "ui-probe")]
        if let Some(image) = self.coordinator.read(cx).fixture_art.get(&self.title.key()) {
            poster = poster.child(
                img(image.clone())
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        let bounds = self.bounds.clone();
        poster = poster.child(
            canvas(
                move |b, _, _| {
                    bounds.set(Some(b));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        let mut copy = div()
            .min_h(px(330.))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(s::S16)
            .child(div().type_style(t::DISPLAY).child(title.name().to_owned()))
            .child(
                div()
                    .type_style(t::SMALL)
                    .text_color(tokens::Palette::dark().text_secondary)
                    .child(format!(
                        "{} · {} · {}",
                        title.year().map(|y| y.to_string()).unwrap_or_default(),
                        if title.kind() == MediaKind::Series {
                            title
                                .seasons()
                                .map(|n| format!("{n} seasons"))
                                .unwrap_or_else(|| "Series".into())
                        } else {
                            title
                                .runtime()
                                .map(|m| format!("{m} min"))
                                .unwrap_or_default()
                        },
                        title.genres().join(" · ")
                    )),
            );
        if let Some(rating) = title.rating() {
            copy = copy.child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S6)
                    .child(crate::widgets::chip::Chip::rating(Some(rating)).over_art(true))
                    .child("IMDb"),
            );
        }
        let synopsis = title.overview().unwrap_or("No synopsis available.");
        copy = copy
            .child(
                div()
                    .flex_none()
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .when(!self.more, |el| el.h(px(84.)).overflow_hidden())
                    .child(synopsis.to_owned()),
            )
            .child(
                Button::new("synopsis-more", if self.more { "Less" } else { "More" }).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.more = !this.more;
                        cx.notify();
                    }),
                ),
            )
            .child(
                Button::new("detail-provenance", "Details & provenance").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.expanded = !this.expanded;
                        cx.notify();
                    },
                )),
            )
            .when(self.expanded, |el| {
                el.child(format!(
                    "IMDb {} · provider {} · IMDb ratings only · {origin:?} · fetched at {}",
                    title.imdb().map(|id| id.as_str()).unwrap_or("unmapped"),
                    if matches!(self.title, BrowseTitle::Tmdb(_)) {
                        "TMDB"
                    } else {
                        "Cinemeta"
                    },
                    fetched_at
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "unknown".into())
                ))
            });
        let mut hero = div().relative().flex_1().min_w_0().overflow_hidden();
        if let Some(key) = details
            .as_ref()
            .and_then(|d| d.background_url.as_deref())
            .and_then(|u| ArtworkKey::new(ArtworkKind::Backdrop, u))
        {
            artwork::request("v3-detail", self.art_generation, key.clone(), cx);
            if let Some(image) = artwork::get(&key, cx) {
                hero = hero.child(
                    img(image)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                );
            }
        }
        #[cfg(feature = "ui-probe")]
        if let Some(image) = self
            .coordinator
            .read(cx)
            .fixture_art
            .get(&format!("{}:backdrop", self.title.key()))
        {
            hero = hero.child(
                img(image.clone())
                    .absolute()
                    .inset_0()
                    .w_full()
                    .h(px(480.))
                    .object_fit(ObjectFit::Cover),
            );
        }
        let dark = tokens::Palette::dark();
        hero = hero
            .child(div().absolute().inset_0().bg(linear_gradient(
                90.,
                linear_color_stop(dark.scrim_max, 0.),
                linear_color_stop(dark.scrim_max.opacity(0.), 1.),
            )))
            .child(div().absolute().inset_0().bg(linear_gradient(
                180.,
                linear_color_stop(p.bg_canvas.opacity(0.), 0.),
                linear_color_stop(p.bg_canvas, 1.),
            )))
            .child(
                div()
                    .relative()
                    .p(s::S32)
                    .flex()
                    .items_start()
                    .gap(s::S24)
                    .child(poster)
                    .child(
                        copy.bg(dark.scrim_max)
                            .p(s::S8)
                            .text_color(dark.text_primary),
                    ),
            );
        if let Some(error) = detail_error {
            hero = hero.child(div().text_color(p.status_error).child(error));
        }
        let mut column = div()
            .flex_none()
            .w(if window.viewport_size().width < px(1100.) {
                px(340.)
            } else {
                px(400.)
            })
            .h_full()
            .bg(p.bg_surface)
            .p(s::S16)
            .flex()
            .flex_col()
            .gap(s::S12)
            .id("detail-actions")
            .overflow_y_scroll();
        if title.imdb().is_none() {
            column = column.child(if metadata_busy {
                "Looking up IMDb mapping…"
            } else {
                "Streams unavailable — no IMDb ID for this title."
            });
        } else if title.kind() == MediaKind::Series && self.episode.is_none() {
            column = column
                .child("Season")
                .when_some(self.season_select.clone(), |el, s| el.child(s));
            if episodes.is_empty() {
                column = column.child(if matches!(self.title,BrowseTitle::Tmdb(_)) {"Episode browsing unavailable: TMDB season requests are not exposed by the coordinator."} else if self.coordinator.read(cx).details.busy() {"Loading episodes…"} else if self.coordinator.read(cx).details.error().is_some() {"Episodes could not be loaded. Refresh to retry."} else {"No episodes available."});
            }
            for episode in episodes.iter().filter(|e| Some(e.season) == self.season) {
                let ep = episode.clone();
                let key = (ep.season, ep.number);
                let focus = self
                    .episode_focus
                    .entry(key)
                    .or_insert_with(|| cx.focus_handle().tab_stop(true))
                    .clone();
                let enter = ep.clone();
                let click = ep.clone();
                let find = cx.entity().downgrade();
                let name = ep.name.clone().unwrap_or_else(|| "Untitled episode".into());
                let marker = format!("S{:02}E{:02}", ep.season, ep.number);
                column = column.child(
                    div().flex_none().child(
                        div()
                            .id(("episode", ep.number as usize))
                            .track_focus(&focus)
                            .tab_stop(true)
                            .focus_visible(move |el| el.border_color(p.focus_ring))
                            .h(px(56.))
                            .w_full()
                            .px(s::S8)
                            .py(s::S6)
                            .border_b_1()
                            .border_color(p.line_hairline)
                            .hover(|el| el.bg(p.ctl_hover))
                            .cursor_pointer()
                            .child(div().child(format!("{:02}  {}", ep.number, name)))
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_secondary)
                                    .child(format!(
                                        "Aired {}",
                                        ep.released.as_deref().unwrap_or("—")
                                    )),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.choose_episode(click.clone(), cx)
                            }))
                            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                if event.keystroke.key == "enter" {
                                    this.choose_episode(enter.clone(), cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .context_menu(("episode-context", ep.number as usize), move |_, _| {
                                vec![
                                    super::browse::menu_item("Find streams", {
                                        let find = find.clone();
                                        let ep = ep.clone();
                                        move |_, cx| {
                                            let _ = find.update(cx, |s, cx| {
                                                s.choose_episode(ep.clone(), cx)
                                            });
                                        }
                                    }),
                                    super::browse::menu_item("Copy episode title", {
                                        let name = name.clone();
                                        move |_, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                name.clone(),
                                            ))
                                        }
                                    }),
                                    super::browse::menu_item("Copy episode number", {
                                        let marker = marker.clone();
                                        move |_, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                marker.clone(),
                                            ))
                                        }
                                    }),
                                ]
                            }),
                    ),
                );
            }
        } else {
            if let Some(ep) = &self.episode {
                column = column.child(
                    Button::new(
                        "episodes-back",
                        format!(
                            "← S{:02}E{:02} · {}",
                            ep.season,
                            ep.number,
                            ep.name.as_deref().unwrap_or("Episode")
                        ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.episode = None;
                        this.target = None;
                        this.coordinator
                            .update(cx, |c, cx| c.cancel_title_requests(cx));
                        cx.notify();
                    })),
                );
            }
            column = column.child(
                div()
                    .flex()
                    .justify_between()
                    .child("Streams")
                    .child(self.indexer.clone()),
            );
            if busy {
                column = column.child("Loading streams…");
            }
            if let Some(error) = error {
                column = column.child(div().text_color(p.status_error).child(error));
            }
            if let Some(sources) = sources {
                let mut filters = vec![(Quality::All, "All".into())];
                for (q, label) in [
                    (Quality::Uhd, "4K"),
                    (Quality::Hd, "1080p"),
                    (Quality::Sd, "720p"),
                    (Quality::Other, "Other"),
                ] {
                    if sources.candidates.iter().any(|c| source_quality(c) == q) {
                        filters.push((q, label.into()));
                    }
                }
                let weak = cx.entity().downgrade();
                column = column.child(Segmented::new("quality", filters, self.filter).on_change(
                    move |q, _, cx| {
                        let _ = weak.update(cx, |s, cx| {
                            s.filter = *q;
                            cx.notify();
                        });
                    },
                ));
                if sources.candidates.is_empty() {
                    column = column.child("No streams found for this indexer.");
                }
                while self.stream_focus.len() < sources.candidates.len() {
                    self.stream_focus.push(cx.focus_handle().tab_stop(true));
                }
                for (i, candidate) in sources.candidates.iter().enumerate().filter(|(_, c)| {
                    self.filter == Quality::All || source_quality(c) == self.filter
                }) {
                    let mut hints = div().flex().items_center().gap(s::S6);
                    for hint in release_hints(candidate) {
                        hints = hints.child(crate::widgets::chip::Chip::quality(hint));
                    }
                    let play = cx.entity().downgrade();
                    let info = play.clone();
                    let name = candidate.name.clone();
                    let row = div()
                        .id(("stream", i))
                        .track_focus(&self.stream_focus[i])
                        .tooltip(crate::widgets::tooltip::tooltip(candidate.name.clone()))
                        .tab_stop(true)
                        .focus_visible(move |el| el.border_color(p.focus_ring))
                        .w_full()
                        .min_h(px(56.))
                        .border_l_2()
                        .border_color(if self.selected == Some(i) {
                            p.accent
                        } else {
                            p.line_hairline
                        })
                        .px(s::S8)
                        .py(s::S6)
                        .cursor_pointer()
                        .hover(|el| el.bg(p.ctl_hover))
                        .when(self.selected == Some(i), |el| el.bg(p.accent_tint))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_stream(i, cx)))
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                            if event.keystroke.key == "enter" {
                                this.open_stream(i, cx);
                                cx.stop_propagation();
                            }
                        }))
                        .child(
                            div()
                                .type_style(t::MONO)
                                .text_ellipsis()
                                .child(candidate.name.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(hints)
                                .child(Button::new(("stream-info", i), "▸").on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.inline = if this.inline == Some(i) {
                                            None
                                        } else {
                                            Some(i)
                                        };
                                        cx.stop_propagation();
                                        cx.notify();
                                    },
                                ))),
                        )
                        .when(self.inline == Some(i), |el| {
                            el.child(div().type_style(t::MONO).child(format!(
                                "{} · source {}",
                                candidate.name,
                                candidate.torrent.hash.as_str()
                            )))
                        })
                        .context_menu(("stream-menu", i), move |_, _| {
                            vec![
                                super::browse::menu_item("Play…", {
                                    let play = play.clone();
                                    move |_, cx| {
                                        let _ = play.update(cx, |s, cx| s.open_stream(i, cx));
                                    }
                                }),
                                super::browse::menu_item("Show details", {
                                    let info = info.clone();
                                    move |_, cx| {
                                        let _ = info.update(cx, |s, cx| {
                                            s.inline = Some(i);
                                            cx.notify();
                                        });
                                    }
                                }),
                                super::browse::menu_item("Copy release name", {
                                    let name = name.clone();
                                    move |_, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            name.clone(),
                                        ))
                                    }
                                }),
                            ]
                        });
                    column = column.child(div().flex_none().child(row));
                }
            }
        }
        let narrow = window.viewport_size().width < px(960.);
        if narrow {
            hero = hero.min_h(px(420.)).flex_none();
            column = column.w_full().h(px(420.));
        }
        let column_duration = motion::duration(Motion::Column, cx);
        let column = if let Some(ep) = &self.episode
            && !column_duration.is_zero()
        {
            column
                .relative()
                .with_animation(
                    ("episode-streams", ep.number as usize),
                    Animation::new(column_duration).with_easing(motion::ease_out_cubic),
                    |el, t| el.left(px(24.) * (1. - t)).opacity(t),
                )
                .into_any_element()
        } else {
            column.into_any_element()
        };
        let content = div()
            .id("detail-content")
            .size_full()
            .relative()
            .flex()
            .when(narrow, |el| el.flex_col().overflow_y_scroll())
            .child(hero)
            .child(column);
        let duration = motion::duration(Motion::Content, cx);
        let content = if duration.is_zero() {
            content.into_any_element()
        } else {
            content
                .with_animation(
                    "detail-open",
                    Animation::new(duration).with_easing(motion::ease_out_cubic),
                    |el, t| el.opacity(t).top(px(8.) * (1. - t)),
                )
                .into_any_element()
        };
        let _ = self.hero_slot_bounds();
        div()
            .relative()
            .size_full()
            .child(content)
            .when_some(self.play.clone(), |el, play| el.child(play))
    }
}
#[cfg(test)]
mod tests {
    use super::{Quality, default_season, quality};
    use kinema_core::catalog::CatalogEpisode;
    #[test]
    fn quality_buckets_are_explicit() {
        assert_eq!(quality("Movie.2160p.HDR"), Quality::Uhd);
        assert_eq!(quality("1080p"), Quality::Hd);
        assert_eq!(quality("720p"), Quality::Sd);
        assert_eq!(quality("year2024"), Quality::Other);
    }
    #[test]
    fn switching_indexer_rejects_stale_sources() {
        use crate::{
            app_state::{Resource, Stage},
            metadata_bridge::SourceSet,
        };
        let mut sources = Resource::<SourceSet>::default();
        let old = sources.begin(true);
        let current = sources.begin(true);
        let result = || Stage::Fetched {
            value: SourceSet {
                candidates: Vec::new(),
                omitted: 0,
            },
            fetched_at: 1,
        };
        assert!(!sources.apply(old, result()));
        assert!(sources.value().is_none());
        assert!(sources.apply(current, result()));
        assert!(sources.value().is_some());
    }
    #[test]
    fn seasons_skip_specials() {
        let episode = |season| CatalogEpisode {
            season,
            number: 1,
            name: None,
            overview: None,
            released: None,
        };
        assert_eq!(
            default_season(&[episode(0), episode(3), episode(1)]),
            Some(1)
        );
        assert_eq!(default_season(&[episode(0)]), Some(0));
        assert_eq!(default_season(&[]), None);
    }
}
