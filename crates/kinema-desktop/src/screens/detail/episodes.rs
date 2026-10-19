//! Detail episodes presentation.
use super::*;
use crate::widgets::{
    notice::{Notice, NoticeKind},
    patterns::{ViewState, focus_ring},
};

/// Padding of the action column body.
const COLUMN_PADDING: Pixels = s::S12;

/// Placeholder shaped like the episode list: 16:9 thumbnail, two text lines.
fn episode_skeleton() -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(s::S8)
        .children((0..geometry::EPISODE_SKELETON_ROWS).map(|_| {
            div()
                .w_full()
                .px(s::S12)
                .py(s::S4)
                .flex()
                .items_center()
                .gap(s::S12)
                .child(
                    Skeleton::block()
                        .w(geometry::EPISODE_ART_WIDTH)
                        .h(geometry::EPISODE_ART_HEIGHT)
                        .rounded(radius::XS),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(s::S6)
                        .child(Skeleton::line().w(relative(0.55)))
                        .child(Skeleton::line().w(relative(0.85))),
                )
                .into_any_element()
        }))
        .into_any_element()
}

/// `E02 · The Signal`
fn episode_title(ep: &CatalogEpisode) -> String {
    let name = ep.name.as_deref().filter(|n| !n.trim().is_empty());
    match name {
        Some(name) => format!("E{:02} · {name}", ep.number),
        None => format!("E{:02} · Untitled episode", ep.number),
    }
}

/// `48 min · 2024-03-02 · A signal reaches the harbor.` — absent parts skipped.
fn episode_subtitle(ep: &CatalogEpisode) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(n) = ep.runtime_minutes {
        parts.push(format!("{n} min"));
    }
    if let Some(date) = ep.released.as_deref().and_then(air_date) {
        parts.push(date.to_owned());
    }
    if let Some(overview) = ep
        .overview
        .as_deref()
        .map(str::trim)
        .filter(|o| !o.is_empty())
    {
        parts.push(overview.replace('\n', " "));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

impl Detail {
    /// Scrollable so any number of seasons stays reachable; large sets use
    /// bare numbers to keep the strip short.
    fn season_selector(&self, season: u32, cx: &mut Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let compact = self.season_options.len() > geometry::FULL_SEASON_LABELS_MAX;
        let weak = cx.entity().downgrade();
        let tabs = Segmented::new(
            "season-tabs",
            self.season_options.iter().map(|(n, _)| {
                let label: SharedString = match (*n, compact) {
                    (0, _) => "Specials".into(),
                    (n, true) => n.to_string().into(),
                    (n, false) => format!("Season {n}").into(),
                };
                (*n, label)
            }),
            season,
        )
        .on_change(move |n, _, cx| {
            let _ = weak.update(cx, |s, cx| s.select_season(*n, cx));
        });
        div()
            .id("season-tab-scroll")
            .w_full()
            .flex()
            .items_center()
            .gap(s::S8)
            .px(s::S4)
            .py(s::S4)
            .overflow_x_scroll()
            .when(compact, |el| {
                el.child(
                    div()
                        .flex_none()
                        .type_style(t::SMALL)
                        .text_color(p.text_muted)
                        .child("Season"),
                )
            })
            .child(div().flex_none().flex().px(s::S4).child(tabs))
            .into_any_element()
    }

    fn episode_thumbnail(&self, ep: &CatalogEpisode, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let mut thumbnail = div()
            .w(geometry::EPISODE_ART_WIDTH)
            .h(geometry::EPISODE_ART_HEIGHT)
            .flex_none()
            .rounded(radius::XS)
            .bg(p.bg_placeholder)
            .overflow_hidden();
        let c = self.coordinator.read(cx);
        let season_art = if matches!(self.title, BrowseTitle::Tmdb(_)) {
            c.tmdb_season.value().and_then(|s| s.poster_url.clone())
        } else {
            None
        };
        let show_art = match &self.title {
            BrowseTitle::Cinemeta(_) => c.details.value().and_then(|d| d.background_url.clone()),
            BrowseTitle::Tmdb(_) => c.tmdb_details.value().and_then(|d| d.backdrop_url.clone()),
        };
        let show = self.display_title(cx);
        let choices = [
            (
                ep.still_url.as_deref(),
                format!("episode:{}:{}", ep.season, ep.number),
            ),
            (season_art.as_deref(), format!("season:{}", ep.season)),
            (
                show_art.as_deref(),
                format!("{}:backdrop", self.title.key()),
            ),
            (show.poster_url(), self.title.key()),
        ];
        for (url, key) in choices {
            if url.is_some()
                && let Some(image) = self.image(url, ArtworkKind::Backdrop, &key, cx)
            {
                thumbnail = thumbnail.child(img(image).size_full().object_fit(ObjectFit::Cover));
                break;
            }
        }
        thumbnail
    }

    pub(super) fn episode_column(&mut self, cx: &mut Context<Self>) -> Div {
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
            }
        };
        let sel = selection(loaded, episodes.is_empty(), error.is_some(), busy);
        let mut section = Section::new("Episodes");
        // A count is only meaningful once the season is actually known.
        if sel.state == ViewState::Content {
            section = section.count(episodes.len());
        }
        let mut column = div()
            .w_full()
            .p(COLUMN_PADDING)
            .flex()
            .flex_col()
            .gap(s::S8)
            .child(section);
        if self.season.is_some() && !self.season_options.is_empty() {
            let season = self.season.unwrap_or_default();
            column = column.child(self.season_selector(season, cx));
        }
        if self.title.imdb().is_none() && !self.meta_status(cx).busy {
            column = column.child(
                Notice::new(NoticeKind::Info, "Streams aren’t available for this title")
                    .body("You can still browse its episodes."),
            );
        }
        let weak = cx.entity().downgrade();
        let mut view = state_view("episodes", sel)
            .skeleton(episode_skeleton())
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
            let rows: Vec<_> = episodes
                .into_iter()
                .map(|ep| self.episode_row(ep, cx))
                .collect();
            view = view.content(div().w_full().flex().flex_col().gap(s::S4).children(rows));
        }
        column.child(view)
    }

    fn episode_row(&mut self, ep: CatalogEpisode, cx: &mut Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let focus = self
            .episode_focus
            .entry((ep.season, ep.number))
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        let click_focus = focus.clone();
        let name = ep.name.clone().unwrap_or_else(|| "Untitled episode".into());
        let marker = episode_marker(&ep);
        let click = ep.clone();
        let enter = ep.clone();
        let find = cx.entity().downgrade();
        let id = ep.number as usize;
        let mut list_row = ListRow::new(("episode-row", id), episode_title(&ep))
            .leading(self.episode_thumbnail(&ep, cx));
        if let Some(subtitle) = episode_subtitle(&ep) {
            list_row = list_row.subtitle(subtitle);
        }
        let row = div()
            .id(("episode", id))
            .track_focus(&focus)
            .tab_stop(true)
            .w_full()
            .rounded(radius::SM)
            .cursor_pointer()
            .hover(move |el| el.bg(p.ctl_hover))
            .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_focus.focus(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.choose_episode(click.clone(), cx)))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" {
                    this.choose_episode(enter.clone(), cx);
                    cx.stop_propagation();
                }
            }))
            .child(list_row)
            .context_menu(("episode-menu", id), move |_, _| {
                vec![
                    menu_item("Find streams", {
                        let find = find.clone();
                        let ep = ep.clone();
                        move |_, cx| {
                            let _ = find.update(cx, |s, cx| s.choose_episode(ep.clone(), cx));
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
                ]
            });
        div().flex_none().child(row).into_any_element()
    }
}
