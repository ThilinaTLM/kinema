//! Detail hero presentation: backdrop, poster, meta badges, actions and the
//! "Details" disclosure.
use super::*;
use crate::widgets::menu::OpenContextMenu;
use crate::widgets::patterns::{POSTER_ASPECT, POSTER_WIDTH};
use kinema_core::catalog::ImdbId;
use std::time::{SystemTime, UNIX_EPOCH};

/// Strength of the even veil over the backdrop (times the palette's scrim).
const BACKDROP_VEIL: f32 = 0.5;
/// Opacity of the leading-edge scrim that keeps the title legible.
const SIDE_SCRIM: f32 = 0.55;
/// Share of the hero height over which the art melts into the canvas.
const HERO_RAMP: f32 = 0.7;
const HERO_RAMP_BARE: f32 = 0.0;
const OVERVIEW_LINES: usize = 3;
/// Overviews at most this long need no More toggle.
const OVERVIEW_COLLAPSE_CHARS: usize = 220;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Overflow menu shared by the More button and right-click on the hero.
fn more_items(weak: WeakEntity<Detail>, name: String, imdb: Option<ImdbId>) -> Vec<MenuItem> {
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
    if let Some(id) = imdb {
        items.push(menu_item("Open on IMDb", move |_, cx| {
            cx.open_url(&format!("https://www.imdb.com/title/{}/", id.as_str()))
        }));
    }
    items
}

impl Detail {
    /// Poster face: placeholder title until the image arrives.
    fn poster_face(
        &self,
        title: &BrowseTitle,
        poster_url: Option<&str>,
        cx: &mut App,
    ) -> Stateful<Div> {
        let p = *tokens::palette(cx);
        let bounds = self.bounds.clone();
        let mut poster = div()
            .id("detail-hero-poster")
            .size_full()
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
        if let Some(image) = self.image(poster_url, ArtworkKind::Poster, &self.title.key(), cx) {
            poster = poster.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        poster.child(
            canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
    }

    /// Year, runtime/seasons, genres and rating as badges.
    fn meta_badges(title: &BrowseTitle) -> Vec<AnyElement> {
        let mut badges = Vec::new();
        if title.kind() == MediaKind::Series {
            badges.push(
                Badge::new("Series")
                    .icon(IconName::Tv)
                    .tone(BadgeTone::Info)
                    .into_any_element(),
            );
        }
        if let Some(year) = title.year() {
            badges.push(Badge::new(year.to_string()).into_any_element());
        }
        if title.kind() == MediaKind::Series {
            if let Some(n) = title.seasons() {
                badges.push(
                    Badge::new(if n == 1 {
                        "1 season".to_owned()
                    } else {
                        format!("{n} seasons")
                    })
                    .into_any_element(),
                );
            }
        } else if let Some(n) = title.runtime() {
            badges.push(Badge::new(format!("{n} min")).into_any_element());
        }
        badges.extend(
            title
                .genres()
                .iter()
                .take(3)
                .map(|g| Badge::new(g.clone()).into_any_element()),
        );
        if let Some(rating) = title.rating().and_then(rating_label) {
            badges.push(
                Badge::new(rating)
                    .icon(IconName::StarFilled)
                    .tone(BadgeTone::Accent)
                    .into_any_element(),
            );
        }
        badges
    }

    fn primary_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let action = self.primary_action(cx);
        let label: SharedString = match (&action, &self.episode) {
            (PrimaryAction::Episodes, _) => "Episodes".into(),
            (_, Some(ep)) => format!("Play {}", episode_marker(ep)).into(),
            _ => "Play".into(),
        };
        let icon = if matches!(action, PrimaryAction::Episodes) {
            IconName::Tv
        } else {
            IconName::Play
        };
        let blocked = match &action {
            PrimaryAction::Blocked(reason) => Some(*reason),
            _ => None,
        };
        let button = Button::new("detail-primary", label)
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Large)
            .icon(icon)
            .disabled(blocked.is_some())
            .on_click(cx.listener(|this, _, _, cx| this.run_primary(cx)));
        match blocked {
            // The disabled button swallows hover, so the reason hangs on a wrapper.
            Some(reason) => div()
                .id("detail-primary-blocked")
                .tooltip(tooltip(reason))
                .child(button)
                .into_any_element(),
            None => button.into_any_element(),
        }
    }

    /// Rough height of everything the left column shows under the hero, used
    /// to let the hero grow into otherwise empty space (no dead band).
    pub(super) fn below_hero_height(&self) -> Pixels {
        let mut height = geometry::INLINE_FACTS_HEIGHT;
        if self.expanded {
            height += geometry::DETAILS_PANEL_HEIGHT;
        }
        if self.title.kind() == MediaKind::Movie && matches!(self.title, BrowseTitle::Tmdb(_)) {
            height += geometry::SIMILAR_STRIP_HEIGHT;
        }
        height
    }

    /// Left column: a cinematic hero that grows to fill spare height, then the
    /// inline facts, optional provenance panel and similar titles.
    pub(super) fn hero(&self, min_height: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let title = self.display_title(cx);
        let status = self.meta_status(cx);
        let p = self.tinted_palette(cx);
        let light = tokens::theme(cx).mode == tokens::Mode::Light;
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
            };
            (poster, backdrop)
        };
        let backdrop = self.backdrop_state(backdrop_url.as_deref(), poster_url.as_deref(), cx);
        let backdrop = match backdrop {
            ArtworkState::Ready(image) => Some(image),
            _ => None,
        };
        // Dark chips only where dark art is actually behind the buttons; in light
        // mode the art melts into a light canvas well before the actions.
        let over_art = backdrop.is_some() && !light;
        let poster = self.poster_face(&title, poster_url.as_deref(), cx);

        // --- backdrop layers -------------------------------------------------
        let fade = motion::duration(Motion::Crossfade, cx);
        let backdrop_layer = backdrop.map(|image| {
            let key = std::sync::Arc::as_ptr(&image) as *const () as usize;
            let layer = div()
                .absolute()
                .inset_0()
                .child(img(image).object_fit(ObjectFit::Cover).size_full());
            if fade.is_zero() {
                layer.into_any_element()
            } else {
                layer
                    .with_animation(
                        ("detail-backdrop", key),
                        Animation::new(fade).with_easing(motion::ease_out_cubic),
                        |el, t| el.opacity(t),
                    )
                    .into_any_element()
            }
        });
        let has_art = backdrop_layer.is_some();
        // One long ramp into the canvas; light mode stacks a second ramp so
        // dark ink always sits on a mostly-canvas surface.
        let ramp = |fraction: f32| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(relative(fraction))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(p.bg_canvas.opacity(0.), 0.),
                    linear_color_stop(p.bg_canvas, 1.),
                ))
        };

        // --- title block -----------------------------------------------------
        let mut actions = div()
            .flex()
            .items_center()
            .gap(s::S8)
            .pt(s::S4)
            .child(self.primary_button(cx));
        if self.is_series() && self.episode.is_some() {
            actions = actions.child(
                Button::new("detail-all-episodes", "All episodes")
                    .icon(IconName::Tv)
                    .over_art(over_art)
                    .on_click(cx.listener(|this, _, _, cx| this.focus_episodes(cx))),
            );
        }
        actions = actions
            .child(
                Button::new(
                    "detail-provenance",
                    if self.expanded {
                        "Hide details"
                    } else {
                        "Details"
                    },
                )
                .icon(if self.expanded {
                    IconName::ChevronDown
                } else {
                    IconName::Info
                })
                .over_art(over_art)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.expanded = !this.expanded;
                    cx.notify();
                })),
            )
            .child(self.more_button(&title, over_art, cx));

        let overview = match title.overview().filter(|o| !o.trim().is_empty()) {
            Some(text) => {
                let collapsible = text.chars().count() > OVERVIEW_COLLAPSE_CHARS;
                let open = self.overview_open && collapsible;
                Some(
                    div()
                        .w_full()
                        .max_w(geometry::OVERVIEW_MAX_WIDTH)
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(s::S4)
                        .child(
                            div()
                                .type_style(t::BODY_LONG)
                                .text_color(p.text_secondary)
                                .when(!open, |el| el.line_clamp(OVERVIEW_LINES))
                                .child(text.to_owned()),
                        )
                        .when(collapsible, |el| {
                            el.child(
                                Button::new(
                                    "detail-overview-toggle",
                                    if open { "Less" } else { "More" },
                                )
                                .variant(ButtonVariant::Ghost)
                                .icon(if open {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.overview_open = !this.overview_open;
                                        cx.notify();
                                    },
                                )),
                            )
                        })
                        .into_any_element(),
                )
            }
            // The overview arrives with the full metadata: show its shape, not text.
            None if status.busy && !status.loaded => Some(
                div()
                    .w(geometry::OVERVIEW_SKELETON_WIDTH)
                    .max_w_full()
                    .child(SkeletonText::new(3))
                    .into_any_element(),
            ),
            None => None,
        };
        let copy = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(s::S8)
            .child(
                div()
                    .type_style(t::DISPLAY)
                    .text_color(p.text_primary)
                    .line_clamp(2)
                    .child(title.name().to_owned()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(s::S8)
                    .children(Self::meta_badges(&title)),
            )
            .children(overview)
            .child(actions);
        let poster_frame = div()
            .flex_none()
            .w(POSTER_WIDTH)
            .h(POSTER_WIDTH * POSTER_ASPECT)
            .rounded(radius::MD)
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .border(s::HAIRLINE)
            .border_color(p.poster_edge)
            .shadow(tokens::elevation::overlay(&p))
            .child(poster);

        let banner = div()
            .id("detail-hero")
            .relative()
            .w_full()
            .min_h(min_height)
            .flex_grow_1()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .justify_end()
            .overflow_hidden()
            .bg(p.bg_canvas)
            .children(backdrop_layer)
            // Even veil keeps the art readable but present.
            .when(has_art, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(p.backdrop_scrim.opacity(BACKDROP_VEIL)),
                )
                .child(div().absolute().inset_0().bg(linear_gradient(
                    90.,
                    linear_color_stop(p.bg_canvas.opacity(SIDE_SCRIM), 0.),
                    linear_color_stop(p.bg_canvas.opacity(0.), 0.7),
                )))
            })
            .child(ramp(if has_art { HERO_RAMP } else { HERO_RAMP_BARE }))
            .when(has_art && light, |el| el.child(ramp(HERO_RAMP)))
            .child(
                div()
                    .relative()
                    .flex()
                    .items_end()
                    .gap(s::S24)
                    .px(s::GRID_INSET)
                    .pt(s::S48)
                    .pb(s::S24)
                    .child(poster_frame)
                    .child(copy),
            );

        let mut stack = div()
            .id("detail-hero-body")
            .w_full()
            .flex_grow_1()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(p.bg_canvas)
            .child(banner)
            .child(self.inline_facts(&title, cx));
        if self.expanded {
            stack = stack.child(self.details_panel(&title, &status, cx));
        } else if let Some(error) = status.error.clone() {
            let retry = cx.listener(|this, _, _, cx| this.retry_metadata(cx));
            stack = stack.child(
                div().w_full().px(s::GRID_INSET).pt(s::S12).child(
                    crate::widgets::notice::Notice::new(
                        crate::widgets::notice::NoticeKind::Warning,
                        if status.loaded {
                            "Couldn’t refresh title details"
                        } else {
                            "Couldn’t load title details"
                        },
                    )
                    .body(error)
                    .action(Button::new("detail-meta-retry", "Retry").on_click(retry)),
                ),
            );
        }
        if title.kind() == MediaKind::Movie {
            stack = stack.child(self.recommendations(cx));
        }
        let weak = cx.entity().downgrade();
        let name = title.name().to_owned();
        let imdb = title.imdb().cloned();
        stack
            .context_menu("detail-backdrop-menu", move |_, _| {
                more_items(weak.clone(), name.clone(), imdb.clone())
            })
            .into_any_element()
    }

    /// Always-visible facts under the hero: released, genres, episodes, source.
    fn inline_facts(&self, title: &BrowseTitle, cx: &App) -> Div {
        let p = self.tinted_palette(cx);
        let mut facts = Vec::new();
        let release = {
            let c = self.coordinator.read(cx);
            c.details
                .value()
                .filter(|_| matches!(self.title, BrowseTitle::Cinemeta(_)))
                .and_then(|d| d.release_info.clone())
                .or_else(|| match title {
                    BrowseTitle::Tmdb(t) => t.release_info.clone(),
                    _ => None,
                })
        };
        if let Some(release) = release.filter(|r| !r.is_empty()) {
            facts.push(("Released", release));
        }
        if !title.genres().is_empty() {
            facts.push(("Genres", title.genres().join(", ")));
        }
        if title.kind() == MediaKind::Series {
            let c = self.coordinator.read(cx);
            let known = match &self.title {
                BrowseTitle::Cinemeta(_) => c.details.value().map(|d| d.episodes.len() as u64),
                BrowseTitle::Tmdb(_) => c.tmdb_details.value().map(|d| {
                    d.seasons
                        .iter()
                        .map(|s| u64::from(s.episode_count))
                        .sum::<u64>()
                }),
            };
            if let Some(n) = known.filter(|n| *n > 0) {
                facts.push(("Episodes", n.to_string()));
            }
        }
        facts.push((
            "Source",
            if matches!(self.title, BrowseTitle::Tmdb(_)) {
                "TMDB"
            } else {
                "Cinemeta"
            }
            .to_owned(),
        ));
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_wrap()
            .gap_x(s::S32)
            .gap_y(s::S12)
            .px(s::GRID_INSET)
            .py(s::S16)
            .children(facts.into_iter().map(|(label, value)| {
                div()
                    .flex()
                    .flex_col()
                    .gap(s::S2)
                    .child(
                        div()
                            .type_style(t::CAPTION)
                            .text_color(p.text_muted)
                            .child(label),
                    )
                    .child(
                        div()
                            .type_style(t::BODY)
                            .text_color(p.text_secondary)
                            .child(value),
                    )
            }))
    }

    /// Icon button that opens the same menu as right-click, anchored to itself.
    fn more_button(
        &self,
        title: &BrowseTitle,
        over_art: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let name = title.name().to_owned();
        let imdb = title.imdb().cloned();
        // Same height as the neighbouring buttons and centred, so the icon
        // shares their vertical centre.
        div()
            .id("detail-more")
            .h(s::CONTROL_HEIGHT)
            .flex()
            .items_center()
            .child(
                IconButton::new("detail-more-button", IconName::MoreHorizontal, "More")
                    .over_art(over_art)
                    .on_click(|_, window, cx| {
                        window.dispatch_action(OpenContextMenu.boxed_clone(), cx);
                    }),
            )
            .context_menu("detail-more-menu", move |_, _| {
                more_items(weak.clone(), name.clone(), imdb.clone())
            })
    }

    /// "Details" disclosure: where the metadata came from and how fresh it is.
    fn details_panel(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
        let rows = self.fact_rows(title, status, cx);
        let skeleton = div()
            .flex()
            .flex_col()
            .gap(s::S12)
            .children((0..4).map(|_| Skeleton::line().w(relative(0.5)).into_any_element()));
        let facts = div()
            .flex()
            .flex_col()
            .gap(s::S8)
            .children(rows.into_iter().map(|(label, value)| {
                div()
                    .flex()
                    .gap(s::S12)
                    .child(
                        div()
                            .w(geometry::DETAIL_LABEL_WIDTH)
                            .flex_none()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child(value),
                    )
            }));
        let sel = selection(status.loaded, false, status.error.is_some(), status.busy);
        let weak = cx.entity().downgrade();
        let view = state_view("detail-facts", sel)
            .skeleton(skeleton)
            .content(facts)
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_metadata(cx));
            });
        let view = match &status.error {
            Some(error) => view.message(error.clone()),
            None => view,
        };
        div()
            .w_full()
            .px(s::GRID_INSET)
            .pt(s::S16)
            .child(
                crate::widgets::patterns::Card::new()
                    .max_w(geometry::DETAILS_MAX_WIDTH)
                    .child(div().p(s::S16).child(view)),
            )
            .into_any_element()
    }

    fn fact_rows(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
        _cx: &App,
    ) -> Vec<(&'static str, String)> {
        let mut rows = Vec::new();
        if title.rating().is_some() {
            rows.push(("Rating", "IMDb".into()));
        }
        let provider = if matches!(self.title, BrowseTitle::Tmdb(_)) {
            "TMDB"
        } else {
            "Cinemeta"
        };
        let freshness = match (status.origin, status.fetched_at) {
            (crate::app_state::Origin::Saved, Some(at)) => {
                format!("{provider} · saved copy from {}", age_label(now_unix(), at))
            }
            (crate::app_state::Origin::Fetched, Some(at)) => {
                format!("{provider} · updated {}", age_label(now_unix(), at))
            }
            _ => format!("{provider} · not loaded yet"),
        };
        rows.push(("Metadata", freshness));
        let ids = match title {
            BrowseTitle::Cinemeta(t) => format!("IMDb {}", t.id().as_str()),
            BrowseTitle::Tmdb(t) => match t.imdb.as_ref() {
                Some(imdb) => format!("TMDB {} · IMDb {}", t.id, imdb.as_str()),
                None => format!("TMDB {}", t.id),
            },
        };
        rows.push(("Identifiers", ids));
        rows
    }
}
