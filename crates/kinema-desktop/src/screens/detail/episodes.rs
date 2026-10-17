//! Detail episodes presentation.
use super::*;
impl Detail {
    pub(super) fn episode_column(&mut self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let episodes = self.episodes(cx);
        let mut column = div().w_full().flex().flex_col().gap_0().child(
            div()
                .h(s::S48)
                .px(s::S12)
                .flex_none()
                .flex()
                .items_center()
                .gap(s::S8)
                .child("Episodes")
                .child(div().flex_1())
                .child(
                    div()
                        .type_style(t::SMALL)
                        .text_color(p.text_muted)
                        .child(format!("{} ep", episodes.len())),
                ),
        );
        if let Some(season) = self.season {
            let weak = cx.entity().downgrade();
            let tabs = Segmented::new(
                "season-tabs",
                self.season_options.iter().map(|(n, _)| {
                    (
                        *n,
                        if *n == 0 {
                            "Specials".into()
                        } else {
                            format!("Season {n}").into()
                        },
                    )
                }),
                season,
            )
            .on_change(move |n, _, cx| {
                let _ = weak.update(cx, |s, cx| s.select_season(*n, cx));
            });
            column = column.child(
                div()
                    .id("season-tab-scroll")
                    .w_full()
                    .px(s::S12)
                    .pb(s::S8)
                    .overflow_x_scroll()
                    .child(tabs),
            );
        }
        let c = self.coordinator.read(cx);
        let (busy, error) = if matches!(self.title, BrowseTitle::Tmdb(_)) {
            (
                c.tmdb_details.busy() || c.tmdb_season.busy(),
                c.tmdb_season.error().or(c.tmdb_details.error()),
            )
        } else {
            (c.details.busy(), c.details.error())
        };
        if busy {
            column = column.child(div().p(s::S12).child("Loading episodes…"));
        }
        if let Some(error) = error {
            column = column.child(div().p(s::S12).text_color(p.status_error).child(format!(
                "Episodes could not be loaded: {error}. Refresh to retry."
            )));
        }
        if episodes.is_empty() && !busy && error.is_none() {
            column = column.child(div().p(s::S12).child("No episodes available."));
        }
        for ep in episodes {
            let focus = self
                .episode_focus
                .entry((ep.season, ep.number))
                .or_insert_with(|| cx.focus_handle().tab_stop(true))
                .clone();
            let click_focus = focus.clone();
            let name = ep.name.clone().unwrap_or_else(|| "Untitled episode".into());
            let marker = format!("S{:02}E{:02}", ep.season, ep.number);
            let click = ep.clone();
            let enter = ep.clone();
            let find = cx.entity().downgrade();
            let mut thumbnail = div()
                .w(geometry::EPISODE_ART_WIDTH)
                .h(geometry::EPISODE_ART_HEIGHT)
                .flex_none()
                .bg(p.bg_placeholder)
                .overflow_hidden();
            let c = self.coordinator.read(cx);
            let season_art = if matches!(self.title, BrowseTitle::Tmdb(_)) {
                c.tmdb_season.value().and_then(|s| s.poster_url.clone())
            } else {
                None
            };
            let show_art = match &self.title {
                BrowseTitle::Cinemeta(_) => {
                    c.details.value().and_then(|d| d.background_url.clone())
                }
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
                    thumbnail =
                        thumbnail.child(img(image).size_full().object_fit(ObjectFit::Cover));
                    break;
                }
            }
            let row = div()
                .id(("episode", ep.number as usize))
                .track_focus(&focus)
                .tab_stop(true)
                .h(s::ROW_TWO_LINE)
                .w_full()
                .px(s::S12)
                .py(s::S8)
                .border_b_1()
                .border_color(p.line_hairline)
                .cursor_pointer()
                .hover(move |el| el.bg(p.ctl_hover))
                .focus_visible(move |el| el.border_color(p.focus_ring))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(s::S8)
                        .child(thumbnail)
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(div().type_style(t::BODY_STRONG).text_ellipsis().child(
                                    format!(
                                        "{:02}  {name}{}",
                                        ep.number,
                                        ep.runtime_minutes
                                            .map(|n| format!(" · {n} min"))
                                            .unwrap_or_default()
                                    ),
                                ))
                                .child(
                                    div()
                                        .type_style(t::SMALL)
                                        .text_color(p.text_secondary)
                                        .child(
                                            ep.released
                                                .as_ref()
                                                .map(|r| {
                                                    format!(
                                                "Aired {}",
                                                r.get(..10)
                                                    .filter(|d| {
                                                        kinema_core::catalog_normalized::valid_date(
                                                            d,
                                                        )
                                                    })
                                                    .unwrap_or(r)
                                            )
                                                })
                                                .unwrap_or_else(|| "Air date unavailable".into()),
                                        ),
                                ),
                        ),
                )
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
                .context_menu(("episode-menu", ep.number as usize), move |_, _| {
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
                            move |_, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(name.clone()))
                            }
                        }),
                        menu_item("Copy episode number", {
                            let marker = marker.clone();
                            move |_, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(marker.clone()))
                            }
                        }),
                    ]
                });
            column = column.child(div().flex_none().child(row));
        }
        column
    }
}
