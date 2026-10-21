//! Facts block (ratings, facts grid, cast row) and the provenance footer.
//!
//! The view model below is filled from the fields the providers expose today;
//! richer title facts plug into `facts_view` without changing the layout.
use super::*;
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
}
#[derive(Default)]
pub(super) struct FactsView {
    pub tagline: Option<String>,
    pub ratings: Vec<RatingTile>,
    pub rows: Vec<(&'static str, String)>,
    pub cast: Vec<CastEntry>,
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

/// Most cast members shown in the row.
const CAST_MAX: usize = 12;

/// Date fields come as `YYYY-MM-DD`; anything else is shown as given.
fn date_text(raw: &str) -> String {
    air_date(raw).unwrap_or(raw).to_owned()
}

fn push_row(rows: &mut Vec<(&'static str, String)>, label: &'static str, value: Option<String>) {
    if let Some(value) = value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) {
        rows.push((label, value));
    }
}
fn push_list(rows: &mut Vec<(&'static str, String)>, label: &'static str, list: &[String]) {
    let items: Vec<&str> = list
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if !items.is_empty() {
        rows.push((label, items.join(", ")));
    }
}

impl Detail {
    /// Provider facts for the open title; default (all empty) until loaded.
    pub(super) fn title_facts(&self, cx: &App) -> TitleFacts {
        let c = self.coordinator.read(cx);
        match &self.title {
            BrowseTitle::Cinemeta(_) => c.details.value().map(|d| d.facts.clone()),
            BrowseTitle::Tmdb(_) => c.tmdb_details.value().map(|d| d.facts.clone()),
        }
        .unwrap_or_default()
    }

    pub(super) fn facts_view(&self, title: &BrowseTitle, cx: &App) -> FactsView {
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
                    _ => None,
                })
        };
        let series = title.kind() == MediaKind::Series;
        let rows = &mut view.rows;
        let first = if series {
            facts.first_air_date.as_deref()
        } else {
            facts.release_date.as_deref()
        };
        push_row(
            rows,
            if series { "First aired" } else { "Released" },
            first
                .map(date_text)
                .or(facts.release_info.clone())
                .or(release.filter(|r| !r.is_empty())),
        );
        if series {
            push_row(
                rows,
                "Last aired",
                facts.last_air_date.as_deref().map(date_text),
            );
        }
        push_row(rows, "Status", facts.status.clone());
        push_row(
            rows,
            "Runtime",
            facts
                .runtime_minutes
                .map(u64::from)
                .or(title.runtime().map(u64::from))
                .filter(|n| *n > 0 && (!series || facts.runtime_minutes.is_some()))
                .map(|n| format!("{n} min")),
        );
        push_row(rows, "Certification", facts.certification.clone());
        push_row(
            rows,
            "Language",
            facts.original_language.as_deref().map(str::to_uppercase),
        );
        push_list(rows, "Directors", &facts.directors);
        push_list(rows, "Creators", &facts.creators);
        push_list(rows, "Writers", &facts.writers);
        push_list(rows, "Studios", &facts.studios);
        push_list(rows, "Networks", &facts.networks);
        push_list(rows, "Countries", &facts.countries);
        if series {
            if let Some(n) = title.seasons() {
                rows.push(("Seasons", n.to_string()));
            }
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
                rows.push(("Episodes", n.to_string()));
            }
        }
        push_list(rows, "Genres", title.genres());
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
            })
            .collect();
        view
    }

    /// A prominent score card: star, large value, source and optional votes.
    fn rating_tile(&self, tile: &RatingTile, cx: &App) -> Div {
        let p = self.tinted_palette(cx);
        div()
            .flex()
            .items_center()
            .gap(s::S12)
            .px(s::S16)
            .py(s::S12)
            .rounded(radius::MD)
            .bg(p.bg_surface)
            .border(s::HAIRLINE)
            .border_color(p.line_subtle)
            .child(Icon::new(IconName::StarFilled).color(p.accent_text))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(s::S4)
                            .child(
                                div()
                                    .type_style(t::DISPLAY)
                                    .text_color(p.text_primary)
                                    .child(tile.value.clone()),
                            )
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_muted)
                                    .child("/ 10"),
                            ),
                    )
                    .child(
                        div()
                            .type_style(t::SMALL_MEDIUM)
                            .text_color(p.text_secondary)
                            .child(match &tile.detail {
                                Some(votes) => format!("{} · {votes}", tile.label),
                                None => tile.label.to_owned(),
                            }),
                    ),
            )
    }

    fn cast_row(&self, cast: &[CastEntry], width: Pixels, cx: &App) -> Stateful<Div> {
        let p = self.tinted_palette(cx);
        let fit = fit_grid(
            width,
            geometry::CAST_MIN_WIDTH,
            geometry::CAST_MAX_WIDTH,
            s::S12,
        );
        div()
            .id("detail-cast")
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(
                div()
                    .type_style(t::HEADING)
                    .text_color(p.text_primary)
                    .child("Cast"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(s::S12)
                    .children(cast.iter().take(fit.cols).map(|member| {
                        div()
                            .w(fit.tile)
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(s::S4)
                            .child(
                                div()
                                    .size(s::S48)
                                    .rounded_full()
                                    .bg(p.bg_placeholder)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .type_style(t::BODY_STRONG)
                                    .text_color(p.text_secondary)
                                    .child(initials(&member.name)),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .type_style(t::SMALL_MEDIUM)
                                    .text_color(p.text_primary)
                                    .text_center()
                                    .clamp_lines(2)
                                    .child(member.name.clone()),
                            )
                            .children(member.role.clone().map(|role| {
                                div()
                                    .w_full()
                                    .type_style(t::CAPTION)
                                    .text_color(p.text_muted)
                                    .text_center()
                                    .clamp_lines(2)
                                    .child(role)
                            }))
                    })),
            )
    }

    /// Ratings, tagline, a responsive facts grid and cast, under the hero.
    pub(super) fn facts_section(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
        width: Pixels,
        cx: &App,
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
        if view.rows.is_empty()
            && view.ratings.is_empty()
            && view.tagline.is_none()
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
        if let Some(tagline) = view.tagline.clone() {
            section = section.child(
                div()
                    .id("detail-tagline")
                    .max_w(px(900.))
                    .type_style(t::TITLE)
                    .italic()
                    .text_color(p.text_secondary)
                    .child(tagline),
            );
        }
        if !view.ratings.is_empty() {
            section = section.child(
                div()
                    .id("detail-ratings")
                    .flex()
                    .flex_wrap()
                    .gap(s::S12)
                    .children(view.ratings.iter().map(|tile| self.rating_tile(tile, cx))),
            );
        }
        if !view.rows.is_empty() {
            let fit = fit_grid(
                width,
                geometry::FACT_MIN_WIDTH,
                geometry::FACT_MAX_WIDTH,
                s::S24,
            );
            section = section.child(
                div()
                    .id("detail-facts-grid")
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap_x(s::S24)
                    .gap_y(s::S16)
                    .children(view.rows.into_iter().map(|(label, value)| {
                        div()
                            .w(fit.tile)
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
                    })),
            );
        }
        if !view.cast.is_empty() {
            section = section.child(self.cast_row(&view.cast, width, cx));
        }
        section.into_any_element()
    }

    /// Rows for the provenance footer: where the data came from and its ids.
    fn provenance_rows(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
    ) -> Vec<(&'static str, String)> {
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
        let ids = match title {
            BrowseTitle::Cinemeta(t) => format!("IMDb {}", t.id().as_str()),
            BrowseTitle::Tmdb(t) => match t.imdb.as_ref() {
                Some(imdb) => format!("TMDB {} · IMDb {}", t.id, imdb.as_str()),
                None => format!("TMDB {}", t.id),
            },
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
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(div().w(geometry::FACT_LABEL_WIDTH).flex_none().child(label))
                            .child(div().flex_1().min_w_0().child(value))
                    }),
            )
            .children(attribution.map(|text| {
                div()
                    .id("detail-attribution")
                    .pt(s::S4)
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
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
