//! Series episodes: pure season grouping and a virtualized episode list.

use gpui_kit::component::{
    ActiveTheme, IndexPath,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::*;
use kinema_core::catalog::CatalogEpisode;

/// Compact one-line row (Graphite design system: episode rows are 32 px).
pub(crate) const ROW_HEIGHT: f32 = 32.0;

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

/// "3 seasons" / "2 seasons + specials". `None` while no season is known, so an
/// unknown count is never shown as zero.
pub(crate) fn seasons_label(seasons: &[u32]) -> Option<String> {
    let regular = seasons.iter().filter(|s| **s > 0).count();
    let specials = seasons.contains(&0);
    match (regular, specials) {
        (0, false) => None,
        (0, true) => Some("Specials".to_owned()),
        (n, specials) => {
            let mut text = format!("{n} season{}", if n == 1 { "" } else { "s" });
            if specials {
                text.push_str(" + specials");
            }
            Some(text)
        }
    }
}

/// Episode count for the visible season: the loaded rows win over a count the
/// provider reported; a reported zero means "not reported", not "empty".
pub(crate) fn episode_count_label(loaded: Option<usize>, reported: Option<u32>) -> Option<String> {
    let count = loaded.or_else(|| reported.filter(|n| *n > 0).map(|n| n as usize))?;
    Some(format!(
        "{count} episode{}",
        if count == 1 { "" } else { "s" }
    ))
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
        let theme = cx.theme();
        let (muted, accent) = (theme.muted_foreground, theme.primary);
        let selected = self.selected == Some(ix);
        let number = div()
            .w(px(28.0))
            .flex_none()
            .text_color(muted)
            .child(episode.number.to_string());
        let title = match &episode.name {
            Some(name) => name.clone(),
            None => format!("Episode {}", episode.number),
        };
        let row = div()
            .size_full()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(13.0))
            .child(
                // Selection marker: visible without relying on colour alone.
                div()
                    .w(px(2.0))
                    .h(px(16.0))
                    .flex_none()
                    .rounded_full()
                    .bg(if selected {
                        accent
                    } else {
                        gpui_kit::transparent_black()
                    }),
            )
            .child(number)
            .child(div().flex_1().min_w_0().truncate().child(title))
            .children(episode.released.as_ref().map(|date| {
                div()
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(muted)
                    .child(release_date(date).to_owned())
            }));
        Some(
            ListItem::new(("episode", ix.row))
                .h(px(ROW_HEIGHT))
                .px_2()
                .selected(selected)
                .accessibility_label(format!(
                    "Season {} episode {}",
                    episode.season, episode.number
                ))
                .child(row),
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
            .text_size(px(13.0))
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
    use super::{
        default_season, episode_count_label, release_date, season_episodes, season_label, seasons,
        seasons_label,
    };
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

    #[test]
    fn counts_are_never_fabricated() {
        assert_eq!(seasons_label(&[]), None);
        assert_eq!(seasons_label(&[0]).as_deref(), Some("Specials"));
        assert_eq!(seasons_label(&[1]).as_deref(), Some("1 season"));
        assert_eq!(
            seasons_label(&[0, 1, 2]).as_deref(),
            Some("2 seasons + specials")
        );
        assert_eq!(episode_count_label(None, None), None);
        assert_eq!(episode_count_label(None, Some(0)), None);
        assert_eq!(
            episode_count_label(None, Some(1)).as_deref(),
            Some("1 episode")
        );
        // Loaded rows win; a genuinely empty loaded season is a real zero.
        assert_eq!(
            episode_count_label(Some(0), Some(8)).as_deref(),
            Some("0 episodes")
        );
        assert_eq!(
            episode_count_label(Some(10), Some(8)).as_deref(),
            Some("10 episodes")
        );
    }
}
