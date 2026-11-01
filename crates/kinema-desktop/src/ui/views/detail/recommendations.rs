//! Provider-scoped recommendations as a wrapping grid, two rows at a time;
//! never consult TMDB for Cinemeta.
use super::*;
use crate::{
    tmdb::{TmdbPage, TmdbTitle},
    ui::blocks::POSTER_ASPECT,
    ui::blocks::ViewState,
    ui::motion::interaction::focus_ring,
    ui::theme::elevation,
};

/// Rows shown before "Show more", and rows added by each click.
const INITIAL_ROWS: usize = 2;
const ROWS_STEP: usize = 2;
/// TMDB serves no pages beyond this.
const LAST_PAGE: u32 = 500;

/// Similar titles accumulated across catalog pages, plus how many grid rows
/// are revealed. Pure state: the coordinator holds only one page at a time.
#[derive(Debug)]
pub(super) struct SimilarGrid {
    titles: Vec<TmdbTitle>,
    loaded_page: u32,
    total_pages: u32,
    rows: usize,
}
impl Default for SimilarGrid {
    fn default() -> Self {
        Self {
            titles: Vec::new(),
            loaded_page: 0,
            total_pages: 0,
            rows: INITIAL_ROWS,
        }
    }
}
impl SimilarGrid {
    /// Forget loaded titles (refresh); the revealed row count is kept.
    pub(super) fn reset(&mut self) {
        self.titles.clear();
        self.loaded_page = 0;
        self.total_pages = 0;
    }
    /// Fold in a page: page 1 replaces, the page after the last loaded one
    /// appends (deduplicated); stale or repeated pages are ignored.
    pub(super) fn merge(&mut self, page: &TmdbPage) {
        if page.page == 1 {
            self.titles = page.titles.clone();
            self.loaded_page = 1;
        } else if self.loaded_page > 0 && page.page == self.loaded_page + 1 {
            for title in &page.titles {
                if !self.titles.iter().any(|t| t.id == title.id) {
                    self.titles.push(title.clone());
                }
            }
            self.loaded_page = page.page;
        } else {
            return;
        }
        self.total_pages = page.total_pages;
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(super) fn loaded(&self) -> usize {
        self.titles.len()
    }
    pub(super) fn is_empty(&self) -> bool {
        self.titles.is_empty()
    }
    pub(super) fn next_page(&self) -> Option<u32> {
        (self.loaded_page > 0 && self.loaded_page < self.total_pages.min(LAST_PAGE))
            .then_some(self.loaded_page + 1)
    }
    pub(super) fn visible(&self, cols: usize) -> &[TmdbTitle] {
        &self.titles[..self.titles.len().min(self.rows * cols.max(1))]
    }
    /// Something is still hidden or can still be fetched.
    pub(super) fn has_more(&self, cols: usize) -> bool {
        self.titles.len() > self.rows * cols.max(1) || self.next_page().is_some()
    }
    /// Reveal two more rows; returns the page to load when the loaded titles
    /// do not fill them.
    pub(super) fn show_more(&mut self, cols: usize) -> Option<u32> {
        self.rows += ROWS_STEP;
        (self.titles.len() < self.rows * cols.max(1))
            .then(|| self.next_page())
            .flatten()
    }
}

/// Series pages drop the section instead of showing an empty or failed state.
fn series_hides_similar(series: bool, state: ViewState) -> bool {
    series && matches!(state, ViewState::Empty | ViewState::Error)
}

impl Detail {
    /// Similar movies / series. Cinemeta has no such feed, so its titles simply
    /// show no section rather than a permanent apology. Series hide the section
    /// when TMDB has nothing or cannot be reached.
    pub(super) fn recommendations(&mut self, width: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let BrowseTitle::Tmdb(_) = &self.title else {
            return div().into_any_element();
        };
        let (busy, error) = {
            let c = self.coordinator.read(cx);
            (c.similar.busy(), c.similar.error().map(str::to_owned))
        };
        let fit = fit_grid(
            width,
            geometry::SIMILAR_MIN_WIDTH,
            geometry::SIMILAR_MAX_WIDTH,
            geometry::SIMILAR_GAP,
        );
        let cols = fit.cols;
        self.similar_cols = cols;
        let sel = selection(
            !self.similar.is_empty(),
            false,
            error.is_some() && self.similar.is_empty(),
            busy,
        );
        let series = self.is_series();
        if series_hides_similar(series, sel.state) {
            return div().into_any_element();
        }
        let weak = cx.entity().downgrade();
        let mut view = state_view("similar", sel)
            .skeleton(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap(geometry::SIMILAR_GAP)
                    .children((0..cols * 2).map(|_| {
                        Skeleton::block()
                            .w(fit.tile)
                            .h(fit.tile * POSTER_ASPECT)
                            .rounded(radius::MD)
                    })),
            )
            .message(if series {
                "Couldn’t load similar series"
            } else {
                "Couldn’t load similar movies"
            })
            .empty(
                IconName::Film,
                if series {
                    "No similar series"
                } else {
                    "No similar movies"
                },
                Some("TMDB has no recommendations for this title yet.".into()),
            )
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_similar(cx));
            });
        if let Some(error) = &error {
            view = view.hint(error.clone());
        }
        if sel.state == ViewState::Content {
            view = view.content(self.similar_grid(fit, busy, error, cx));
        }
        div()
            .id("detail-similar")
            .w_full()
            .py(s::S16)
            .flex()
            .flex_col()
            .gap(s::S12)
            .child(Section::new(if series {
                "Similar series"
            } else {
                "Similar movies"
            }))
            .child(view)
            .into_any_element()
    }

    fn similar_tile(
        &self,
        title: &TmdbTitle,
        tile_width: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *theme::palette(cx);
        let mut tile = div()
            .id(("similar-title", title.id))
            .debug_selector(|| format!("similar-title-{}", title.id))
            .w(tile_width)
            .h(tile_width * POSTER_ASPECT)
            .flex_none()
            .relative()
            .overflow_hidden()
            .rounded(radius::MD)
            .bg(p.bg_placeholder)
            .border(s::HAIRLINE)
            .border_color(p.poster_edge)
            .shadow(elevation::raised(&p))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .hover(move |el| {
                el.shadow(elevation::card_hover(&p))
                    .border_color(p.line_control)
            })
            .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
            .tooltip(tooltip(title.name.clone()));
        if let Some(image) = self.image(
            title.poster_url(),
            ArtworkKind::Poster,
            &format!("similar:{}", title.id),
            cx,
        ) {
            tile = tile.child(img(image).size_full().object_fit(ObjectFit::Cover));
        } else {
            tile = tile.child(
                div()
                    .p(s::S8)
                    .type_style(t::SMALL)
                    .text_color(p.text_secondary)
                    .child(title.name.clone()),
            );
        }
        let click = title.clone();
        let enter = title.clone();
        tile.on_click(
            cx.listener(move |this, _, _, cx| this.open_recommendation(click.clone(), cx)),
        )
        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
            if event.keystroke.key == "enter" {
                this.open_recommendation(enter.clone(), cx);
                cx.stop_propagation();
            }
        }))
        .into_any_element()
    }

    fn similar_grid(
        &self,
        fit: GridFit,
        busy: bool,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *theme::palette(cx);
        let cols = fit.cols;
        let tiles: Vec<AnyElement> = self
            .similar
            .visible(cols)
            .iter()
            .map(|title| self.similar_tile(title, fit.tile, cx))
            .collect();
        let mut grid = div().w_full().flex().flex_col().gap(s::S12).child(
            div()
                .id("similar-grid")
                .w_full()
                .flex()
                .flex_wrap()
                .gap(geometry::SIMILAR_GAP)
                .children(tiles),
        );
        if let Some(error) = error {
            grid = grid.child(
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(format!("Couldn’t load more: {error}")),
            );
        }
        if self.similar.has_more(cols) {
            grid = grid.child(
                div().flex().child(
                    Button::new("similar-more", "Show more")
                        .icon(IconName::ChevronDown)
                        .busy(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.show_more_similar(cx))),
                ),
            );
        }
        grid.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    use kinema_core::MediaKind;

    fn title(id: u64) -> TmdbTitle {
        TmdbTitle {
            id,
            kind: MediaKind::Movie,
            name: format!("Film {id}"),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        }
    }
    fn page(n: u32, total: u32, ids: std::ops::Range<u64>) -> TmdbPage {
        TmdbPage {
            titles: ids.map(title).collect(),
            page: n,
            total_pages: total,
        }
    }

    #[test]
    fn shows_two_rows_then_reveals_and_pages() {
        let mut grid = SimilarGrid::default();
        grid.merge(&page(1, 3, 0..20));
        // 6 columns: 12 of the 20 titles are visible.
        assert_eq!(grid.visible(6).len(), 12);
        assert!(grid.has_more(6));
        // Four rows need 24 titles but 20 are loaded: page 2 is requested.
        assert_eq!(grid.show_more(6), Some(2));
        grid.merge(&page(2, 3, 20..40));
        assert_eq!(grid.visible(6).len(), 24);
        // Six rows (36) fit the 40 loaded titles: no fetch.
        assert_eq!(grid.show_more(6), None);
        assert_eq!(grid.visible(6).len(), 36);
        assert_eq!(grid.next_page(), Some(3));
    }

    #[test]
    fn pages_merge_in_order_without_duplicates() {
        let mut grid = SimilarGrid::default();
        grid.merge(&page(2, 3, 0..5));
        assert!(grid.is_empty(), "page 2 before page 1 is ignored");
        grid.merge(&page(1, 2, 0..5));
        grid.merge(&page(1, 2, 0..5));
        assert_eq!(grid.titles.len(), 5);
        grid.merge(&page(2, 2, 3..8));
        assert_eq!(grid.titles.len(), 8);
        grid.merge(&page(2, 2, 3..8));
        assert_eq!(grid.titles.len(), 8);
        assert_eq!(grid.next_page(), None);
        assert!(!grid.has_more(6));
    }

    #[test]
    fn reset_keeps_revealed_rows() {
        let mut grid = SimilarGrid::default();
        grid.merge(&page(1, 1, 0..30));
        grid.show_more(6);
        grid.reset();
        assert!(grid.is_empty());
        assert_eq!(grid.rows, INITIAL_ROWS + ROWS_STEP);
    }

    #[test]
    fn series_hide_empty_and_failed_similar_but_movies_keep_their_states() {
        for state in [ViewState::Empty, ViewState::Error] {
            assert!(series_hides_similar(true, state));
            assert!(!series_hides_similar(false, state));
        }
        for state in [ViewState::Loading, ViewState::Content] {
            assert!(!series_hides_similar(true, state));
        }
    }

    #[test]
    fn series_pages_merge_like_movie_pages() {
        let series = |ids: std::ops::Range<u64>| TmdbPage {
            titles: ids
                .map(|id| TmdbTitle {
                    kind: MediaKind::Series,
                    name: format!("Show {id}"),
                    ..title(id)
                })
                .collect(),
            page: 1,
            total_pages: 1,
        };
        let mut grid = SimilarGrid::default();
        grid.merge(&series(0..20));
        assert_eq!(grid.visible(8).len(), 16);
        assert!(grid.visible(8).iter().all(|t| t.kind == MediaKind::Series));
        assert!(grid.has_more(8));
    }
}
