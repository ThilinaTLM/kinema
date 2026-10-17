//! Virtualized title list for catalog browse and search results.

use gpui_kit::component::{
    ActiveTheme, IndexPath,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::*;
use kinema_core::{MediaKind, catalog::CatalogTitle};
use kinema_desktop::design::MediaCardData;
pub(crate) use kinema_desktop::design::grid::GridMetrics;

pub(crate) const ROW_HEIGHT: f32 = 32.0;

/// How the catalog is laid out; the grid is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewMode {
    #[default]
    Grid,
    List,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nav {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

impl Nav {
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            "home" => Self::Home,
            "end" => Self::End,
            _ => return None,
        })
    }
}

/// Keyboard movement over `items` cells laid out in `cols` columns.
pub(crate) fn step(selected: Option<usize>, nav: Nav, cols: usize, items: usize) -> Option<usize> {
    if items == 0 {
        return None;
    }
    let cols = cols.max(1);
    let Some(current) = selected.filter(|ix| *ix < items) else {
        return Some(if nav == Nav::End { items - 1 } else { 0 });
    };
    Some(match nav {
        Nav::Left => current.saturating_sub(1),
        Nav::Right => (current + 1).min(items - 1),
        Nav::Up if current >= cols => current - cols,
        Nav::Up => current,
        Nav::Down if current / cols < (items - 1) / cols => (current + cols).min(items - 1),
        Nav::Down => current,
        Nav::Home => 0,
        Nav::End => items - 1,
    })
}

/// Resolve painted card identity against the current projection, never its old index.
pub(crate) fn title_index(
    titles: &[CatalogTitle],
    kind: MediaKind,
    id: &kinema_core::catalog::ImdbId,
) -> Option<usize> {
    titles
        .iter()
        .position(|title| title.kind() == kind && title.id() == id)
}

/// Keep the selected identity when filtering, replacing or appending pages.
fn retained_selection(
    previous: &[CatalogTitle],
    selected: Option<usize>,
    next: &[CatalogTitle],
) -> Option<usize> {
    let title = previous.get(selected?)?;
    title_index(next, title.kind(), title.id())
}

/// Grid rows and selection for one media kind. Selection follows stable title
/// identity when the shown titles change.
pub(crate) struct GridState {
    titles: Vec<CatalogTitle>,
    cards: Vec<MediaCardData>,
    card_focus: Vec<FocusHandle>,
    pub(crate) artwork_owner: &'static str,
    pub(crate) artwork_generation: u64,
    selected: Option<usize>,
    empty: Option<SharedString>,
    pub(crate) scroll: UniformListScrollHandle,
    pub(crate) focus: FocusHandle,
}

impl GridState {
    pub(crate) fn new(kind: MediaKind, cx: &mut App) -> Self {
        let owner = match kind {
            MediaKind::Movie => "cinemeta-movies",
            MediaKind::Series => "cinemeta-series",
        };
        Self {
            titles: Vec::new(),
            cards: Vec::new(),
            card_focus: Vec::new(),
            artwork_owner: owner,
            artwork_generation: kinema_desktop::artwork::begin_generation(owner, cx),
            selected: None,
            empty: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }

    pub(crate) fn titles(&self) -> &[CatalogTitle] {
        &self.titles
    }

    pub(crate) fn card_focus(&self, ix: usize) -> &FocusHandle {
        &self.card_focus[ix]
    }

    pub(crate) fn focused_selection(&self, window: &Window, cx: &App) -> Option<usize> {
        self.card_focus
            .iter()
            .position(|focus| focus.contains_focused(window, cx))
            .or(self.selected)
    }

    pub(crate) fn focus_selected(&self, window: &mut Window, cx: &mut App) {
        match self.selected {
            Some(ix) => self.card_focus[ix].focus(window, cx),
            None => self.focus.focus(window, cx),
        }
    }

    pub(crate) fn card(&self, ix: usize) -> MediaCardData {
        self.cards[ix].clone()
    }

    pub(crate) fn invalidate_artwork(&mut self, cx: &mut App) {
        self.artwork_generation = kinema_desktop::artwork::begin_generation(self.artwork_owner, cx);
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub(crate) fn select(&mut self, ix: Option<usize>) {
        self.selected = ix.filter(|ix| *ix < self.titles.len());
    }

    pub(crate) fn empty(&self) -> Option<&SharedString> {
        self.empty.as_ref()
    }

    /// Returns whether anything visible changed.
    pub(crate) fn set(
        &mut self,
        titles: &[CatalogTitle],
        empty: Option<SharedString>,
        cx: &mut App,
    ) -> bool {
        let mut changed = self.empty != empty;
        if self.titles != titles {
            self.selected = retained_selection(&self.titles, self.selected, titles);
            let mut previous: std::collections::HashMap<_, _> = self
                .titles
                .iter()
                .zip(self.card_focus.drain(..))
                .map(|(title, focus)| ((title.kind(), title.id().clone()), focus))
                .collect();
            self.card_focus = titles
                .iter()
                .map(|title| {
                    previous
                        .remove(&(title.kind(), title.id().clone()))
                        .unwrap_or_else(|| cx.focus_handle())
                })
                .collect();
            self.titles = titles.to_vec();
            self.cards = titles.iter().map(card_data).collect();
            self.invalidate_artwork(cx);
            changed = true;
        }
        self.empty = empty;
        changed
    }
}

fn kind_label(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "Movie",
        MediaKind::Series => "Series",
    }
}

/// Prepared once when results change; visible rows only clone shared strings.
fn card_data(title: &CatalogTitle) -> MediaCardData {
    MediaCardData {
        id: format!(
            "cinemeta-{}-{}",
            kind_label(title.kind()),
            title.id().as_str()
        )
        .into(),
        title: title.name().to_owned().into(),
        kind: kind_label(title.kind()).into(),
        year: title.year(),
        overview: title.overview().map(|value| value.to_owned().into()),
        genres: title
            .genres()
            .iter()
            .map(|value| value.clone().into())
            .collect(),
        runtime_minutes: title.runtime_minutes(),
        season_count: title.season_count(),
        imdb_score: title.imdb_rating(),
        has_imdb_id: true,
        poster_url: title.poster_url().map(|value| value.to_owned().into()),
        poster: None,
        poster_status: Default::default(),
    }
}

pub(crate) struct TitleDelegate {
    titles: Vec<CatalogTitle>,
    selected: Option<IndexPath>,
    /// Shown when `titles` is empty; `None` renders nothing (e.g. while loading).
    empty: Option<SharedString>,
}

impl TitleDelegate {
    pub(crate) fn new() -> Self {
        Self {
            titles: Vec::new(),
            selected: None,
            empty: None,
        }
    }

    pub(crate) fn title(&self, ix: IndexPath) -> Option<&CatalogTitle> {
        self.titles.get(ix.row)
    }

    /// Replaces rows; the selection is dropped when the rows changed.
    /// Returns whether anything visible changed.
    pub(crate) fn set(&mut self, titles: &[CatalogTitle], empty: Option<SharedString>) -> bool {
        let mut changed = self.empty != empty;
        if self.titles != titles {
            self.titles = titles.to_vec();
            self.selected = None;
            changed = true;
        }
        self.empty = empty;
        changed
    }
}

impl ListDelegate for TitleDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.titles.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let title = self.titles.get(ix.row)?;
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().items_center().gap_2().overflow_hidden().child(
            div()
                .flex_1()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(title.name().to_owned()),
        );
        body = body.child(
            div().text_size(px(11.0)).text_color(muted).child(
                title
                    .year()
                    .map_or_else(|| "—".to_owned(), |year| year.to_string()),
            ),
        );
        Some(
            ListItem::new(("title", ix.row))
                .h(px(ROW_HEIGHT))
                .px_3()
                .selected(self.selected == Some(ix))
                .accessibility_label(title.name().to_owned())
                .child(body),
        )
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        let message = self.empty.clone();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .children(message)
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
        CatalogTitle, GridMetrics, MediaKind, Nav, card_data, retained_selection, step, title_index,
    };

    fn title(id: &str) -> CatalogTitle {
        CatalogTitle::new(
            kinema_core::catalog::ImdbId::new(id)
                .unwrap_or_else(|error| panic!("invalid fixture identity: {error}")),
            MediaKind::Movie,
            id,
            None,
        )
        .unwrap_or_else(|error| panic!("invalid fixture title: {error}"))
    }

    #[test]
    fn projection_keeps_unknown_facts_and_real_zero_score_distinct() {
        let unknown = card_data(&title("tt0000001"));
        assert_eq!(unknown.imdb_score, None);
        assert_eq!(unknown.year, None);
        assert_eq!(unknown.poster_url, None);
        assert_eq!(
            unknown.poster_status,
            kinema_desktop::design::PosterStatus::Missing
        );
        assert!(unknown.has_imdb_id);
        let rated = title("tt0000001").with_display(kinema_core::catalog::CatalogDisplay {
            imdb_rating: kinema_core::catalog::ImdbRating::new(0.0),
            year: Some(2026),
            ..Default::default()
        });
        let projected = card_data(&rated);
        assert_eq!(projected.imdb_score, Some(0.0));
        assert_eq!(projected.year, Some(2026));
        assert_eq!(projected.id, unknown.id);
    }

    #[test]
    fn painted_identity_resolves_after_refresh_reorders_or_removes_it() {
        let a = title("tt0000001");
        let b = title("tt0000002");
        let painted_id = b.id().clone();
        assert_eq!(
            title_index(&[a.clone(), b.clone()], MediaKind::Movie, &painted_id),
            Some(1)
        );
        assert_eq!(
            title_index(&[b, a.clone()], MediaKind::Movie, &painted_id),
            Some(0)
        );
        assert_eq!(
            title_index(std::slice::from_ref(&a), MediaKind::Movie, &painted_id),
            None
        );
        assert_eq!(
            title_index(std::slice::from_ref(&a), MediaKind::Series, a.id()),
            None
        );
    }

    #[test]
    fn selection_follows_identity_after_filter_and_append() {
        let a = title("tt0000001");
        let b = title("tt0000002");
        let c = title("tt0000003");
        assert_eq!(
            retained_selection(&[a.clone(), b.clone()], Some(1), std::slice::from_ref(&b)),
            Some(0)
        );
        assert_eq!(
            retained_selection(&[a.clone(), b.clone()], Some(1), &[a.clone(), b.clone(), c]),
            Some(1)
        );
        assert_eq!(retained_selection(&[a.clone(), b], Some(1), &[a]), None);
        assert_eq!(retained_selection(&[], Some(4), &[]), None);
    }

    #[test]
    fn resize_changes_columns_not_selected_item() {
        let wide = GridMetrics::new(1200.0, 21);
        let narrow = GridMetrics::new(600.0, 21);
        assert!(wide.columns > narrow.columns);
        assert_eq!(
            step(Some(12), Nav::Down, narrow.columns, 21),
            Some(12 + narrow.columns)
        );
        assert_eq!(narrow.row_of(12), 12 / narrow.columns);
    }

    #[test]
    fn keyboard_steps_stay_inside_the_grid() {
        assert_eq!(step(None, Nav::Down, 4, 0), None);
        assert_eq!(step(None, Nav::Right, 4, 10), Some(0));
        assert_eq!(step(None, Nav::End, 4, 10), Some(9));
        assert_eq!(step(Some(0), Nav::Left, 4, 10), Some(0));
        assert_eq!(step(Some(9), Nav::Right, 4, 10), Some(9));
        assert_eq!(step(Some(1), Nav::Down, 4, 10), Some(5));
        assert_eq!(step(Some(6), Nav::Down, 4, 10), Some(9));
        assert_eq!(step(Some(9), Nav::Down, 4, 10), Some(9));
        assert_eq!(step(Some(2), Nav::Up, 4, 10), Some(2));
        assert_eq!(step(Some(5), Nav::Up, 4, 10), Some(1));
        assert_eq!(Nav::from_key("pagedown"), None);
    }
}
