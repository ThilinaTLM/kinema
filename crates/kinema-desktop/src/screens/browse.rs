//! Virtualized poster browse with stable loaded cards and static loading rows.
mod layout;
use super::coordinator::{AppCoordinator, BrowseTitle};
use crate::{
    artwork::{self, ArtworkState},
    widgets::{
        button::Button,
        menu::MenuItem,
        notice::{Notice, NoticeKind},
        poster_card::{CardOpened, PosterCard},
        skeleton_card::SkeletonCard,
        text::TypeExt,
        tokens::{self, space as s, typeface as t},
    },
};
use gpui::{prelude::*, *};
use kinema_core::MediaKind;
use layout::LoadingLayout;
use std::{cmp::Ordering, rc::Rc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sort {
    #[default]
    Provider,
    Title,
    Year,
    Rating,
}
impl Sort {
    pub fn options() -> Vec<(Self, SharedString)> {
        vec![
            (Self::Provider, "Provider order (loaded)".into()),
            (Self::Title, "Title (loaded)".into()),
            (Self::Year, "Newest (loaded)".into()),
            (Self::Rating, "IMDb rating (loaded)".into()),
        ]
    }
}
pub(crate) fn rating_order(a: Option<f32>, b: Option<f32>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.total_cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}
pub(crate) fn columns(width: f32, preferred: f32) -> (usize, f32) {
    let available = (width - 2. * f32::from(s::GRID_INSET)).max(0.);
    let gap = f32::from(s::GRID_GAP);
    let count = ((available + gap) / (preferred + gap)).floor().max(2.) as usize;
    (
        count,
        ((available - (count - 1) as f32 * gap) / count as f32).max(1.),
    )
}
fn navigate(
    current: Option<usize>,
    key: &str,
    cols: usize,
    count: usize,
    page_rows: usize,
) -> Option<usize> {
    if count == 0
        || !matches!(
            key,
            "left"
                | "right"
                | "up"
                | "down"
                | "home"
                | "end"
                | "loaded-home"
                | "loaded-end"
                | "pageup"
                | "pagedown"
        )
    {
        return None;
    }
    let Some(i) = current.filter(|i| *i < count) else {
        return Some(match key {
            "loaded-end" => count - 1,
            "end" => cols.max(1).min(count) - 1,
            _ => 0,
        });
    };
    let cols = cols.max(1);
    Some(match key {
        "left" => i.saturating_sub(1),
        "right" => (i + 1).min(count - 1),
        "up" => i.saturating_sub(cols).max(if i < cols { i } else { 0 }),
        "down" => {
            if i / cols < (count - 1) / cols {
                (i + cols).min(count - 1)
            } else {
                i
            }
        }
        "home" => (i / cols) * cols,
        "end" => (((i / cols) + 1) * cols - 1).min(count - 1),
        "loaded-home" => 0,
        "loaded-end" => count - 1,
        "pageup" => i.saturating_sub(cols * page_rows.max(1)),
        "pagedown" => (i + cols * page_rows.max(1)).min(count - 1),
        _ => return None,
    })
}
#[derive(Clone)]
pub(crate) struct OpenTitle(pub BrowseTitle);
impl EventEmitter<OpenTitle> for Browse {}
struct Cell {
    title: BrowseTitle,
    card: Entity<PosterCard>,
    _subscription: Subscription,
}
pub(crate) struct Browse {
    coordinator: Entity<AppCoordinator>,
    cells: Vec<Cell>,
    pub sort: Sort,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    selected: Option<String>,
    cols: usize,
    page_request_generation: Option<u64>,
    subject: (MediaKind, String),
    artwork_generation: u64,
    _observation: Subscription,
}
impl Browse {
    pub fn new(coordinator: Entity<AppCoordinator>, cx: &mut Context<Self>) -> Self {
        let observation = cx.observe(&coordinator, |_, _, cx| cx.notify());
        Self {
            coordinator,
            cells: Vec::new(),
            sort: Sort::default(),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            selected: None,
            cols: 2,
            page_request_generation: None,
            subject: (MediaKind::Movie, String::new()),
            artwork_generation: artwork::begin_generation("v3-browse", cx),
            _observation: observation,
        }
    }
    pub fn restore_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(cell) = self
            .cells
            .iter()
            .find(|c| Some(c.title.key()) == self.selected)
        {
            cell.card.update(cx, |card, cx| card.focus(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
    }
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.coordinator.read(cx);
        let subject = (state.kind, state.query.clone());
        let mut titles = state.titles.clone();
        let old_top = self.scroll.0.borrow().base_handle.logical_scroll_top().0 * self.cols;
        let anchor = (self.subject == subject)
            .then(|| self.cells.get(old_top).map(|c| c.title.key()))
            .flatten();
        if self.subject != subject {
            self.subject = subject;
            self.selected = None;
            self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
            self.artwork_generation = artwork::begin_generation("v3-browse", cx);
        }

        match self.sort {
            Sort::Provider => {}
            Sort::Title => titles.sort_by_key(|a| a.name().to_lowercase()),
            Sort::Year => titles.sort_by_key(|a| std::cmp::Reverse(a.year())),
            Sort::Rating => titles.sort_by(|a, b| rating_order(a.rating(), b.rating())),
        }
        if self.cells.iter().map(|c| &c.title).eq(titles.iter()) {
            return;
        }
        let mut old = std::mem::take(&mut self.cells);
        self.cells = titles
            .into_iter()
            .enumerate()
            .map(|(index, title)| {
                if let Some(i) = old.iter().position(|c| c.title == title) {
                    return old.remove(i);
                }
                let card = old
                    .iter()
                    .position(|c| c.title.key() == title.key())
                    .map(|i| old.remove(i).card)
                    .unwrap_or_else(|| {
                        cx.new(|cx| PosterCard::new(index, title.name().to_owned(), window, cx))
                    });
                card.update(cx, |card, cx| {
                    card.set_title(title.name().to_owned());
                    card.set_rating(title.rating());
                    card.year = title
                        .year()
                        .map(|y| y.to_string())
                        .unwrap_or_default()
                        .into();
                    card.series = title.kind() == MediaKind::Series;
                    let mut facts = Vec::new();
                    if let Some(year) = title.year() {
                        facts.push(year.to_string());
                    }
                    if card.series {
                        if let Some(n) = title.seasons() {
                            facts.push(format!("{n} seasons"));
                        }
                    } else if let Some(n) = title.runtime() {
                        facts.push(format!("{n} min"));
                    }
                    card.facts = facts.join(" · ").into();
                    card.genres = title
                        .genres()
                        .iter()
                        .take(2)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" · ")
                        .into();
                    card.synopsis = title.overview().unwrap_or_default().to_owned().into();
                    cx.notify();
                });
                let title_for_event = title.clone();
                let subscription = cx.subscribe(&card, move |this, _, _: &CardOpened, cx| {
                    this.selected = Some(title_for_event.key());
                    cx.emit(OpenTitle(title_for_event.clone()));
                });
                let weak = cx.entity().downgrade();
                let open = title.clone();
                let mut items = vec![menu_item("Open details", move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.selected = Some(open.key());
                        cx.emit(OpenTitle(open.clone()));
                    });
                })];
                if title.kind() == MediaKind::Movie && title.imdb().is_some() {
                    let weak = cx.entity().downgrade();
                    let movie = title.clone();
                    items.push(menu_item("Find streams", move |_, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.selected = Some(movie.key());
                            if let Some(id) = movie.imdb() {
                                this.coordinator.update(cx, |c, cx| {
                                    if let Some(settings) = &c.settings {
                                        c.sources(
                                            settings.indexer,
                                            kinema_core::sources::PlaybackTarget::Movie(id.clone()),
                                            cx,
                                        );
                                    }
                                });
                            }
                            cx.emit(OpenTitle(movie.clone()));
                        });
                    }));
                }
                let copy = title.name().to_owned();
                items.push(menu_item("Copy title", move |_, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                }));
                if let Some(id) = title.imdb() {
                    let id = id.as_str().to_owned();
                    let external = id.clone();
                    items.push(menu_item("Copy IMDb ID", move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(id.clone()))
                    }));
                    items.push(menu_item("Open on IMDb", move |_, cx| {
                        cx.open_url(&format!("https://www.imdb.com/title/{external}/"))
                    }));
                }
                let coordinator = self.coordinator.clone();
                let refresh_title = title.clone();
                items.push(menu_item("Refresh metadata", move |_, cx| {
                    coordinator.update(cx, |c, cx| c.title_details(&refresh_title, true, cx))
                }));
                card.update(cx, |card, _| card.menu = items);
                Cell {
                    title,
                    card,
                    _subscription: subscription,
                }
            })
            .collect();
        if let Some(key) = anchor
            && let Some(index) = self.cells.iter().position(|c| c.title.key() == key)
            && index != old_top
        {
            self.scroll
                .scroll_to_item_strict(index / self.cols, ScrollStrategy::Top);
        }
    }
}
pub(crate) fn menu_item(
    label: &'static str,
    handler: impl Fn(&mut Window, &mut App) + 'static,
) -> MenuItem {
    MenuItem::Action {
        label: label.into(),
        shortcut: None,
        icon: None,
        disabled_reason: None,
        checked: false,
        handler: Rc::new(handler),
    }
}
impl Render for Browse {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(window, cx);
        let p = *tokens::palette(cx);
        let preferred = self
            .coordinator
            .read(cx)
            .settings
            .as_ref()
            .map(|s| s.poster_size.preferred_width())
            .unwrap_or(172.);
        let (cols, width) = columns(f32::from(window.viewport_size().width), preferred);
        if cols != self.cols {
            let anchor = self.scroll.0.borrow().base_handle.logical_scroll_top().0 * self.cols;
            self.cols = cols;
            self.scroll
                .scroll_to_item_strict(anchor / cols, ScrollStrategy::Top);
        }
        let row_height = width * 1.5 + f32::from(s::GRID_GAP);
        let state = self.coordinator.read(cx);
        let busy = state.collection.busy();
        let append_in_flight = state.append_in_flight();
        let generation = state.collection.generation();
        let error = state.collection.error().map(str::to_owned);
        let query = state.query.clone();
        let missing = state
            .settings
            .as_ref()
            .is_some_and(|s| s.metadata == crate::settings::MetadataProvider::Tmdb)
            && matches!(
                state.tmdb_credential,
                Some(crate::app::CredentialState::Missing)
            );
        let count = self.cells.len();
        let content = if count == 0 && !busy {
            let text = if missing {
                "TMDB needs an API token.".into()
            } else if let Some(e) = &error {
                e.clone()
            } else if query.is_empty() {
                "No titles in this collection.".into()
            } else {
                format!("No titles match “{query}”.")
            };
            let coordinator = self.coordinator.clone();
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(s::S12)
                .child(div().w(px(420.)).max_w_full().child(Notice::new(
                    if missing {
                        NoticeKind::Warning
                    } else if error.is_some() {
                        NoticeKind::Error
                    } else {
                        NoticeKind::Empty
                    },
                    text,
                )))
                .child(
                    Button::new(
                        "collection-action",
                        if missing {
                            "Open Settings…"
                        } else if error.is_some() {
                            "Retry"
                        } else {
                            "Clear search"
                        },
                    )
                    .on_click(move |_, _, cx| {
                        coordinator.update(cx, |c, cx| {
                            if missing {
                                c.open_settings(cx);
                                return;
                            }
                            c.load(
                                c.kind,
                                if error.is_some() {
                                    c.query.clone()
                                } else {
                                    String::new()
                                },
                                true,
                                cx,
                            )
                        })
                    }),
                )
                .into_any_element()
        } else {
            let layout = LoadingLayout::new(
                count,
                cols,
                (f32::from(window.viewport_size().height) / row_height).ceil() as usize + 2,
                busy,
                append_in_flight,
            );
            let rows = layout.rows();
            let failed = error.is_some();
            uniform_list(
                "v3-poster-grid",
                rows,
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    // GPUI also invokes the renderer with one measurement row.
                    // Only the multi-row viewport pass may evict offscreen art.
                    for (i, cell) in this.cells.iter().enumerate() {
                        if range.len() > 1 && (i < range.start * cols || i >= range.end * cols) {
                            cell.card.update(cx, |card, _| card.set_art(None));
                        }
                    }
                    if range.len() > 1 {
                        let keys = (range.start..range.end)
                            .chain(range.end..range.end.saturating_add(2).min(rows))
                            .chain(range.start.saturating_sub(2)..range.start)
                            .flat_map(|row| row * cols..(row + 1) * cols)
                            .filter_map(|index| this.cells.get(index))
                            .filter_map(|cell| cell.title.poster_url())
                            .filter_map(|url| {
                                artwork::ArtworkKey::new(artwork::ArtworkKind::Poster, url)
                            })
                            .collect();
                        artwork::request_window("v3-browse", this.artwork_generation, keys, cx);
                    }
                    if layout.should_request(
                        &range,
                        busy,
                        failed,
                        generation,
                        this.page_request_generation,
                    ) {
                        // Renderers also run during measurement. Defer mutation until
                        // after layout, and admit at most one request per generation.
                        this.page_request_generation = Some(generation);
                        let coordinator = this.coordinator.clone();
                        cx.defer(move |cx| {
                            coordinator.update(cx, |c, cx| {
                                if c.collection.generation() == generation {
                                    c.next_page(cx);
                                }
                            });
                        });
                    }
                    range
                        .map(|row| {
                            let mut cards = Vec::new();
                            if row >= layout.title_rows {
                                return div().h(px(row_height)).flex().gap(s::GRID_GAP).children(
                                    (0..layout.skeleton_count().min(cols))
                                        .map(|_| SkeletonCard::new(width).into_any_element()),
                                );
                            }
                            for index in row * cols..((row + 1) * cols).min(count) {
                                let cell = &this.cells[index];
                                let image = cell.title.poster_url().and_then(|url| {
                                    match artwork::lookup_or_request(
                                        "v3-browse",
                                        this.artwork_generation,
                                        url,
                                        cx,
                                    ) {
                                        ArtworkState::Ready(image) => Some(image),
                                        _ => None,
                                    }
                                });
                                #[cfg(feature = "ui-probe")]
                                let image = this
                                    .coordinator
                                    .read(cx)
                                    .fixture_art
                                    .get(&cell.title.key())
                                    .cloned()
                                    .or(image);
                                cell.card.update(cx, |card, _| {
                                    card.width = width;
                                    card.set_art(image);
                                });
                                cards.push(cell.card.clone().into_any_element());
                            }
                            div()
                                .h(px(row_height))
                                .flex()
                                .gap(s::GRID_GAP)
                                .children(cards)
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .size_full()
            .into_any_element()
        };
        let stale =
            !self.cells.is_empty() && self.coordinator.read(cx).collection.error().is_some();
        div()
            .id("v3-browse")
            .track_focus(&self.focus)
            .key_context("KinemaBrowse")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(p.bg_canvas)
            .text_color(p.text_primary)
            .type_style(t::BODY)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let current = this
                    .cells
                    .iter()
                    .position(|c| c.card.read(cx).focus_handle(cx).is_focused(window))
                    .or_else(|| {
                        this.cells
                            .iter()
                            .position(|c| Some(c.title.key()) == this.selected)
                    });
                let key = event.keystroke.key.as_str();
                let key = if event.keystroke.modifiers.control || event.keystroke.modifiers.platform
                {
                    match key {
                        "home" => "loaded-home",
                        "end" => "loaded-end",
                        _ => key,
                    }
                } else {
                    key
                };
                if key == "enter" {
                    if let Some(i) = current {
                        cx.emit(OpenTitle(this.cells[i].title.clone()));
                        cx.stop_propagation();
                    }
                    return;
                }
                if let Some(i) = navigate(
                    current,
                    key,
                    cols,
                    this.cells.len(),
                    (f32::from(window.viewport_size().height) / row_height).floor() as usize,
                ) {
                    this.selected = Some(this.cells[i].title.key());
                    this.scroll.scroll_to_item(i / cols, ScrollStrategy::Top);
                    this.cells[i]
                        .card
                        .update(cx, |card, cx| card.focus(window, cx));
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .when(stale, |el| {
                let coordinator = self.coordinator.clone();
                el.child(
                    Notice::new(
                        NoticeKind::Warning,
                        "Showing previous results · request failed",
                    )
                    .strip()
                    .action(
                        Button::new("retry-stale", "Retry").on_click(move |_, _, cx| {
                            coordinator.update(cx, |c, cx| c.retry_page(cx))
                        }),
                    ),
                )
            })
            .child(div().flex_1().min_h_0().p(s::GRID_INSET).child(content))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn grid_fills_width() {
        let (n, w) = columns(1280., 172.);
        assert_eq!(n, 6);
        assert!((n as f32 * w + (n - 1) as f32 * 12. + 40. - 1280.).abs() < 0.01);
        assert_eq!(columns(100., 172.).0, 2);
    }
    #[::core::prelude::v1::test]
    fn unknown_is_last_zero_is_numeric() {
        let mut ratings = [None, Some(0.), Some(8.1)];
        ratings.sort_by(|a, b| rating_order(*a, *b));
        assert_eq!(ratings, [Some(8.1), Some(0.), None]);
    }
    #[::core::prelude::v1::test]
    fn keyboard_edges_and_pages() {
        assert_eq!(navigate(None, "end", 6, 14, 2), Some(5));
        assert_eq!(navigate(Some(7), "home", 6, 14, 2), Some(6));
        assert_eq!(navigate(Some(7), "end", 6, 14, 2), Some(11));
        assert_eq!(navigate(Some(7), "loaded-home", 6, 14, 2), Some(0));
        assert_eq!(navigate(None, "loaded-end", 6, 14, 2), Some(13));
        assert_eq!(navigate(Some(11), "down", 6, 14, 2), Some(13));
        assert_eq!(navigate(Some(13), "down", 6, 14, 2), Some(13));
        assert_eq!(navigate(Some(1), "pagedown", 6, 14, 2), Some(13));
        assert_eq!(navigate(Some(0), "left", 6, 14, 2), Some(0));
        assert_eq!(navigate(None, "home", 6, 0, 2), None);
    }
}
