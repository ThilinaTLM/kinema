//! Facts block (ratings, details columns, cast grid) and the provenance footer.
//!
//! The view model below is filled from the fields the providers expose today;
//! richer title facts plug into `facts_view` without changing the layout.
use super::*;
use crate::ui::theme::typography::{TextSize, TypeExt, Weight};
use kinema_core::catalog::TitleFacts;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// One prominent score: `8.1` with its source label and optional detail.
pub(super) struct RatingTile {
    pub label: &'static str,
    pub value: String,
    pub detail: Option<String>,
}
pub(super) struct CastEntry {
    pub name: String,
    pub role: Option<String>,
    pub profile_url: Option<String>,
}
#[derive(Default)]
pub(super) struct FactsView {
    pub tagline: Option<String>,
    pub ratings: Vec<RatingTile>,
    pub glance: Vec<(&'static str, String)>,
    /// Facts for the "Details" block that the hero does not already show.
    pub details: Vec<(&'static str, String)>,
    pub cast: Vec<CastEntry>,
    /// Pieces of the hero meta line (year, runtime, certification, genres…).
    pub meta: Vec<layout::MetaPart>,
}

/// Up to two letters for a cast placeholder avatar.
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// `12,345 votes`; singular for one.
fn votes_label(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{out} vote{}", if n == 1 { "" } else { "s" })
}

/// Cast members kept from the provider.
const CAST_MAX: usize = kinema_core::catalog::MAX_CAST;
/// Cast photo frame: 3:4 portrait.
const CAST_PHOTO_ASPECT: f32 = 4. / 3.;

impl Detail {
    /// Provider facts for the open title; default (all empty) until loaded.
    pub(super) fn title_facts(&self, cx: &App) -> TitleFacts {
        let c = self.coordinator.read(cx);
        match &self.title {
            BrowseTitle::Cinemeta(_) => c.details.value().map(|d| d.facts.clone()),
            BrowseTitle::Tmdb(_) => c.tmdb_details.value().map(|d| d.facts.clone()),
            BrowseTitle::Local(_) => self
                .local_detail(cx)
                .map(|d| local::local_facts(&d.metadata)),
        }
        .unwrap_or_default()
    }

    /// Season summary from the TMDB series details (poster, air date,
    /// overview) -- available for every season without loading it.
    pub(super) fn tmdb_season_summary(
        &self,
        season: u32,
        cx: &App,
    ) -> Option<crate::tmdb::TmdbSeasonSummary> {
        match &self.title {
            BrowseTitle::Tmdb(_) | BrowseTitle::Local(_) => self
                .coordinator
                .read(cx)
                .tmdb_details
                .value()
                .and_then(|d| d.seasons.iter().find(|s| s.number == season).cloned()),
            BrowseTitle::Cinemeta(_) => None,
        }
    }

    /// Episodes known for the whole show (not just the open season).
    fn known_episode_count(&self, cx: &App) -> Option<u64> {
        let c = self.coordinator.read(cx);
        match &self.title {
            BrowseTitle::Cinemeta(_) => c.details.value().map(|d| d.episodes.len() as u64),
            BrowseTitle::Tmdb(_) => c.tmdb_details.value().map(|d| {
                d.seasons
                    .iter()
                    .map(|s| u64::from(s.episode_count))
                    .sum::<u64>()
            }),
            BrowseTitle::Local(_) => self.local_detail(cx).map(|d| d.episodes.len() as u64),
        }
        .filter(|n| *n > 0)
    }

    /// Facts model, memoized per coordinator revision: every input is coordinator
    /// state (observed by `data_rev`) or the immutable open title, so scrolling and
    /// other repaints reuse it instead of re-deriving ratings, details and cast.
    pub(super) fn facts_view(&self, title: &BrowseTitle, cx: &App) -> Rc<FactsView> {
        let mut memo = self.facts_memo.borrow_mut();
        if let Some((rev, view)) = memo.as_ref()
            && *rev == self.data_rev
        {
            return view.clone();
        }
        let view = Rc::new(self.compute_facts_view(title, cx));
        *memo = Some((self.data_rev, view.clone()));
        view
    }

    fn compute_facts_view(&self, title: &BrowseTitle, cx: &App) -> FactsView {
        #[cfg(all(test, feature = "ui-probe-test"))]
        super::work::count(super::work::Section::FactsModel, 1);
        let facts = self.title_facts(cx);
        let mut view = FactsView {
            tagline: facts
                .tagline
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned),
            ..FactsView::default()
        };
        // IMDb and TMDB scores are separate facts; neither stands in for the other.
        let imdb = facts.imdb_rating.map(|r| r.value()).or(title.rating());
        if let Some(score) = imdb.and_then(rating_label) {
            view.ratings.push(RatingTile {
                label: "IMDb",
                value: score,
                detail: None,
            });
        }
        if let Some(score) = facts.tmdb_rating.map(|r| r.value()).and_then(rating_label) {
            view.ratings.push(RatingTile {
                label: "TMDB",
                value: score,
                detail: facts.tmdb_vote_count.filter(|n| *n > 0).map(votes_label),
            });
        }
        let release = {
            let c = self.coordinator.read(cx);
            c.details
                .value()
                .filter(|_| matches!(self.title, BrowseTitle::Cinemeta(_)))
                .and_then(|d| d.release_info.clone())
                .or_else(|| match title {
                    BrowseTitle::Tmdb(t) => t.release_info.clone(),
                    BrowseTitle::Local(t) => t
                        .metadata
                        .text(if t.metadata.kind == MediaKind::Movie {
                            "release_date"
                        } else {
                            "first_air_date"
                        })
                        .map(str::to_owned),
                    _ => None,
                })
        };
        let series = title.kind() == MediaKind::Series;
        let first = if series {
            facts.first_air_date.as_deref()
        } else {
            facts.release_date.as_deref()
        };
        let released = first
            .map(layout::format_date)
            .or(facts.release_info.clone())
            .or(release.filter(|r| !r.is_empty()))
            .map(|r| r.trim().to_owned())
            .filter(|r| !r.is_empty());
        let runtime = facts
            .runtime_minutes
            .or_else(|| {
                // A series' catalog runtime is a per-episode guess only when the
                // provider's facts carry it.
                title.runtime().map(u32::from).filter(|_| !series)
            })
            .filter(|n| *n > 0);
        let episodes = series.then(|| self.known_episode_count(cx)).flatten();
        let source = layout::FactSource {
            series,
            facts: &facts,
            released,
            runtime,
        };
        view.glance = layout::glance_rows(&source);
        view.details = layout::detail_rows(&source);
        view.meta = layout::meta_parts(&layout::MetaInput {
            series,
            year: title.year(),
            facts: &facts,
            runtime,
            seasons: title.seasons(),
            episodes,
            genres: title.genres(),
        });
        view.cast = facts
            .cast
            .iter()
            .filter(|m| !m.name.trim().is_empty())
            .take(CAST_MAX)
            .map(|m| CastEntry {
                name: m.name.trim().to_owned(),
                role: m
                    .character
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned),
                profile_url: m.profile_url.clone(),
            })
            .collect();
        view
    }

    /// Key facts in the story column, under the meta line: rating blocks
    /// (star, score, source) then short label-over-value facts, separated by
    /// hairlines and wrapping onto more lines when narrow.
    pub(super) fn key_facts(&self, view: &FactsView, cx: &App) -> Option<Stateful<Div>> {
        if view.ratings.is_empty() && view.glance.is_empty() {
            return None;
        }
        let p = self.tinted_palette(cx);
        let caption = |text: SharedString| {
            div()
                .text(TextSize::Xs)
                .weight(Weight::Medium)
                .text_color(p.subtle_foreground)
                .child(text)
        };
        let ratings = view.ratings.iter().enumerate().map(|(i, tile)| {
            let source: SharedString = match &tile.detail {
                Some(detail) => format!("{} \u{b7} {detail}", tile.label).into(),
                None => tile.label.into(),
            };
            div()
                .id(("detail-rating", i))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(s::S4)
                        .child(Icon::new(IconName::StarFilled).color(p.primary_text))
                        .child(
                            div()
                                .text(TextSize::Lg)
                                .weight(Weight::Bold)
                                .text_color(p.foreground)
                                .child(tile.value.clone()),
                        )
                        .child(
                            div()
                                .text(TextSize::Sm)
                                .text_color(p.subtle_foreground)
                                .child("/10"),
                        ),
                )
                .child(caption(source))
                .into_any_element()
        });
        let facts = view.glance.iter().map(|(label, value)| {
            div()
                .flex()
                .flex_col()
                .child(caption((*label).into()))
                .child(
                    div()
                        .text(TextSize::Sm)
                        .weight(Weight::Medium)
                        .text_color(p.foreground)
                        .child(value.clone()),
                )
                .into_any_element()
        });
        let divider = || {
            div()
                .flex_none()
                .w(size::HAIRLINE)
                .h(s::S24)
                .bg(p.border)
                .into_any_element()
        };
        let mut items: Vec<AnyElement> = Vec::new();
        for (i, item) in ratings.chain(facts).enumerate() {
            if i > 0 {
                items.push(divider());
            }
            items.push(item);
        }
        Some(
            div()
                .id("detail-key-facts")
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_x(s::S16)
                .gap_y(s::S8)
                .py(s::S4)
                .children(items),
        )
    }

    /// One borderless detail: small uppercase caption over a wrapping value.
    fn detail_item(&self, label: &'static str, value: &str, width: Pixels, cx: &App) -> Div {
        let p = self.tinted_palette(cx);
        div()
            .w(width)
            .flex_none()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(s::S4)
            .child(
                div()
                    .text(TextSize::Xs)
                    .weight(Weight::Semibold)
                    .text_color(p.subtle_foreground)
                    .child(label.to_uppercase()),
            )
            .child(
                div()
                    .text(TextSize::Base)
                    .text_color(p.foreground)
                    .child(value.to_owned()),
            )
    }

    /// Portrait card: 3:4 photo (initials until/unless it loads), name, role.
    fn cast_card(&self, member: &CastEntry, tile: Pixels, cx: &mut Context<Self>) -> Div {
        let p = self.tinted_palette(cx);
        let mut photo = div()
            .w(tile)
            .h(tile * CAST_PHOTO_ASPECT)
            .relative()
            .overflow_hidden()
            .rounded(radius::LG)
            .bg(p.muted)
            .border(size::HAIRLINE)
            .border_color(p.poster_edge)
            .flex()
            .items_center()
            .justify_center()
            .text(TextSize::Xl)
            .weight(Weight::Semibold)
            .text_color(p.subtle_foreground)
            .child(initials(&member.name));
        if let Some(image) = self.image(
            member.profile_url.as_deref(),
            ArtworkKind::Poster,
            &format!("cast:{}", member.name),
            cx,
        ) {
            photo = photo.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        div()
            .w(tile)
            .flex_none()
            .flex()
            .flex_col()
            .gap(s::S2)
            .child(photo)
            .child(
                div()
                    .pt(s::S4)
                    .text(TextSize::Sm)
                    .weight(Weight::Medium)
                    .weight(Weight::Semibold)
                    .text_color(p.foreground)
                    .truncate()
                    .child(member.name.clone()),
            )
            .children(member.role.clone().map(|role| {
                div()
                    .text(TextSize::Sm)
                    .text_color(p.subtle_foreground)
                    .truncate()
                    .child(role)
            }))
    }

    /// Wrapping cast grid: two rows, then "Show all N" expands in place.
    fn cast_section(
        &self,
        cast: &[CastEntry],
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let fit = fit_grid(width, geometry::CAST_MIN, geometry::CAST_MAX, s::S12);
        let total = cast.len();
        let visible = layout::cast_visible(total, fit.cols, self.cast_expanded);
        let toggle = layout::cast_has_toggle(total, fit.cols);
        let expanded = self.cast_expanded;
        let cards: Vec<Div> = cast
            .iter()
            .take(visible)
            .map(|member| self.cast_card(member, fit.tile, cx))
            .collect();
        div()
            .id("detail-cast")
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(Section::new("Cast").count(total))
            .child(
                div()
                    .id("detail-cast-grid")
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap_x(s::S12)
                    .gap_y(s::S16)
                    .children(cards),
            )
            .when(toggle, |el| {
                el.child(
                    div().w_full().flex().justify_center().pt(s::S4).child(
                        Button::new(
                            "detail-cast-toggle",
                            if expanded {
                                "Show fewer".to_owned()
                            } else {
                                format!("Show all {total}")
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cast_expanded = !this.cast_expanded;
                            cx.notify();
                        })),
                    ),
                )
            })
    }

    /// Borderless "Details" columns (under "About the series" for series)
    /// followed by the cast grid.
    pub(super) fn facts_section(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.tinted_palette(cx);
        let view = self.facts_view(title, cx);
        let mut section = div()
            .id("detail-facts")
            .w_full()
            .py(s::S20)
            .flex()
            .flex_col()
            .gap(s::S20);
        if view.details.is_empty()
            && view.ratings.is_empty()
            && view.cast.is_empty()
            && status.busy
            && !status.loaded
        {
            return section
                .child(
                    div().flex().flex_col().gap(s::S12).children(
                        (0..3).map(|_| Skeleton::line().w(relative(0.4)).into_any_element()),
                    ),
                )
                .into_any_element();
        }
        if self.is_series() && (!view.details.is_empty() || !view.cast.is_empty()) {
            section = section.child(
                div()
                    .id("detail-about")
                    .text(TextSize::Xl)
                    .weight(Weight::Semibold)
                    .text_color(p.foreground)
                    .child("About the series"),
            );
        }
        if !view.details.is_empty() {
            let gap = s::S24;
            let fit = fit_grid(
                width,
                crate::ui::theme::layout::detail::DETAIL_FACT_MIN,
                crate::ui::theme::layout::detail::DETAIL_FACT_MAX,
                gap,
            );
            let grid = div()
                .id("detail-facts-grid")
                .w_full()
                .flex()
                .flex_wrap()
                .gap_x(gap)
                .gap_y(s::S16)
                .children(
                    view.details
                        .iter()
                        .map(|(label, value)| self.detail_item(label, value, fit.tile, cx)),
                );
            section = if self.is_series() {
                section.child(grid)
            } else {
                section.child(Section::new("Details").child(grid))
            };
        }
        if !view.cast.is_empty() {
            section = section.child(self.cast_section(&view.cast, width, cx));
        }
        section.into_any_element()
    }

    /// Rows for the provenance footer: where the data came from and its ids.
    fn provenance_rows(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
    ) -> Vec<(&'static str, String)> {
        let provider = if matches!(self.title, BrowseTitle::Local(_)) {
            "Local catalog"
        } else if matches!(self.title, BrowseTitle::Tmdb(_)) {
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
        let ids = match title {
            BrowseTitle::Cinemeta(t) => format!("IMDb {}", t.id().as_str()),
            BrowseTitle::Tmdb(t) => match t.imdb.as_ref() {
                Some(imdb) => format!("TMDB {} · IMDb {}", t.id, imdb.as_str()),
                None => format!("TMDB {}", t.id),
            },
            BrowseTitle::Local(t) => {
                let mut ids = vec![format!("Canonical {}", t.title_id.get())];
                ids.extend(
                    t.metadata
                        .identifiers
                        .iter()
                        .map(|id| format!("{} {}", id.namespace.to_uppercase(), id.external_id)),
                );
                ids.join(" · ")
            }
        };
        vec![("Metadata", freshness), ("Identifiers", ids)]
    }

    /// Quiet provenance at the very bottom of the page.
    pub(super) fn metadata_footer(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
        cx: &App,
    ) -> AnyElement {
        let p = self.tinted_palette(cx);
        let attribution = self.coordinator.read(cx).detail_attribution();
        div()
            .id("detail-provenance")
            .w_full()
            .py(s::S24)
            .flex()
            .flex_col()
            .gap(s::S4)
            .children(
                self.provenance_rows(title, status)
                    .into_iter()
                    .map(|(label, value)| {
                        div()
                            .flex()
                            .gap(s::S12)
                            .text(TextSize::Sm)
                            .text_color(p.subtle_foreground)
                            .child(div().w(geometry::FACT_LABEL_WIDTH).flex_none().child(label))
                            .child(div().flex_1().min_w_0().child(value))
                    }),
            )
            .children(attribution.map(|text| {
                div()
                    .id("detail-attribution")
                    .pt(s::S4)
                    .text(TextSize::Sm)
                    .text_color(p.subtle_foreground)
                    .child(text)
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{initials, votes_label};
    #[test]
    fn votes_group_thousands() {
        assert_eq!(votes_label(1), "1 vote");
        assert_eq!(votes_label(999), "999 votes");
        assert_eq!(votes_label(12_345), "12,345 votes");
        assert_eq!(votes_label(1_234_567), "1,234,567 votes");
    }
    #[test]
    fn initials_take_two_letters() {
        assert_eq!(initials("ada lovelace byron"), "AL");
        assert_eq!(initials("Cher"), "C");
        assert_eq!(initials(""), "");
    }
}
