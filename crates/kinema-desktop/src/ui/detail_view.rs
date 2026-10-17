//! Title detail pane: metadata, series seasons and episodes, source entry point.

use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme, IconName, Sizable as _,
    button::{Button, ButtonVariants},
    list::{List, ListState},
    tab::TabBar,
    tag::Tag,
};
use gpui_kit::*;
use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, TitleDetails},
};

use crate::{
    app_state::Resource,
    shell::Shell,
    ui::{
        episodes::{self, EpisodeDelegate},
        source_picker::SourcesPane,
        strip::{StripModel, status_strip},
    },
};

pub(crate) struct DetailPane {
    pub title: CatalogTitle,
    pub details: Resource<TitleDetails>,
    pub season: Option<u32>,
    pub episodes: Entity<ListState<EpisodeDelegate>>,
    /// `(season, episode)` highlighted in the list.
    pub selected: Option<(u32, u32)>,
    pub sources: Option<SourcesPane>,
    /// Honest one-line outcome of the last action (never a success claim).
    pub notice: Option<String>,
    /// Opened from another metadata provider only to host the source picker.
    pub synthetic: bool,
    pub _subscriptions: Vec<Subscription>,
}

impl DetailPane {
    pub(crate) fn seasons(&self) -> Vec<u32> {
        self.details
            .value()
            .map(|d| episodes::seasons(&d.episodes))
            .unwrap_or_default()
    }

    /// Keeps the season valid and pushes its rows into the list.
    pub(crate) fn sync_episodes(&mut self, cx: &mut Context<Shell>) {
        let seasons = self.seasons();
        if self.season.is_none_or(|s| !seasons.contains(&s)) {
            self.season = episodes::default_season(&seasons);
            self.selected = None;
        }
        let rows = match (self.details.value(), self.season) {
            (Some(details), Some(season)) => episodes::season_episodes(&details.episodes, season),
            _ => Vec::new(),
        };
        self.episodes.update(cx, |list, cx| {
            list.delegate_mut().set(rows);
            cx.notify();
        });
    }
}

pub(crate) fn render(pane: &DetailPane, kind: MediaKind, cx: &mut Context<Shell>) -> Div {
    let muted = cx.theme().muted_foreground;
    let details = pane.details.value();

    // Compact header: no hero, 20 px title, back + refresh on one line.
    let header = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            Button::new("back")
                .icon(IconName::ArrowLeft)
                .ghost()
                .small()
                .tooltip("Back")
                .on_click(cx.listener(move |this, _, _, cx| this.close_top(kind, cx))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(20.0))
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(pane.title.name().to_owned()),
        )
        .child(
            Button::new("refresh-details")
                .icon(IconName::RefreshCw)
                .label("Refresh")
                .outline()
                .small()
                .disabled(pane.details.busy())
                .on_click(cx.listener(move |this, _, _, cx| this.refresh_details(kind, cx))),
        );

    let mut meta = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_size(px(13.0))
        .text_color(muted);
    let title = details.map_or(&pane.title, |details| &details.title);
    meta = meta.child(crate::design::ImdbBadge::new(title.imdb_rating()));
    if let Some(info) = details.and_then(|details| details.release_info.as_ref()) {
        meta = meta.child(info.clone());
    } else if let Some(year) = title.year() {
        meta = meta.child(year.to_string());
    }
    if let Some(minutes) = title.runtime_minutes() {
        meta = meta.child(format!("{minutes} min"));
    }
    if let Some(count) = title.season_count() {
        meta = meta.child(format!(
            "{count} season{}",
            if count == 1 { "" } else { "s" }
        ));
    }
    let genres = details.map_or_else(|| title.genres(), |details| details.genres.as_slice());
    for genre in genres {
        meta = meta.child(Tag::secondary().child(genre.clone()));
    }
    meta = meta.child(
        div()
            .text_size(px(12.0))
            .child(format!("IMDb ID {}", pane.title.id().as_str())),
    );
    let mut head = div().flex().flex_col().gap_2().child(meta);
    let overview = details
        .and_then(|d| d.title.overview())
        .or_else(|| pane.title.overview());
    if let Some(overview) = overview {
        head = head.child(
            div()
                .id("overview")
                .max_h(px(112.0))
                .overflow_y_scroll()
                .text_size(px(13.0))
                .text_color(muted)
                .child(overview.to_owned()),
        );
    }

    // Service state is its own region: a failure never reads as "no data".
    let strip = status_strip(
        "detail-error",
        &StripModel {
            phase: pane.details.phase(),
            error: pane.details.error(),
            has_data: details.is_some(),
            origin: pane.details.origin(),
            fetched_at: pane.details.fetched_at(),
            service: "Cinemeta",
        },
        cx,
    );

    let mut actions = div().flex().items_center().gap_2();
    let body = match kind {
        MediaKind::Movie => {
            actions = actions.child(
                Button::new("find-sources")
                    .icon(IconName::Search)
                    .label("Find sources")
                    .primary()
                    .small()
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.open_sources(kind, window, cx)),
                    ),
            );
            None
        }
        MediaKind::Series => {
            actions = actions.child(
                Button::new("find-sources")
                    .icon(IconName::Search)
                    .label(source_label(pane.selected))
                    .primary()
                    .small()
                    .disabled(pane.selected.is_none())
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.open_sources(kind, window, cx)),
                    ),
            );
            Some(episodes_section(pane, kind, cx))
        }
    };

    let mut root = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap_3()
        .child(header)
        .child(head)
        .child(strip)
        .child(actions);
    if let Some(notice) = &pane.notice {
        root = root.child(
            div()
                .text_size(px(13.0))
                .text_color(muted)
                .child(notice.clone()),
        );
    }
    if let Some(body) = body {
        root = root.child(body);
    }
    root
}

/// Label of the primary action of a series page; the selection is explicit.
pub(crate) fn source_label(selected: Option<(u32, u32)>) -> String {
    match selected {
        Some((s, e)) => format!("Find sources for S{s:02}E{e:02}"),
        None => "Select an episode to find sources".to_owned(),
    }
}

fn episodes_section(pane: &DetailPane, kind: MediaKind, cx: &mut Context<Shell>) -> Div {
    let seasons = pane.seasons();
    let muted = cx.theme().muted_foreground;
    if seasons.is_empty() {
        let text = match (pane.details.value(), pane.details.busy()) {
            (Some(_), _) => "The catalog lists no episodes for this series.",
            (None, true) => "Loading episodes…",
            (None, false) => "Episodes are unavailable until details load.",
        };
        return div().text_size(px(13.0)).text_color(muted).child(text);
    }
    let selected = pane
        .season
        .and_then(|s| seasons.iter().position(|x| *x == s))
        .unwrap_or(0);
    let labels: Vec<String> = seasons.iter().map(|s| episodes::season_label(*s)).collect();
    let for_click = seasons.clone();
    let tabs = TabBar::new("seasons")
        .segmented()
        .small()
        .selected_index(selected)
        .on_click(cx.listener(move |this, ix: &usize, _, cx| {
            if let Some(season) = for_click.get(*ix) {
                this.select_season(kind, *season, cx);
            }
        }))
        .children(labels);
    // Cinemeta lists every episode up front, so loaded rows are the count.
    let loaded = pane.season.map(|s| {
        pane.details
            .value()
            .map_or(0, |d| episodes::season_episodes(&d.episodes, s).len())
    });
    let summary = [
        episodes::seasons_label(&seasons),
        episodes::episode_count_label(loaded, None),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(div().flex_1().min_w_0().overflow_x_hidden().child(tabs))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.0))
                        .text_color(muted)
                        .child(summary),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .child(List::new(&pane.episodes)),
        )
}

#[cfg(test)]
mod tests {
    use super::source_label;

    #[test]
    fn source_action_names_the_explicit_selection() {
        assert_eq!(source_label(None), "Select an episode to find sources");
        assert_eq!(source_label(Some((1, 2))), "Find sources for S01E02");
        assert_eq!(source_label(Some((12, 10))), "Find sources for S12E10");
    }
}
