//! Series episodes: pure season grouping and a virtualized episode list.

use gpui_kit::component::{
    ActiveTheme, IndexPath,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::*;
use kinema_core::catalog::CatalogEpisode;

pub(crate) const ROW_HEIGHT: f32 = 68.0;

/// Distinct season numbers in ascending order (season 0 is specials).
pub(crate) fn seasons(episodes: &[CatalogEpisode]) -> Vec<u32> {
    let mut seasons: Vec<u32> = episodes.iter().map(|e| e.season).collect();
    seasons.sort_unstable();
    seasons.dedup();
    seasons
}

/// Episodes of one season ordered by number.
pub(crate) fn season_episodes(episodes: &[CatalogEpisode], season: u32) -> Vec<CatalogEpisode> {
    let mut rows: Vec<CatalogEpisode> = episodes
        .iter()
        .filter(|e| e.season == season)
        .cloned()
        .collect();
    rows.sort_by_key(|e| e.number);
    rows
}

/// The season to show first: the lowest regular season, else specials.
pub(crate) fn default_season(seasons: &[u32]) -> Option<u32> {
    seasons
        .iter()
        .copied()
        .find(|s| *s > 0)
        .or_else(|| seasons.first().copied())
}

/// `2020-08-14T08:00:00.000Z` -> `2020-08-14`; other text is shown unchanged.
pub(crate) fn release_date(text: &str) -> &str {
    match text.split_once('T') {
        Some((date, _))
            if date.len() == 10 && date.bytes().all(|b| b.is_ascii_digit() || b == b'-') =>
        {
            date
        }
        _ => text,
    }
}

pub(crate) fn season_label(season: u32) -> String {
    if season == 0 {
        "Specials".to_owned()
    } else {
        format!("Season {season}")
    }
}

pub(crate) struct EpisodeDelegate {
    rows: Vec<CatalogEpisode>,
    selected: Option<IndexPath>,
}

impl EpisodeDelegate {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            selected: None,
        }
    }

    pub(crate) fn episode(&self, ix: IndexPath) -> Option<&CatalogEpisode> {
        self.rows.get(ix.row)
    }

    pub(crate) fn set(&mut self, rows: Vec<CatalogEpisode>) {
        if self.rows != rows {
            self.rows = rows;
            self.selected = None;
        }
    }
}

impl ListDelegate for EpisodeDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.rows.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let episode = self.rows.get(ix.row)?;
        let muted = cx.theme().muted_foreground;
        let heading = match &episode.name {
            Some(name) => format!("{}. {name}", episode.number),
            None => format!("Episode {}", episode.number),
        };
        let mut body = div().flex().flex_col().gap_1().overflow_hidden().child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(heading),
                )
                .children(episode.released.as_ref().map(|date| {
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(release_date(date).to_owned())
                })),
        );
        if let Some(overview) = &episode.overview {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .line_clamp(2)
                    .child(overview.clone()),
            );
        }
        Some(
            ListItem::new(("episode", ix.row))
                .h(px(ROW_HEIGHT))
                .px_3()
                .selected(self.selected == Some(ix))
                .accessibility_label(format!(
                    "Season {} episode {}",
                    episode.season, episode.number
                ))
                .child(body),
        )
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No episodes in this season.")
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{default_season, release_date, season_episodes, season_label, seasons};
    use kinema_core::catalog::CatalogEpisode;

    fn ep(season: u32, number: u32) -> CatalogEpisode {
        CatalogEpisode {
            season,
            number,
            name: None,
            overview: None,
            released: None,
        }
    }

    #[test]
    fn groups_and_orders_seasons_and_episodes() {
        let all = [ep(2, 2), ep(0, 1), ep(1, 3), ep(1, 1), ep(2, 1)];
        assert_eq!(seasons(&all), [0, 1, 2]);
        assert_eq!(default_season(&seasons(&all)), Some(1));
        let numbers: Vec<u32> = season_episodes(&all, 1).iter().map(|e| e.number).collect();
        assert_eq!(numbers, [1, 3]);
        assert_eq!(default_season(&[0]), Some(0));
        assert_eq!(default_season(&[]), None);
        assert_eq!(release_date("2020-08-14T08:00:00.000Z"), "2020-08-14");
        assert_eq!(release_date("Aug 2020"), "Aug 2020");
        assert_eq!(season_label(0), "Specials");
        assert_eq!(season_label(3), "Season 3");
    }
}
