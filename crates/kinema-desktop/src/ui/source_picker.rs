//! Source picker pane for one playback target: indexer choice, sort/filter
//! controls, and a virtualized candidate list. Choosing emits a
//! `PrepareSource` intent through the shell; nothing is started here.

use gpui_kit::base::Disableable as _;
use gpui_kit::component::{
    ActiveTheme, IconName, Sizable as _,
    button::{Button, ButtonVariants},
    list::{List, ListState},
    tab::TabBar,
};
use gpui_kit::*;
use kinema_core::{
    MediaKind,
    sources::{IndexerKind, PlaybackTarget},
};

use crate::{
    app_state::Resource,
    metadata_bridge::SourceSet,
    shell::Shell,
    ui::{
        sources::{self, SortKey, SourceDelegate, SourceOptions},
        strip::{StripModel, status_strip},
    },
};

pub(crate) const INDEXERS: [IndexerKind; 2] = [IndexerKind::Torrentio, IndexerKind::Peerflix];
const SEEDER_STEPS: [u32; 3] = [0, 5, 20];

pub(crate) fn indexer_label(indexer: IndexerKind) -> &'static str {
    match indexer {
        IndexerKind::Torrentio => "Torrentio",
        IndexerKind::Peerflix => "Peerflix",
    }
}

/// Why the candidate list shows no rows. Failure detail stays in the status
/// strip; nothing here inspects error text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EmptyKind {
    Loading,
    Failed,
    NoneReturned,
    FilteredOut,
    NotLoaded,
}

/// `total` is the unfiltered candidate count when a result exists.
pub(crate) fn empty_kind(total: Option<usize>, busy: bool, has_error: bool) -> EmptyKind {
    match total {
        Some(0) => EmptyKind::NoneReturned,
        Some(_) => EmptyKind::FilteredOut,
        None if busy => EmptyKind::Loading,
        None if has_error => EmptyKind::Failed,
        None => EmptyKind::NotLoaded,
    }
}

pub(crate) fn empty_text(kind: EmptyKind) -> &'static str {
    match kind {
        EmptyKind::Loading => "",
        EmptyKind::Failed => "Sources could not be loaded. See the message above.",
        EmptyKind::NoneReturned => "The indexer returned no sources for this title.",
        EmptyKind::FilteredOut => "No sources match the current filters.",
        EmptyKind::NotLoaded => "No sources loaded.",
    }
}

pub(crate) struct SourcesPane {
    pub target: PlaybackTarget,
    /// Human label such as "Dune" or "Severance S01E02".
    pub label: String,
    pub indexer: IndexerKind,
    pub sources: Resource<SourceSet>,
    pub options: SourceOptions,
    pub list: Entity<ListState<SourceDelegate>>,
    pub _subscription: Subscription,
}

impl SourcesPane {
    pub(crate) fn sync(&self, cx: &mut Context<Shell>) {
        let rows = self
            .sources
            .value()
            .map(|set| sources::visible(&set.candidates, &self.options))
            .unwrap_or_default();
        let message = self.empty_message(rows.is_empty());
        self.list.update(cx, |list, cx| {
            list.delegate_mut().set(rows);
            list.delegate_mut().set_empty_message(message);
            cx.notify();
        });
    }

    fn empty_message(&self, no_rows: bool) -> &'static str {
        if !no_rows {
            return "";
        }
        empty_text(empty_kind(
            self.sources.value().map(|set| set.candidates.len()),
            self.sources.busy(),
            self.sources.error().is_some(),
        ))
    }

    pub(crate) fn qualities(&self) -> Vec<String> {
        self.sources
            .value()
            .map(|set| sources::available_qualities(&set.candidates))
            .unwrap_or_default()
    }
}

pub(crate) fn render(
    pane: &SourcesPane,
    kind: MediaKind,
    notice: Option<&str>,
    cx: &mut Context<Shell>,
) -> Div {
    let muted = cx.theme().muted_foreground;
    let set = pane.sources.value();

    let header = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            Button::new("back-sources")
                .icon(IconName::ArrowLeft)
                .label("Back")
                .small()
                .h(px(24.))
                .text_size(px(12.))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| this.close_top(kind, cx))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(format!("Sources · {}", pane.label)),
        );

    let indexer_ix = INDEXERS
        .iter()
        .position(|i| *i == pane.indexer)
        .unwrap_or(0);
    let indexer_tabs = TabBar::new("indexers")
        .segmented()
        .small()
        .h(px(24.))
        .text_size(px(12.))
        .selected_index(indexer_ix)
        .on_click(cx.listener(move |this, ix: &usize, _, cx| {
            if let Some(indexer) = INDEXERS.get(*ix) {
                this.set_indexer(kind, *indexer, cx);
            }
        }))
        .children(INDEXERS.map(|i| indexer_label(i).to_owned()));

    let sort_ix = SortKey::ALL
        .iter()
        .position(|k| *k == pane.options.sort)
        .unwrap_or(0);
    let arrow = if pane.options.descending {
        "↓"
    } else {
        "↑"
    };
    let sort_labels: Vec<String> = SortKey::ALL
        .iter()
        .map(|k| {
            if *k == pane.options.sort {
                format!("{} {arrow}", k.label())
            } else {
                k.label().to_owned()
            }
        })
        .collect();
    let sort_tabs = TabBar::new("sort")
        .segmented()
        .small()
        .h(px(24.))
        .text_size(px(12.))
        .selected_index(sort_ix)
        .on_click(cx.listener(move |this, ix: &usize, _, cx| {
            if let Some(key) = SortKey::ALL.get(*ix) {
                this.sort_sources(kind, *key, cx);
            }
        }))
        .children(sort_labels);

    let qualities = pane.qualities();
    let mut quality_labels = vec!["All qualities".to_owned()];
    quality_labels.extend(qualities.iter().cloned());
    let quality_ix = pane
        .options
        .quality
        .as_ref()
        .and_then(|q| qualities.iter().position(|x| x == q))
        .map_or(0, |i| i + 1);
    let quality_tabs = TabBar::new("quality")
        .segmented()
        .small()
        .h(px(24.))
        .text_size(px(12.))
        .selected_index(quality_ix)
        .on_click(cx.listener(move |this, ix: &usize, _, cx| {
            let choice = ix.checked_sub(1).and_then(|i| qualities.get(i).cloned());
            this.filter_quality(kind, choice, cx);
        }))
        .children(quality_labels);

    let seeder_ix = SEEDER_STEPS
        .iter()
        .position(|s| *s == pane.options.min_seeders)
        .unwrap_or(0);
    let seeder_tabs = TabBar::new("seeders")
        .segmented()
        .small()
        .h(px(24.))
        .text_size(px(12.))
        .selected_index(seeder_ix)
        .on_click(cx.listener(move |this, ix: &usize, _, cx| {
            if let Some(min) = SEEDER_STEPS.get(*ix) {
                this.filter_seeders(kind, *min, cx);
            }
        }))
        .children(SEEDER_STEPS.map(|s| {
            if s == 0 {
                "Any seeders".to_owned()
            } else {
                format!("{s}+ seeders")
            }
        }));

    let controls = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_size(px(12.))
        .child(indexer_tabs)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_size(px(12.)).text_color(muted).child("Sort"))
                .child(sort_tabs),
        )
        .child(quality_tabs)
        .child(seeder_tabs)
        .child(
            Button::new("refresh-sources")
                .icon(IconName::RefreshCw)
                .label("Refresh")
                .small()
                .h(px(24.))
                .text_size(px(12.))
                .outline()
                .disabled(pane.sources.busy())
                .on_click(cx.listener(move |this, _, _, cx| this.refresh_sources(kind, cx))),
        );

    let strip = status_strip(
        "sources-error",
        &StripModel {
            phase: pane.sources.phase(),
            error: pane.sources.error(),
            has_data: set.is_some(),
            origin: pane.sources.origin(),
            fetched_at: pane.sources.fetched_at(),
            service: indexer_label(pane.indexer),
        },
        cx,
    );

    let visible = pane.list.read(cx).delegate().len();
    let total = set.map_or(0, |s| s.candidates.len());
    let mut summary = if set.is_some() {
        format!("{visible} of {total} sources shown")
    } else {
        match empty_kind(None, pane.sources.busy(), pane.sources.error().is_some()) {
            EmptyKind::Loading => "Loading sources…",
            EmptyKind::Failed => "Sources unavailable",
            _ => "Sources not loaded",
        }
        .to_owned()
    };
    if let Some(set) = set
        && set.omitted > 0
    {
        summary.push_str(&format!(
            " · {} more omitted by the provider cap",
            set.omitted
        ));
    }
    let can_use = pane.list.read(cx).delegate().selected_candidate().is_some();

    div()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .text_size(px(13.))
        .flex()
        .flex_col()
        .gap_2()
        .child(header)
        .child(controls)
        .child(strip)
        .children(notice.map(|text| {
            div()
                .text_size(px(12.))
                .text_color(muted)
                .child(text.to_owned())
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_size(px(12.)).text_color(muted).child(summary))
                .child(
                    Button::new("use-source")
                        .icon(IconName::Play)
                        .label("Use selected source")
                        .h(px(28.))
                        .primary()
                        .disabled(!can_use)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.prepare_selected(kind, cx)),
                        ),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_md()
                .overflow_hidden()
                .child(List::new(&pane.list)),
        )
}

#[cfg(test)]
mod tests {
    use super::{EmptyKind, empty_kind, empty_text};
    use std::prelude::v1::test;

    #[test]
    fn distinguishes_empty_states() {
        assert_eq!(empty_kind(Some(0), false, false), EmptyKind::NoneReturned);
        assert_eq!(empty_kind(Some(3), false, false), EmptyKind::FilteredOut);
        assert_eq!(empty_kind(None, true, false), EmptyKind::Loading);
        assert_eq!(empty_kind(None, false, true), EmptyKind::Failed);
        assert_eq!(empty_kind(None, false, false), EmptyKind::NotLoaded);
    }

    #[test]
    fn stale_data_wins_over_refresh_failure_or_busy() {
        assert_eq!(empty_kind(Some(0), true, true), EmptyKind::NoneReturned);
        assert_eq!(empty_kind(Some(2), false, true), EmptyKind::FilteredOut);
    }

    #[test]
    fn failure_is_not_worded_as_empty_result() {
        let failed = empty_text(EmptyKind::Failed);
        assert!(failed.contains("could not be loaded"));
        assert_ne!(failed, empty_text(EmptyKind::NoneReturned));
        assert_ne!(
            empty_text(EmptyKind::FilteredOut),
            empty_text(EmptyKind::NoneReturned)
        );
        assert!(empty_text(EmptyKind::Loading).is_empty());
    }
}
