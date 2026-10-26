//! Virtualized poster browse with stable loaded cards and static loading rows,
//! over a cinematic backdrop that follows the hovered or keyboard-selected title.
pub(crate) mod filters;
mod layout;
pub(crate) mod library;
use super::coordinator::{AppCoordinator, BrowseTitle, PersonalLoad, PersonalView};
pub(crate) use crate::widgets::patterns::menu_item;
use crate::{
    artwork::{self, ArtworkKey, ArtworkKind, ArtworkState},
    widgets::{
        button::Button,
        icon::IconName,
        menu::MenuItem,
        motion::{self, Motion},
        notice::{Notice, NoticeKind},
        patterns::{StateView, ViewState},
        poster_card::{CardOpened, MembershipRequested, MembershipRetry, PosterCard},
        skeleton_card::SkeletonCard,
        text::TypeExt,
        tokens::{self, Mode, space as s, theme, typeface as t},
    },
};
use gpui::{prelude::*, *};
use kinema_core::{MediaKind, discover::DiscoverFilters};
use layout::LoadingLayout;
pub(crate) use library::{LibraryFilters, Scope};
use library::{ScopeContext, membership_status, movie_watched, refocus};
use std::{cmp::Ordering, rc::Rc, sync::Arc, time::Duration};

/// Hover changes shorter than this never reach the artwork pipeline.
const BACKDROP_DEBOUNCE: Duration = Duration::from_millis(150);
/// Light canvas shows the (dark) backdrop only as a faint wash.
const BACKDROP_LIGHT_OPACITY: f32 = 0.22;
/// Share of the height, from the bottom, that fades into the canvas.
const BACKDROP_FADE_HEIGHT: f32 = 0.7;

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
            (Self::Provider, "Popular".into()),
            (Self::Title, "Title A–Z".into()),
            (Self::Year, "Newest".into()),
            (Self::Rating, "Rating".into()),
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
/// Which title drives the backdrop: the settled hover/selection if it is still
/// loaded, else the first title of the viewport. `None` for an empty grid.
fn backdrop_index(hinted: Option<usize>, first_visible: usize, count: usize) -> Option<usize> {
    (count > 0).then(|| {
        hinted
            .filter(|i| *i < count)
            .unwrap_or(first_visible.min(count - 1))
    })
}

/// Crossfade bookkeeping for the backdrop image, kept across renders.
#[derive(Default)]
struct BackdropLayers {
    current: Option<Arc<RenderImage>>,
    previous: Option<Arc<RenderImage>>,
}
impl BackdropLayers {
    fn show(&mut self, image: Arc<RenderImage>) {
        if self
            .current
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(current, &image))
        {
            self.previous = self.current.replace(image);
        }
    }
}

#[derive(Clone)]
pub(crate) struct OpenTitle(pub BrowseTitle);
impl EventEmitter<OpenTitle> for Browse {}
/// Empty-state actions the window handles because it owns the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowseRequest {
    ShowDiscover,
    ClearLibraryFilters,
}
impl EventEmitter<BrowseRequest> for Browse {}

/// The last write a card initiated, kept so a failure can offer Retry.
#[derive(Clone)]
pub(crate) enum CardWrite {
    Membership(BrowseTitle, bool),
    Watched(BrowseTitle, bool),
}

/// Everything the card context menu depends on; a change rebuilds the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MenuSig {
    saved: Option<bool>,
    watched: Option<bool>,
    pending: bool,
    failed: bool,
}
impl MenuSig {
    fn of(view: &PersonalView) -> Self {
        Self {
            saved: match &view.state {
                PersonalLoad::Ready(t) => Some(t.added_at.is_some()),
                _ => None,
            },
            watched: movie_watched(view),
            pending: view.pending,
            failed: matches!(view.state, PersonalLoad::Failed(_)),
        }
    }
}

/// What the grid shows; a change resets scroll and selection. Discover facets
/// are part of it because a new facet set is a new result list.
type Subject = (
    Scope,
    MediaKind,
    String,
    Option<LibraryFilters>,
    Option<DiscoverFilters>,
);

struct Cell {
    title: BrowseTitle,
    card: Entity<PosterCard>,
    sig: Option<MenuSig>,
    _subscriptions: Vec<Subscription>,
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
    /// Last visible IMDb IDs reported for enrichment; avoids per-frame calls.
    last_visible: Vec<kinema_core::catalog::ImdbId>,
    subject: Subject,
    scope: Scope,
    /// Remembered view per scope (indexed by `Scope::index`); Library's search lives here.
    contexts: [ScopeContext; 2],
    library: LibraryFilters,
    /// Row to scroll to once the restored scope has cards again.
    pending_scroll: Option<usize>,
    artwork_generation: u64,
    /// Settled title (by key) the backdrop follows; set after `BACKDROP_DEBOUNCE`.
    backdrop_title: Option<String>,
    pending_backdrop: Option<String>,
    backdrop_task: Option<Task<()>>,
    backdrop: BackdropLayers,
    _observation: Subscription,
}
impl Browse {
    /// How far the grid has scrolled under the floating header.
    pub(crate) fn scrolled_under_header(&self) -> Pixels {
        (-self.scroll.0.borrow().base_handle.offset().y).max(px(0.))
    }
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
            last_visible: Vec::new(),
            subject: (Scope::Discover, MediaKind::Movie, String::new(), None, None),
            scope: Scope::Discover,
            contexts: Default::default(),
            library: LibraryFilters::default(),
            pending_scroll: None,
            artwork_generation: artwork::begin_generation("v3-browse", cx),
            backdrop_title: None,
            pending_backdrop: None,
            backdrop_task: None,
            backdrop: BackdropLayers::default(),
            _observation: observation,
        }
    }
    /// Debounced: the backdrop follows the pointer or keyboard only once it rests.
    fn point_at(&mut self, key: String, cx: &mut Context<Self>) {
        if self.backdrop_title.as_ref() == Some(&key) {
            self.pending_backdrop = None;
            self.backdrop_task = None;
            return;
        }
        if self.pending_backdrop.as_ref() == Some(&key) {
            return;
        }
        self.pending_backdrop = Some(key);
        self.backdrop_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BACKDROP_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                this.backdrop_title = this.pending_backdrop.take();
                this.backdrop_task = None;
                cx.notify();
            });
        }));
    }
    fn reset_backdrop(&mut self) {
        self.backdrop_title = None;
        self.pending_backdrop = None;
        self.backdrop_task = None;
        self.backdrop = BackdropLayers::default();
    }
    /// Resolve the backdrop image for the target title. Requests carry the
    /// owner generation and are deduplicated by the artwork store.
    fn update_backdrop(&mut self, cols: usize, cx: &mut Context<Self>) {
        let hinted = self
            .backdrop_title
            .as_ref()
            .and_then(|key| self.cells.iter().position(|c| c.title.key() == *key));
        let first = self.scroll.0.borrow().base_handle.logical_scroll_top().0 * cols;
        let Some(index) = backdrop_index(hinted, first, self.cells.len()) else {
            return;
        };
        #[cfg(feature = "ui-probe")]
        if let Some(image) = self
            .coordinator
            .read(cx)
            .fixture_art
            .get(&self.cells[index].title.key())
            .cloned()
        {
            self.backdrop.show(image);
            return;
        }
        let Some(key) = self.cells[index]
            .title
            .poster_url()
            .and_then(|url| ArtworkKey::new(ArtworkKind::Poster, url))
        else {
            return;
        };
        // Loading and failed keep the previous image: no flash to empty.
        if let ArtworkState::Ready(image) =
            artwork::backdrop_or_request("v3-browse", self.artwork_generation, &key, cx)
        {
            self.backdrop.show(image);
        }
    }
    fn backdrop_layer(&self, cx: &App) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let light = theme(cx).mode == Mode::Light;
        let fade = motion::duration(Motion::Crossfade, cx);
        let image = |image: &Arc<RenderImage>| {
            div()
                .absolute()
                .inset_0()
                .when(light, |el| el.opacity(BACKDROP_LIGHT_OPACITY))
                .child(img(image.clone()).object_fit(ObjectFit::Cover).size_full())
        };
        let current = self.backdrop.current.as_ref().map(|current| {
            let layer = image(current);
            if fade.is_zero() {
                layer.into_any_element()
            } else {
                // Keyed by the image so each new backdrop restarts the fade-in.
                let key = Arc::as_ptr(current) as *const () as usize;
                layer
                    .with_animation(
                        ("browse-backdrop", key),
                        Animation::new(fade).with_easing(motion::ease_out_cubic),
                        |el, t| el.opacity(t),
                    )
                    .into_any_element()
            }
        });
        let previous = self
            .backdrop
            .previous
            .as_ref()
            .filter(|_| !fade.is_zero())
            .map(image);
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .bg(p.bg_canvas)
            .children(previous)
            .children(current)
            .when(!light && self.backdrop.current.is_some(), |el| {
                el.child(div().absolute().inset_0().bg(p.backdrop_scrim))
            })
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(relative(BACKDROP_FADE_HEIGHT))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(p.bg_canvas.opacity(0.), 0.),
                        linear_color_stop(p.bg_canvas, 1.),
                    )),
            )
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn card_keys(&self) -> Vec<String> {
        self.cells.iter().map(|c| c.title.key()).collect()
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn card_for_test(&self, index: usize) -> Entity<PosterCard> {
        self.cells[index].card.clone()
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn focused_key(&self, window: &Window, cx: &App) -> Option<String> {
        self.focused_cell(window, cx)
            .map(|i| self.cells[i].title.key())
    }
    pub(crate) fn scope(&self) -> Scope {
        self.scope
    }
    pub(crate) fn library_filters(&self) -> LibraryFilters {
        self.library
    }
    pub(crate) fn library_search(&self) -> &str {
        &self.contexts[Scope::Library.index()].search
    }
    fn first_visible_row(&self) -> usize {
        self.scroll.0.borrow().base_handle.logical_scroll_top().0
    }
    fn focused_cell(&self, window: &Window, cx: &App) -> Option<usize> {
        self.cells
            .iter()
            .position(|c| c.card.read(cx).focus_handle(cx).is_focused(window))
    }
    /// Snapshot of this scope's selection, scroll and focus, taken before the
    /// window leaves it for Detail or the other scope.
    pub(crate) fn remember_context(&mut self, window: &Window, cx: &App) {
        let card_focused = self.focused_cell(window, cx).is_some();
        let row = self.first_visible_row();
        let ctx = &mut self.contexts[self.scope.index()];
        ctx.selected = self.selected.clone();
        ctx.scroll_row = row;
        ctx.card_focused = card_focused || ctx.card_focused && self.selected.is_some();
    }
    /// Switch scope, restoring the target scope's own selection and scroll.
    pub(crate) fn set_scope(&mut self, scope: Scope, window: &Window, cx: &mut Context<Self>) {
        self.enter_scope(scope, true, window, cx);
    }
    /// Native fixture seam: show Library for `filters`/`search` without a local read
    /// that would replace the seeded results.
    #[cfg(feature = "ui-probe")]
    pub(crate) fn show_library_fixture(
        &mut self,
        filters: LibraryFilters,
        search: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.library = filters;
        self.contexts[Scope::Library.index()].search = search.to_owned();
        self.enter_scope(Scope::Library, false, window, cx);
        self.subject = self.current_subject(cx);
    }
    fn enter_scope(&mut self, scope: Scope, reload: bool, window: &Window, cx: &mut Context<Self>) {
        if scope == self.scope {
            return;
        }
        self.remember_context(window, cx);
        self.scope = scope;
        let ctx = self.contexts[scope.index()].clone();
        self.selected = ctx.selected;
        self.pending_scroll = Some(ctx.scroll_row).filter(|row| *row > 0);
        // Cards belong to one scope; the other's list is rebuilt from its source.
        self.cells.clear();
        self.subject = self.current_subject(cx);
        self.reset_backdrop();
        self.last_visible.clear();
        self.page_request_generation = None;
        if reload && scope == Scope::Library {
            self.reload_library(cx);
        }
        cx.notify();
    }
    fn current_subject(&self, cx: &App) -> Subject {
        let state = self.coordinator.read(cx);
        match self.scope {
            Scope::Discover => (
                Scope::Discover,
                state.kind,
                state.query.clone(),
                None,
                state
                    .discover_filters_apply()
                    .then(|| state.discover_filters().clone()),
            ),
            Scope::Library => (
                Scope::Library,
                MediaKind::Movie,
                self.library_search().to_owned(),
                Some(self.library),
                None,
            ),
        }
    }
    pub(crate) fn reload_library(&mut self, cx: &mut Context<Self>) {
        let query = self.library.query(self.library_search());
        self.coordinator
            .update(cx, |c, cx| c.load_library(query, cx));
    }
    /// Apply new Library filters and search; results come from a fresh local query.
    pub(crate) fn set_library(
        &mut self,
        mut filters: LibraryFilters,
        search: &str,
        cx: &mut Context<Self>,
    ) {
        filters.set_kind(filters.kind);
        self.library = filters;
        self.contexts[Scope::Library.index()].search = search.to_owned();
        if self.scope == Scope::Library {
            self.reload_library(cx);
        }
        cx.notify();
    }
    pub(crate) fn clear_library_filters(&mut self, cx: &mut Context<Self>) {
        self.library.kind = Default::default();
        self.library.status = Default::default();
        self.contexts[Scope::Library.index()].search.clear();
        self.reload_library(cx);
        cx.notify();
    }
    /// Issue a card-initiated write with the clicked title's identity.
    pub(crate) fn write(&mut self, write: CardWrite, cx: &mut Context<Self>) {
        // A rejected write is recorded by the coordinator as the latest failure
        // (with its exact Retry), which the window's single toast presents.
        let _ = self.coordinator.update(cx, |c, cx| match &write {
            CardWrite::Membership(t, saved) => c.set_membership(t, *saved, cx),
            CardWrite::Watched(t, watched) => c.set_movie_watched(t, *watched, cx),
        });
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
        let subject = self.current_subject(cx);
        let state = self.coordinator.read(cx);
        let mut titles = match self.scope {
            Scope::Discover => state.titles.clone(),
            Scope::Library => state.library_titles.clone(),
        };
        // A same-query Library reload (or a failed one) with no rows yet keeps the
        // loaded cards, their focus and scroll instead of flashing an empty grid.
        let keep = self.scope == Scope::Library
            && titles.is_empty()
            && (state.library.busy() || state.library.error().is_some())
            && self.subject == subject
            && !self.cells.is_empty();
        if keep {
            self.finish_sync(cx);
            return;
        }
        let old_top = self.first_visible_row() * self.cols;
        let anchor = (self.subject == subject)
            .then(|| self.cells.get(old_top).map(|c| c.title.key()))
            .flatten();
        if self.subject != subject {
            self.subject = subject;
            self.selected = None;
            self.pending_scroll = None;
            self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
            self.artwork_generation = artwork::begin_generation("v3-browse", cx);
            self.reset_backdrop();
            self.last_visible.clear();
        }

        // Provider facets order results server-side; the local sort only applies
        // where they cannot (search, or a provider without faceted Discover).
        if self.scope == Scope::Discover && !self.coordinator.read(cx).discover_filters_apply() {
            match self.sort {
                Sort::Provider => {}
                Sort::Title => titles.sort_by_key(|a| a.name().to_lowercase()),
                Sort::Year => titles.sort_by_key(|a| std::cmp::Reverse(a.year())),
                Sort::Rating => titles.sort_by(|a, b| rating_order(a.rating(), b.rating())),
            }
        }
        if self.cells.iter().map(|c| &c.title).eq(titles.iter()) {
            self.finish_sync(cx);
            return;
        }
        let old_keys: Vec<String> = self.cells.iter().map(|c| c.title.key()).collect();
        let focused_key = self.focused_cell(window, cx).map(|i| old_keys[i].clone());
        let mut fresh: Vec<BrowseTitle> = Vec::new();
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
                if !old_keys.contains(&title.key()) && !matches!(title, BrowseTitle::Local(_)) {
                    fresh.push(title.clone());
                }
                let mut subscriptions = Vec::new();
                let opened = title.clone();
                subscriptions.push(cx.subscribe(&card, move |this, _, _: &CardOpened, cx| {
                    this.selected = Some(opened.key());
                    cx.emit(OpenTitle(opened.clone()));
                }));
                let clicked = title.clone();
                subscriptions.push(cx.subscribe(
                    &card,
                    move |this, _, event: &MembershipRequested, cx| {
                        this.selected = Some(clicked.key());
                        this.write(CardWrite::Membership(clicked.clone(), event.0), cx);
                    },
                ));
                let retried = title.clone();
                subscriptions.push(
                    cx.subscribe(&card, move |this, _, _: &MembershipRetry, cx| {
                        let title = retried.clone();
                        this.coordinator
                            .update(cx, |c, cx| c.load_personal(&[title], cx));
                    }),
                );
                Cell {
                    title,
                    card,
                    sig: None,
                    _subscriptions: subscriptions,
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
        // Keep focus by title; when the focused card left the list, move to the
        // next card, else the previous, else the grid (the empty state follows).
        let new_keys: Vec<String> = self.cells.iter().map(|c| c.title.key()).collect();
        let tracked = focused_key.clone().or_else(|| self.selected.clone());
        if let Some(key) = tracked
            && !new_keys.contains(&key)
        {
            let target = refocus(&old_keys, &new_keys, &key);
            self.selected = target.map(|i| new_keys[i].clone());
            if focused_key.is_some() {
                match target {
                    Some(i) => self.cells[i]
                        .card
                        .update(cx, |card, cx| card.focus(window, cx)),
                    None => self.focus.focus(window, cx),
                }
            }
        }
        if !fresh.is_empty() {
            let coordinator = self.coordinator.clone();
            cx.defer(move |cx| coordinator.update(cx, |c, cx| c.load_personal(&fresh, cx)));
        }
        self.finish_sync(cx);
    }
    /// Shared per-frame tail: card state from the coordinator, then pending scroll.
    fn finish_sync(&mut self, cx: &mut Context<Self>) {
        let views: Vec<PersonalView> = {
            let state = self.coordinator.read(cx);
            self.cells
                .iter()
                .map(|c| state.personal(&c.title))
                .collect()
        };
        let weak = cx.entity().downgrade();
        for (cell, view) in self.cells.iter_mut().zip(views) {
            let status = membership_status(&view);
            cell.card.update(cx, |card, cx| {
                if card.membership != status {
                    card.membership = status;
                    cx.notify();
                }
            });
            let sig = MenuSig::of(&view);
            if cell.sig != Some(sig) {
                cell.sig = Some(sig);
                let items = card_menu(weak.clone(), self.coordinator.clone(), &cell.title, sig);
                cell.card.update(cx, |card, _| card.menu = items);
            }
        }
        if let Some(row) = self.pending_scroll
            && !self.cells.is_empty()
        {
            self.pending_scroll = None;
            self.scroll.scroll_to_item_strict(
                row.min((self.cells.len() - 1) / self.cols),
                ScrollStrategy::Top,
            );
        }
    }
}

fn action_item(
    label: &str,
    disabled_reason: Option<&str>,
    handler: impl Fn(&mut Window, &mut App) + 'static,
) -> MenuItem {
    MenuItem::Action {
        label: label.to_owned().into(),
        shortcut: None,
        icon: None,
        disabled_reason: disabled_reason.map(|r| r.to_owned().into()),
        checked: false,
        handler: Rc::new(handler),
    }
}

/// Context menu for one card; Library and watched commands act on the clicked title.
fn card_menu(
    browse: WeakEntity<Browse>,
    coordinator: Entity<AppCoordinator>,
    title: &BrowseTitle,
    sig: MenuSig,
) -> Vec<MenuItem> {
    let weak = browse.clone();
    let open = title.clone();
    let mut items = vec![menu_item("Open details", move |_, cx| {
        let _ = weak.update(cx, |this, cx| {
            this.selected = Some(open.key());
            cx.emit(OpenTitle(open.clone()));
        });
    })];
    if title.kind() == MediaKind::Movie && title.imdb().is_some() {
        let weak = browse.clone();
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
    let blocked = if sig.pending {
        Some("Saving…")
    } else if sig.failed {
        Some("Library status unavailable")
    } else if sig.saved.is_none() {
        Some("Checking Library status…")
    } else {
        None
    };
    let saved = sig.saved.unwrap_or(false);
    let weak = browse.clone();
    let target = title.clone();
    items.push(action_item(
        if saved {
            "Remove from Library"
        } else {
            "Add to Library"
        },
        blocked,
        move |_, cx| {
            let target = target.clone();
            let _ = weak.update(cx, |this, cx| {
                this.write(CardWrite::Membership(target, !saved), cx)
            });
        },
    ));
    if title.kind() == MediaKind::Movie {
        let watched = sig.watched.unwrap_or(false);
        let weak = browse.clone();
        let target = title.clone();
        items.push(action_item(
            if watched {
                "Mark as unwatched"
            } else {
                "Mark as watched"
            },
            blocked,
            move |_, cx| {
                let target = target.clone();
                let _ = weak.update(cx, |this, cx| {
                    this.write(CardWrite::Watched(target, !watched), cx)
                });
            },
        ));
    }
    if sig.failed {
        let retry = title.clone();
        let coordinator = coordinator.clone();
        items.push(action_item("Retry Library status", None, move |_, cx| {
            let retry = retry.clone();
            coordinator.update(cx, |c, cx| c.load_personal(&[retry], cx));
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
    let refresh_title = title.clone();
    items.push(menu_item("Refresh metadata", move |_, cx| {
        coordinator.update(cx, |c, cx| c.title_details(&refresh_title, true, cx))
    }));
    items
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
        let scope = self.scope;
        let library = scope == Scope::Library;
        let has_more = library && state.library_has_more;
        let count = self.cells.len();
        let (busy, append_in_flight, generation, error, query) = if library {
            (
                state.library.busy(),
                state.library.busy() && has_more && count > 0,
                state.library.generation(),
                state.library.error().map(str::to_owned),
                self.library_search().to_owned(),
            )
        } else {
            (
                state.collection.busy(),
                state.append_in_flight(),
                state.collection.generation(),
                state.collection.error().map(str::to_owned),
                state.query.clone(),
            )
        };
        let filtered = library && self.library.is_filtered(&query);
        self.update_backdrop(cols, cx);
        let is_list = !(count == 0 && !busy);
        let stale = !self.cells.is_empty() && error.is_some();
        // A stale strip sits directly under the header and pushes the grid down.
        let top_inset = if stale {
            s::GRID_INSET
        } else {
            s::TOOLBAR_HEIGHT + s::GRID_INSET
        };
        let content = if count == 0 && !busy {
            let coordinator = self.coordinator.clone();
            let weak_retry = cx.entity().downgrade();
            if let Some(message) = error.clone() {
                // A failed request is never presented as "no results".
                StateView::new("browse-error", ViewState::Error)
                    .message(message)
                    .on_retry(move |_, cx| {
                        if library {
                            let _ = weak_retry.update(cx, |b, cx| b.reload_library(cx));
                        } else {
                            coordinator
                                .update(cx, |c, cx| c.load(c.kind, c.query.clone(), true, cx))
                        }
                    })
                    .into_any_element()
            } else if library {
                let weak = cx.entity().downgrade();
                let (title, hint, label, request) = if filtered {
                    (
                        "No saved titles match".to_owned(),
                        "Try different words or relax the filters.",
                        "Clear search and filters",
                        BrowseRequest::ClearLibraryFilters,
                    )
                } else {
                    (
                        "Your Library is empty".to_owned(),
                        "Save movies and series with the bookmark on their posters.",
                        "Browse Discover",
                        BrowseRequest::ShowDiscover,
                    )
                };
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(StateView::new("library-empty", ViewState::Empty).empty(
                        IconName::Bookmark,
                        title,
                        Some(hint.into()),
                    ))
                    .child(
                        Button::new("library-action", label).on_click(move |_, _, cx| {
                            let _ = weak.update(cx, |_, cx| cx.emit(request));
                        }),
                    )
                    .into_any_element()
            } else {
                let (title, hint) = if query.is_empty() {
                    (
                        "Nothing to show here".to_owned(),
                        Some("This collection has no titles right now.".into()),
                    )
                } else {
                    (
                        format!("No titles match “{query}”"),
                        Some("Check the spelling or try a different search.".into()),
                    )
                };
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(StateView::new("browse-empty", ViewState::Empty).empty(
                        IconName::Search,
                        title,
                        hint,
                    ))
                    .when(!query.is_empty(), |el| {
                        el.child(Button::new("collection-action", "Clear search").on_click(
                            move |_, _, cx| {
                                coordinator
                                    .update(cx, |c, cx| c.load(c.kind, String::new(), true, cx))
                            },
                        ))
                    })
                    .into_any_element()
            }
        } else {
            let layout = LoadingLayout::new(
                count,
                cols,
                (f32::from(window.viewport_size().height) / row_height).ceil() as usize + 2,
                busy,
                append_in_flight,
            );
            let rows = layout.rows();
            // Library pages stop at the end of the local result set.
            let failed = error.is_some() || (library && !has_more);
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
                    if range.len() > 1 {
                        // Visible rows only; a changed set is debounced by the
                        // coordinator, never submitted from this render pass.
                        let ids: Vec<_> = (range.start * cols..(range.end * cols).min(count))
                            .filter_map(|index| this.cells.get(index))
                            .filter_map(|cell| cell.title.imdb().cloned())
                            .collect();
                        if ids != this.last_visible {
                            this.last_visible.clone_from(&ids);
                            let coordinator = this.coordinator.clone();
                            cx.defer(move |cx| {
                                coordinator.update(cx, |c, cx| c.note_visible_titles(ids, cx));
                            });
                        }
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
                                if library {
                                    if c.library.generation() == generation {
                                        c.next_library_page(cx);
                                    }
                                } else if c.collection.generation() == generation {
                                    c.next_page(cx);
                                }
                            });
                        });
                    }
                    let weak = cx.entity().downgrade();
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
                                let key = cell.title.key();
                                let weak = weak.clone();
                                cards.push(
                                    div()
                                        .id(("poster-cell", index))
                                        .flex_none()
                                        .w(px(width))
                                        .child(cell.card.clone())
                                        .on_hover(move |hovered, _, cx| {
                                            if *hovered {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.point_at(key.clone(), cx)
                                                });
                                            }
                                        })
                                        .into_any_element(),
                                );
                            }
                            // While appending, the partial last row is completed
                            // with placeholders so the loading block reads as one grid.
                            if row + 1 == layout.title_rows && layout.skeleton_rows > 0 {
                                let missing = cols * layout.title_rows - count;
                                cards.extend(
                                    (0..missing)
                                        .map(|_| SkeletonCard::new(width).into_any_element()),
                                );
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
            // Rows scroll under the transparent header; the inset keeps the
            // first row clear of it at scroll top.
            .pt(top_inset)
            .pb(s::GRID_INSET)
            .px(s::GRID_INSET)
            .size_full()
            .into_any_element()
        };
        // Non-list states start below the floating header; the list insets itself
        // so its rows can scroll under it.
        let content = if is_list {
            content
        } else {
            div()
                .size_full()
                .px(s::GRID_INSET)
                .pb(s::GRID_INSET)
                .pt(s::TOOLBAR_HEIGHT + s::GRID_INSET)
                .child(content)
                .into_any_element()
        };
        div()
            .id("v3-browse")
            .track_focus(&self.focus)
            .key_context("KinemaBrowse")
            .size_full()
            .min_h_0()
            .relative()
            .flex()
            .flex_col()
            .bg(p.bg_canvas)
            .text_color(p.text_primary)
            .type_style(t::BODY)
            .child(self.backdrop_layer(cx))
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
                    let key = this.cells[i].title.key();
                    this.selected = Some(key.clone());
                    this.point_at(key, cx);
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
                let weak_retry = cx.entity().downgrade();
                el.child(
                    div().pt(s::TOOLBAR_HEIGHT).child(
                        Notice::new(
                            NoticeKind::Warning,
                            "Showing previous results · request failed",
                        )
                        .strip()
                        .action(
                            Button::new("retry-stale", "Retry").on_click(move |_, _, cx| {
                                if library {
                                    // A failed page keeps loaded cards; reload only if none remain.
                                    if coordinator.read(cx).library_titles.is_empty() {
                                        let _ = weak_retry.update(cx, |b, cx| b.reload_library(cx));
                                    } else {
                                        coordinator.update(cx, |c, cx| c.next_library_page(cx));
                                    }
                                } else {
                                    coordinator.update(cx, |c, cx| c.retry_page(cx))
                                }
                            }),
                        ),
                    ),
                )
            })
            .child(div().flex_1().min_h_0().child(content))
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
    fn backdrop_defaults_to_first_visible_and_survives_stale_hints() {
        assert_eq!(backdrop_index(None, 6, 20), Some(6));
        assert_eq!(backdrop_index(Some(3), 6, 20), Some(3));
        assert_eq!(backdrop_index(Some(30), 6, 20), Some(6));
        assert_eq!(backdrop_index(None, 40, 20), Some(19));
        assert_eq!(backdrop_index(Some(0), 0, 0), None);
    }
    #[::core::prelude::v1::test]
    fn sort_labels_are_user_facing() {
        let labels: Vec<_> = Sort::options().into_iter().map(|(_, l)| l).collect();
        assert_eq!(labels, ["Popular", "Title A–Z", "Newest", "Rating"]);
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
