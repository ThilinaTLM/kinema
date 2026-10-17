//! Detail hero presentation.
use super::*;
impl Detail {
    /// Art-bearing disclosures deliberately use the dark artwork palette in both themes.
    fn disclosure(
        &self,
        id: &'static str,
        label: &'static str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let dark = tokens::Palette::dark();
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .h(s::CONTROL_HEIGHT)
            .px(s::S6)
            .gap(s::S4)
            .flex()
            .items_center()
            .type_style(t::SMALL)
            .text_color(dark.text_secondary)
            .cursor_pointer()
            .border(s::FOCUS_RING)
            .border_color(dark.bg_canvas.opacity(0.))
            .hover(move |el| el.bg(dark.ctl_hover))
            .focus_visible(move |el| el.border_color(dark.focus_ring))
            .child(
                Icon::new(if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .color(dark.text_secondary),
            )
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                if id == "synopsis-more" {
                    this.more = !this.more;
                } else {
                    this.expanded = !this.expanded;
                }
                cx.notify();
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    if id == "synopsis-more" {
                        this.more = !this.more;
                    } else {
                        this.expanded = !this.expanded;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
    }
    pub(super) fn hero(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let dark = tokens::Palette::dark();
        let title = self.display_title(cx);
        let c = self.coordinator.read(cx);
        let details = if matches!(self.title, BrowseTitle::Cinemeta(_)) {
            c.details.value()
        } else {
            None
        };
        let poster_url = details
            .and_then(|d| d.poster_url.as_deref())
            .or_else(|| title.poster_url())
            .map(str::to_owned);
        let backdrop_url = match &self.title {
            BrowseTitle::Cinemeta(_) => details.and_then(|d| d.background_url.clone()),
            BrowseTitle::Tmdb(_) => c.tmdb_details.value().and_then(|d| d.backdrop_url.clone()),
        };
        let release = details
            .and_then(|d| d.release_info.clone())
            .or_else(|| match &title {
                BrowseTitle::Tmdb(t) => t.release_info.clone(),
                _ => None,
            });
        let (origin, fetched_at, error, busy) = if matches!(self.title, BrowseTitle::Tmdb(_)) {
            (
                c.tmdb_details.origin(),
                c.tmdb_details.fetched_at(),
                c.tmdb_details.error().map(str::to_owned),
                c.tmdb_details.busy(),
            )
        } else {
            (
                c.details.origin(),
                c.details.fetched_at(),
                c.details.error().map(str::to_owned),
                c.details.busy(),
            )
        };
        let mut poster = div()
            .id("detail-hero-poster")
            .w(geometry::POSTER_WIDTH)
            .h(geometry::POSTER_HEIGHT)
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px(s::S12)
                    .gap(s::S8)
                    .text_color(p.text_primary)
                    .child(
                        div()
                            .type_style(t::HEADING)
                            .text_center()
                            .child(title.name().to_owned()),
                    )
                    .when_some(title.year(), |el, year| {
                        el.child(
                            div()
                                .type_style(t::SMALL)
                                .text_color(p.text_muted)
                                .child(year.to_string()),
                        )
                    }),
            );
        if let Some(image) = self.image(
            poster_url.as_deref(),
            ArtworkKind::Poster,
            &self.title.key(),
            cx,
        ) {
            poster = poster.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        let bounds = self.bounds.clone();
        poster = poster.child(
            canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        );
        let mut copy = div()
            .flex_1()
            .min_w_0()
            .max_w(geometry::SYNOPSIS_MEASURE)
            .flex()
            .flex_col()
            .gap(s::S8)
            .text_color(dark.text_primary)
            .child(div().type_style(t::DISPLAY).child(title.name().to_owned()))
            .child(
                div()
                    .type_style(t::SMALL)
                    .text_color(dark.text_secondary)
                    .child(title_facts(&title)),
            );
        if title.rating().is_some() {
            copy = copy.child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S6)
                    .child(Chip::rating(title.rating()).over_art(true))
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(dark.text_primary)
                            .child("IMDb"),
                    ),
            );
        }
        copy = copy
            .child(
                div()
                    .type_style(t::BODY_LONG)
                    .w_full()
                    .max_w(geometry::SYNOPSIS_MEASURE)
                    .when(!self.more, |el| el.line_clamp(4).overflow_hidden())
                    .child(
                        title
                            .overview()
                            .unwrap_or("No synopsis available.")
                            .to_owned(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S6)
                    .child(self.disclosure(
                        "synopsis-more",
                        if self.more { "Less" } else { "More" },
                        self.more,
                        cx,
                    ))
                    .child(self.disclosure(
                        "detail-provenance",
                        "Details & provenance",
                        self.expanded,
                        cx,
                    )),
            );
        let band_height = (window.viewport_size().height * geometry::HERO_FRACTION)
            .max(geometry::POSTER_HEIGHT + s::S32 * 2.);
        let mut band = div()
            .bg(dark.bg_canvas)
            .relative()
            .w_full()
            .flex_none()
            .min_h(band_height)
            .when(!self.more, |el| el.h(band_height))
            .flex()
            .flex_col()
            .justify_end()
            .p(s::S32);
        if let Some(image) = self.image(
            backdrop_url.as_deref(),
            ArtworkKind::Backdrop,
            &format!("{}:backdrop", self.title.key()),
            cx,
        ) {
            band = band.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        band = band
            .child(div().absolute().inset_0().bg(linear_gradient(
                90.,
                linear_color_stop(dark.scrim_max, 0.),
                linear_color_stop(dark.scrim_max.opacity(0.), 0.65),
            )))
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(relative(geometry::FADE_FRACTION))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(p.bg_canvas.opacity(0.), 0.),
                        linear_color_stop(p.bg_canvas, 1.),
                    )),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h(geometry::POSTER_HEIGHT)
                    .flex()
                    .items_start()
                    .gap(s::S32)
                    .child(poster)
                    .child(copy),
            );
        let mut hero = div()
            .id("detail-hero")
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(band);
        let provider = if matches!(self.title, BrowseTitle::Tmdb(_)) {
            "TMDB"
        } else {
            "Cinemeta"
        };
        let mut facts = vec![("Metadata", provider.to_owned())];
        if title.kind() == MediaKind::Series {
            let c = self.coordinator.read(cx);
            if let Some(d) = c
                .details
                .value()
                .filter(|_| matches!(self.title, BrowseTitle::Cinemeta(_)))
            {
                facts.push(("Known episodes", d.episodes.len().to_string()));
            }
            if let Some(d) = c
                .tmdb_details
                .value()
                .filter(|_| matches!(self.title, BrowseTitle::Tmdb(_)))
            {
                facts.push((
                    "Known episodes",
                    d.seasons
                        .iter()
                        .map(|s| u64::from(s.episode_count))
                        .sum::<u64>()
                        .to_string(),
                ));
            }
        }

        if let Some(release) = release.filter(|r| !r.is_empty()) {
            facts.insert(0, ("Released", release));
        }
        if !title.genres().is_empty() {
            facts.push(("Genres", title.genres().join(" · ")));
        }
        hero = hero.child(
            div()
                .flex()
                .flex_none()
                .flex_wrap()
                .gap(s::S32)
                .px(s::S32)
                .py(s::S20)
                .children(facts.into_iter().map(|(label, value)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(s::S4)
                        .child(
                            div()
                                .type_style(t::SMALL)
                                .text_color(p.text_muted)
                                .child(label),
                        )
                        .child(
                            div()
                                .type_style(t::SMALL)
                                .text_color(p.text_secondary)
                                .child(value),
                        )
                })),
        );
        if self.expanded {
            let ids = match &title {
                BrowseTitle::Cinemeta(t) => format!("IMDb {}", t.id().as_str()),
                BrowseTitle::Tmdb(t) => format!(
                    "TMDB {} · IMDb {}",
                    t.id,
                    t.imdb.as_ref().map(|id| id.as_str()).unwrap_or("unmapped")
                ),
            };
            let freshness = match (origin, fetched_at) {
                (crate::app_state::Origin::Saved, Some(at)) => {
                    format!("Saved metadata · fetched at Unix {at}")
                }
                (crate::app_state::Origin::Fetched, Some(at)) => {
                    format!("Fetched metadata · fetched at Unix {at}")
                }
                _ => "Metadata not loaded".into(),
            };
            hero = hero.child(
                div()
                    .flex_none()
                    .px(s::S32)
                    .pb(s::S20)
                    .type_style(t::SMALL)
                    .text_color(p.text_secondary)
                    .child(format!(
                        "{ids} · Provider {provider} · Rating origin: IMDb only · {freshness}"
                    )),
            );
        }
        if busy {
            hero = hero.child(
                div()
                    .px(s::S32)
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child("Refreshing title metadata…"),
            );
        }
        if let Some(error) = error {
            hero = hero.child(
                div()
                    .px(s::S32)
                    .text_color(p.status_error)
                    .child(format!("Metadata could not be refreshed: {error}")),
            );
        }
        if title.kind() == MediaKind::Movie {
            hero = hero.child(self.recommendations(cx));
        }
        let weak = cx.entity().downgrade();
        let name = title.name().to_owned();
        let imdb = title.imdb().cloned();
        hero.context_menu("detail-backdrop-menu", move |_, _| {
            let mut items = vec![
                menu_item("Refresh", {
                    let weak = weak.clone();
                    move |_, cx| {
                        let _ = weak.update(cx, |s, cx| s.refresh(cx));
                    }
                }),
                menu_item("Copy title", {
                    let name = name.clone();
                    move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(name.clone()))
                }),
            ];
            if let Some(id) = imdb.clone() {
                items.push(menu_item("Open on IMDb", move |_, cx| {
                    cx.open_url(&format!("https://www.imdb.com/title/{}/", id.as_str()))
                }));
            }
            items
        })
        .into_any_element()
    }
}
