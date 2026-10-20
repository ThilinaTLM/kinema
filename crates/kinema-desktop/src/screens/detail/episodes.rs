//! Detail episodes presentation.
use super::*;
use crate::widgets::{
    notice::{Notice, NoticeKind},
    patterns::{ViewState, focus_ring},
};

/// Placeholder shaped like the episode grid: 16:9 still and two text lines.
fn episode_skeleton(width: Pixels, cols: usize) -> AnyElement {
    let tile = tile_width(width, cols);
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
                        .rounded(radius::SM),
                )
                .child(Skeleton::line().w(relative(0.6)))
                .child(Skeleton::line().w(relative(0.9)))
                .into_any_element()
        }))
        .into_any_element()
}

/// Width of one card when `cols` cards share `width` with the grid gap.
fn tile_width(width: Pixels, cols: usize) -> Pixels {
    let cols = cols.max(1) as f32;
    ((width - geometry::EPISODE_GRID_GAP * (cols - 1.)) / cols).floor()
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

/// `48 min · 2024-03-02` — absent parts skipped.
fn episode_meta(ep: &CatalogEpisode) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(n) = ep.runtime_minutes {
        parts.push(format!("{n} min"));
    }
    if let Some(date) = ep.released.as_deref().and_then(air_date) {
        parts.push(date.to_owned());
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn episode_overview(ep: &CatalogEpisode) -> Option<String> {
    ep.overview
        .as_deref()
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(|o| o.replace('\n', " "))
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

    fn episode_thumbnail(&self, ep: &CatalogEpisode, width: Pixels, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let mut thumbnail = div()
            .relative()
            .w(width)
            .h(width * 9. / 16.)
            .flex_none()
            .rounded(radius::SM)
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
        thumbnail.child(
            div()
                .absolute()
                .left(s::S6)
                .bottom(s::S6)
                .px(s::S6)
                .rounded(radius::XS)
                .bg(p.backdrop_scrim.opacity(0.8))
                .type_style(t::CAPTION)
                .text_color(p.text_primary)
                .child(format!("E{:02}", ep.number)),
        )
    }

    /// Episodes below the details: season tabs and a responsive card grid.
    /// Clicking a card opens the Streams page for that episode.
    pub(super) fn episodes_section(&mut self, width: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let cols = grid_columns(
            width,
            geometry::EPISODE_CARD_MIN_WIDTH,
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
            .skeleton(episode_skeleton(width, cols))
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
            let tile = tile_width(width, cols);
            let cards: Vec<_> = episodes
                .into_iter()
                .map(|ep| self.episode_card(ep, tile, cx))
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
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
        let thumbnail = self.episode_thumbnail(&ep, tile, cx);
        let card = div()
            .id(("episode", id))
            .debug_selector(|| format!("episode-{id}"))
            .track_focus(&focus)
            .tab_stop(true)
            .role(Role::ListItem)
            .aria_label(format!("{marker} {name}"))
            .w(tile)
            .flex_none()
            .flex()
            .flex_col()
            .gap(s::S6)
            .pb(s::S8)
            .rounded(radius::SM)
            .cursor_pointer()
            .hover(move |el| el.bg(p.ctl_hover))
            .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_focus.focus(window, cx)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.open_episode(click.clone(), cx)))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.open_episode(enter.clone(), cx);
                    cx.stop_propagation();
                }
            }))
            .child(thumbnail)
            .child(
                div()
                    .px(s::S4)
                    .flex()
                    .flex_col()
                    .gap(s::S2)
                    .child(
                        div()
                            .type_style(t::BODY_STRONG)
                            .text_color(p.text_primary)
                            .truncate()
                            .child(name.clone()),
                    )
                    .children(episode_meta(&ep).map(|meta| {
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(meta)
                    }))
                    .children(episode_overview(&ep).map(|text| {
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .line_clamp(2)
                            .child(text)
                    })),
            )
            .context_menu(("episode-menu", id), move |_, _| {
                vec![
                    menu_item("Find streams", {
                        let find = find.clone();
                        let ep = ep.clone();
                        move |_, cx| {
                            let _ = find.update(cx, |s, cx| s.open_episode(ep.clone(), cx));
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

    #[test]
    fn card_text_skips_missing_parts() {
        let full = ep(
            Some(48),
            Some("2024-03-02T00:00:00Z"),
            Some("  A\nsignal. "),
        );
        assert_eq!(episode_meta(&full).as_deref(), Some("48 min · 2024-03-02"));
        assert_eq!(episode_overview(&full).as_deref(), Some("A signal."));
        let bare = ep(None, Some("soon"), Some(" "));
        assert_eq!(episode_meta(&bare), None);
        assert_eq!(episode_overview(&bare), None);
        assert_eq!(episode_name(&bare), "Untitled episode");
    }

    #[test]
    fn cards_share_the_width_exactly() {
        let w = px(1000.);
        assert_eq!(
            grid_columns(
                w,
                geometry::EPISODE_CARD_MIN_WIDTH,
                geometry::EPISODE_GRID_GAP
            ),
            3
        );
        assert_eq!(
            grid_columns(
                px(200.),
                geometry::EPISODE_CARD_MIN_WIDTH,
                geometry::EPISODE_GRID_GAP
            ),
            1
        );
        let tile = tile_width(w, 3);
        assert!(tile * 3. + geometry::EPISODE_GRID_GAP * 2. <= w);
    }
}
