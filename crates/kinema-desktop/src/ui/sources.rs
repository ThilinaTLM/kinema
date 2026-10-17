//! Source picker: pure sort/filter model and a virtualized candidate list.
//!
//! Candidates are display-only snapshots (no signed URLs or secrets). Choosing a
//! row only produces a `PrepareSource` intent; nothing here claims success.

use std::cmp::Ordering;

use gpui_kit::component::{
    ActiveTheme, IndexPath,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::*;
use kinema_core::sources::SourceCandidate;

pub(crate) const ROW_HEIGHT: f32 = 32.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SortKey {
    Seeders,
    Size,
    Quality,
    Name,
}

impl SortKey {
    pub(crate) const ALL: [Self; 4] = [Self::Seeders, Self::Size, Self::Quality, Self::Name];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Seeders => "Seeders",
            Self::Size => "Size",
            Self::Quality => "Quality",
            Self::Name => "Name",
        }
    }

    /// Largest/best first for numeric keys, A–Z for names.
    fn default_descending(self) -> bool {
        self != Self::Name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceOptions {
    pub sort: SortKey,
    pub descending: bool,
    /// Exact quality label from the candidates, or all when `None`.
    pub quality: Option<String>,
    /// Hide candidates below this seeder count (unknown counts are hidden too).
    pub min_seeders: u32,
}

impl Default for SourceOptions {
    fn default() -> Self {
        Self {
            sort: SortKey::Seeders,
            descending: true,
            quality: None,
            min_seeders: 0,
        }
    }
}

impl SourceOptions {
    /// Selecting the active key flips direction; a new key starts at its default.
    pub(crate) fn select_sort(&mut self, key: SortKey) {
        if self.sort == key {
            self.descending = !self.descending;
        } else {
            self.sort = key;
            self.descending = key.default_descending();
        }
    }
}

/// Resolution rank for ordering; unknown labels rank lowest.
pub(crate) fn quality_rank(label: &str) -> u32 {
    let lower = label.to_ascii_lowercase();
    [
        (["2160", "4k", "uhd"].as_slice(), 2160),
        (["1440"].as_slice(), 1440),
        (["1080"].as_slice(), 1080),
        (["720"].as_slice(), 720),
        (["576"].as_slice(), 576),
        (["480"].as_slice(), 480),
        (["360"].as_slice(), 360),
    ]
    .iter()
    .find(|(needles, _)| needles.iter().any(|n| lower.contains(n)))
    .map_or(0, |(_, rank)| *rank)
}

/// Distinct quality labels, best first, for the filter control.
pub(crate) fn available_qualities(candidates: &[SourceCandidate]) -> Vec<String> {
    let mut labels: Vec<String> = candidates
        .iter()
        .filter_map(|c| c.quality.as_ref())
        .map(|q| q.trim().to_owned())
        .filter(|q| !q.is_empty())
        .collect();
    labels.sort_by(|a, b| quality_rank(b).cmp(&quality_rank(a)).then_with(|| a.cmp(b)));
    labels.dedup();
    labels
}

/// Missing values sort last in either direction.
fn compare_optional<T: Ord>(a: Option<T>, b: Option<T>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            if descending {
                b.cmp(&a)
            } else {
                a.cmp(&b)
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Filtered and sorted copies of the candidates. The sort is stable and ties
/// fall back to name then info hash so ordering never flickers between renders.
pub(crate) fn visible(
    candidates: &[SourceCandidate],
    options: &SourceOptions,
) -> Vec<SourceCandidate> {
    let mut rows: Vec<SourceCandidate> = candidates
        .iter()
        .filter(|c| {
            options
                .quality
                .as_ref()
                .is_none_or(|wanted| c.quality.as_deref().map(str::trim) == Some(wanted.as_str()))
        })
        .filter(|c| options.min_seeders == 0 || c.seeders.is_some_and(|s| s >= options.min_seeders))
        .cloned()
        .collect();
    let desc = options.descending;
    rows.sort_by(|a, b| {
        let primary = match options.sort {
            SortKey::Seeders => compare_optional(a.seeders, b.seeders, desc),
            SortKey::Size => compare_optional(a.size, b.size, desc),
            SortKey::Quality => compare_optional(
                a.quality.as_deref().map(quality_rank),
                b.quality.as_deref().map(quality_rank),
                desc,
            ),
            SortKey::Name => {
                let ordering = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if desc { ordering.reverse() } else { ordering }
            }
        };
        primary
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.torrent.hash.as_str().cmp(b.torrent.hash.as_str()))
    });
    rows
}

pub(crate) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Fixed columns keep known and unknown facts aligned without inventing values.
fn source_facts(source: &SourceCandidate) -> [String; 3] {
    [
        source
            .quality
            .as_deref()
            .map(str::trim)
            .filter(|quality| !quality.is_empty())
            .unwrap_or("Quality —")
            .to_owned(),
        source.size.map_or_else(|| "Size —".to_owned(), format_size),
        source
            .seeders
            .map_or_else(|| "Seeds —".to_owned(), |count| format!("{count} ↑")),
    ]
}

pub(crate) struct SourceDelegate {
    rows: Vec<SourceCandidate>,
    selected: Option<IndexPath>,
    empty_message: SharedString,
}

impl SourceDelegate {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            selected: None,
            empty_message: SharedString::new_static(""),
        }
    }

    /// Text shown when no row is visible; the pane distinguishes loading,
    /// service failure, an empty provider result, and filters hiding everything.
    pub(crate) fn set_empty_message(&mut self, message: impl Into<SharedString>) {
        self.empty_message = message.into();
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn selected_candidate(&self) -> Option<&SourceCandidate> {
        self.selected.and_then(|ix| self.rows.get(ix.row))
    }

    pub(crate) fn set(&mut self, rows: Vec<SourceCandidate>) {
        if self.rows != rows {
            self.rows = rows;
            self.selected = None;
        }
    }
}

impl ListDelegate for SourceDelegate {
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
        let source = self.rows.get(ix.row)?;
        let muted = cx.theme().muted_foreground;
        let [quality, size, seeders] = source_facts(source);
        let accessible = format!("{} · {quality} · {size} · {seeders}", source.name);
        let facts = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_3()
            .text_size(px(11.))
            .text_color(muted)
            .child(
                div()
                    .w(px(60.))
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(quality),
            )
            .child(div().w(px(72.)).truncate().child(size))
            .child(div().w(px(60.)).truncate().child(seeders));
        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap_3()
            .overflow_hidden()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .truncate()
                    .child(source.name.clone()),
            )
            .child(facts);
        Some(
            ListItem::new(("source", ix.row))
                .h(px(ROW_HEIGHT))
                .px_2()
                .selected(self.selected == Some(ix))
                .accessibility_label(accessible)
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
            .text_size(px(12.))
            .text_color(cx.theme().muted_foreground)
            .child(self.empty_message.clone())
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
#[allow(clippy::expect_used)]
mod tests {
    use kinema_core::sources::{InfoHash, TorrentSource};
    use std::prelude::v1::test;

    use super::{SortKey, SourceOptions, available_qualities, format_size, source_facts, visible};
    use kinema_core::sources::SourceCandidate;

    fn cand(
        n: u8,
        quality: Option<&str>,
        size: Option<u64>,
        seeders: Option<u32>,
    ) -> SourceCandidate {
        SourceCandidate {
            torrent: TorrentSource {
                hash: InfoHash::new(format!("{n:040x}")).expect("hash"),
                file_index: None,
                filename: None,
            },
            name: format!("src{n}"),
            quality: quality.map(str::to_owned),
            size,
            seeders,
        }
    }

    fn names(rows: &[SourceCandidate]) -> Vec<&str> {
        rows.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn sorts_by_seeders_with_unknown_last_in_both_directions() {
        let all = [
            cand(1, None, None, None),
            cand(2, None, None, Some(5)),
            cand(3, None, None, Some(50)),
        ];
        let mut options = SourceOptions::default();
        assert_eq!(names(&visible(&all, &options)), ["src3", "src2", "src1"]);
        options.select_sort(SortKey::Seeders);
        assert!(!options.descending);
        assert_eq!(names(&visible(&all, &options)), ["src2", "src3", "src1"]);
    }

    #[test]
    fn quality_rank_orders_resolutions_and_filters_exactly() {
        let all = [
            cand(1, Some("720p"), Some(1), Some(1)),
            cand(2, Some("4K"), Some(2), Some(1)),
            cand(3, Some("1080p"), Some(3), Some(1)),
            cand(4, Some("1080p"), Some(4), None),
        ];
        assert_eq!(available_qualities(&all), ["4K", "1080p", "720p"]);
        let mut options = SourceOptions {
            sort: SortKey::Quality,
            ..SourceOptions::default()
        };
        assert_eq!(names(&visible(&all, &options))[0], "src2");
        options.quality = Some("1080p".into());
        assert_eq!(visible(&all, &options).len(), 2);
        options.min_seeders = 1;
        assert_eq!(names(&visible(&all, &options)), ["src3"]);
    }

    #[test]
    fn size_sort_and_switching_key_resets_direction() {
        let all = [cand(1, None, Some(10), None), cand(2, None, Some(30), None)];
        let mut options = SourceOptions::default();
        options.select_sort(SortKey::Size);
        assert!(options.descending);
        assert_eq!(names(&visible(&all, &options)), ["src2", "src1"]);
        options.select_sort(SortKey::Name);
        assert!(!options.descending);
        assert_eq!(names(&visible(&all, &options)), ["src1", "src2"]);
    }

    #[test]
    fn source_facts_preserve_zero_and_explicit_unknowns() {
        assert_eq!(
            source_facts(&cand(1, None, None, None)),
            ["Quality —", "Size —", "Seeds —"]
        );
        assert_eq!(
            source_facts(&cand(1, Some(" 1080p "), Some(0), Some(0))),
            ["1080p", "0 B", "0 ↑"]
        );
        assert_eq!(
            source_facts(&cand(1, Some("  "), None, None))[0],
            "Quality —"
        );
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }
}
