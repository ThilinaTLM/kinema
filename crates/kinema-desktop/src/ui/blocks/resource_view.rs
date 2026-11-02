//! Loading / error / empty / content switching for remote-backed views.
//!
//! [`select_view`] is the pure policy. [`StateView`] renders an explicit
//! [`ViewState`]; [`ResourceView`] derives the state from an
//! `app_state::Resource`, keeping a stale value on screen while it refreshes.
use std::{rc::Rc, time::Instant};

use gpui::{prelude::*, *};

use crate::{
    app_state::Resource,
    ui::motion::{self, AnimateExt, transition::CROSSFADE},
    ui::primitives::Icon,
    ui::primitives::IconName,
    ui::primitives::button::Button,
    ui::primitives::notice::{Notice, NoticeKind},
    ui::theme::motion::timing,
    ui::theme::typography::TypeExt,
    ui::theme::{palette, space, typeface},
};

/// What a view currently shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewState {
    Loading,
    Error,
    Empty,
    Content,
}

/// Result of [`select_view`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewSelection {
    pub state: ViewState,
    /// Stale content stays visible while a refresh is in flight.
    pub refreshing: bool,
    /// Stale content stays visible after a failed refresh.
    pub stale_error: bool,
}

/// Pure state policy.
///
/// * No value: failure -> `Error`; otherwise `Loading` (also when idle: there
///   is nothing to show yet, which is not the same as an empty result).
/// * Value present: failure with content keeps it with `stale_error`; an empty
///   value with a failure is an `Error`; an empty value being refreshed is
///   `Loading`; an empty settled value is `Empty`; otherwise `Content`
///   (`refreshing` while busy).
pub fn select_view(
    has_value: bool,
    value_is_empty: bool,
    failed: bool,
    busy: bool,
) -> ViewSelection {
    let plain = |state| ViewSelection {
        state,
        refreshing: false,
        stale_error: false,
    };
    if !has_value {
        return plain(if failed {
            ViewState::Error
        } else {
            ViewState::Loading
        });
    }
    if value_is_empty {
        return plain(if failed {
            ViewState::Error
        } else if busy {
            ViewState::Loading
        } else {
            ViewState::Empty
        });
    }
    ViewSelection {
        state: ViewState::Content,
        refreshing: busy,
        stale_error: failed,
    }
}

type EmptyCheck<'a, T> = Box<dyn Fn(&T) -> bool + 'a>;
type Retry = Rc<dyn Fn(&mut Window, &mut App)>;

struct EmptySpec {
    icon: IconName,
    title: SharedString,
    hint: Option<SharedString>,
}

/// Skeleton -> content crossfade bookkeeping, kept across renders.
#[derive(Default)]
struct Transition {
    was_loading: bool,
    epoch: u64,
    settled: Option<Instant>,
}

/// Indeterminate sweep height of the inline refresh indicator.
const REFRESH_BAR: Pixels = space::S2;

#[derive(IntoElement)]
pub struct StateView {
    id: ElementId,
    state: ViewState,
    base: Div,
    message: Option<SharedString>,
    hint: Option<SharedString>,
    loading: Option<AnyElement>,
    content: Option<AnyElement>,
    empty: Option<EmptySpec>,
    retry: Option<Retry>,
    refreshing: bool,
    stale_error: bool,
}

impl StateView {
    /// `id` must be stable across renders: it keys the crossfade state.
    pub fn new(id: impl Into<ElementId>, state: ViewState) -> Self {
        Self {
            id: id.into(),
            state,
            base: div().w_full().flex().flex_col(),
            message: None,
            hint: None,
            loading: None,
            content: None,
            empty: None,
            retry: None,
            refreshing: false,
            stale_error: false,
        }
    }
    /// Skeleton shown while loading and faded out when content arrives.
    pub fn skeleton(mut self, skeleton: impl IntoElement) -> Self {
        self.loading = Some(skeleton.into_any_element());
        self
    }
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
    pub fn empty(
        mut self,
        icon: IconName,
        title: impl Into<SharedString>,
        hint: impl Into<Option<SharedString>>,
    ) -> Self {
        self.empty = Some(EmptySpec {
            icon,
            title: title.into(),
            hint: hint.into(),
        });
        self
    }
    /// Failure text for [`ViewState::Error`] (and the stale-content strip).
    pub fn message(mut self, message: impl Into<SharedString>) -> Self {
        self.message = Some(message.into());
        self
    }
    /// Secondary line under the error message.
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    pub fn on_retry(mut self, retry: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.retry = Some(Rc::new(retry));
        self
    }
    pub fn refreshing(mut self, refreshing: bool) -> Self {
        self.refreshing = refreshing;
        self
    }
    pub fn stale_error(mut self, stale_error: bool) -> Self {
        self.stale_error = stale_error;
        self
    }
    #[cfg_attr(not(feature = "ui-probe"), allow(dead_code))]
    fn apply(mut self, selection: ViewSelection) -> Self {
        self.state = selection.state;
        self.refreshing = selection.refreshing;
        self.stale_error = selection.stale_error;
        self
    }
}

impl Styled for StateView {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

fn retry_button(id: impl Into<ElementId>, retry: Retry) -> Button {
    Button::new(id, "Retry").on_click(move |_, window, cx| retry(window, cx))
}

fn centered(icon: IconName, ink: Hsla, title: SharedString, hint: Option<SharedString>) -> Div {
    div()
        .w_full()
        .py(space::S32)
        .flex()
        .flex_col()
        .items_center()
        .gap(space::S8)
        .child(Icon::new(icon).color(ink))
        .child(
            div()
                .type_style(typeface::HEADING)
                .text_center()
                .child(title),
        )
        .when_some(hint, |el, hint| {
            el.child(
                div()
                    .type_style(typeface::SMALL)
                    .text_center()
                    .opacity(0.8)
                    .child(hint),
            )
        })
}

fn refresh_bar(cx: &App) -> AnyElement {
    let p = palette(cx);
    let track = div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(REFRESH_BAR)
        .overflow_hidden();
    if motion::reduced(cx) {
        return track
            .child(div().size_full().bg(p.text_muted))
            .into_any_element();
    }
    track
        .child(
            div()
                .absolute()
                .h_full()
                .w(relative(0.25))
                .rounded_full()
                .bg(p.primary)
                .animate_loop("refresh-sweep", timing::SPIN_PERIOD, cx, |el, t| {
                    el.left(relative(t * 1.25 - 0.25))
                }),
        )
        .into_any_element()
}

impl RenderOnce for StateView {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let fade = motion::duration(CROSSFADE, cx);
        let transition = window.use_keyed_state((self.id.clone(), "transition"), cx, |_, _| {
            Transition::default()
        });
        let now = Instant::now();
        transition.update(cx, |t, _| match self.state {
            ViewState::Loading => {
                t.was_loading = true;
                t.settled = None;
            }
            _ if t.was_loading => {
                t.was_loading = false;
                t.epoch += 1;
                t.settled = Some(now);
            }
            _ => {}
        });
        let (epoch, settled) = {
            let t = transition.read(cx);
            (t.epoch, t.settled)
        };
        let base = self.base;
        match self.state {
            ViewState::Loading => base.children(self.loading.take()).into_any_element(),
            ViewState::Error => {
                let message = self
                    .message
                    .unwrap_or_else(|| "Something went wrong.".into());
                base.child(
                    centered(IconName::Warning, p.status_warn, message, self.hint)
                        .text_color(p.text_primary)
                        .when_some(self.retry, |el, retry| {
                            el.child(
                                div()
                                    .pt(space::S4)
                                    .child(retry_button((self.id.clone(), "retry"), retry)),
                            )
                        }),
                )
                .into_any_element()
            }
            ViewState::Empty | ViewState::Content => {
                let fading =
                    !fade.is_zero() && settled.is_some_and(|at| now.duration_since(at) < fade);
                let body = if self.state == ViewState::Empty {
                    let spec = self.empty.take().unwrap_or(EmptySpec {
                        icon: IconName::Search,
                        title: "Nothing here yet.".into(),
                        hint: None,
                    });
                    centered(spec.icon, p.text_secondary, spec.title, spec.hint)
                        .text_color(p.text_primary)
                        .into_any_element()
                } else {
                    div()
                        .w_full()
                        .children(self.content.take())
                        .into_any_element()
                };
                // Always the same wrapper so the content keeps its identity;
                // without a loading-to-content switch it is an instant no-op.
                let body = div().w_full().child(body).animate_stable(
                    (self.id.clone(), format!("fade-{epoch}")),
                    CROSSFADE,
                    settled.is_some(),
                    cx,
                    |el, t| el.opacity(t),
                );
                base.when(self.stale_error && self.state == ViewState::Content, |el| {
                    let strip = Notice::new(NoticeKind::Warning, "Showing saved results")
                        .body(
                            self.message
                                .clone()
                                .unwrap_or_else(|| "The refresh failed.".into()),
                        )
                        .strip();
                    let strip = match self.retry.clone() {
                        Some(retry) => {
                            strip.action(retry_button((self.id.clone(), "retry"), retry))
                        }
                        None => strip,
                    };
                    el.child(div().w_full().pb(space::S8).child(strip))
                })
                .child(
                    div()
                        .relative()
                        .w_full()
                        .child(body)
                        .when(self.refreshing, |el| el.child(refresh_bar(cx)))
                        .when(fading, |el| {
                            el.children(self.loading.take().map(|skeleton| {
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .right_0()
                                    .overflow_hidden()
                                    .child(skeleton)
                                    .animate_exit(
                                        (self.id.clone(), format!("skeleton-out-{epoch}")),
                                        CROSSFADE,
                                        cx,
                                    )
                            }))
                        }),
                )
                .into_any_element()
            }
        }
    }
}

/// Derives a [`StateView`] from a `Resource<T>`:
/// `ResourceView::new("id", &resource).skeleton(..).content(|value| ..)`.
// Screens adopt this next; until then only the design lab (ui-probe) uses it.
#[cfg_attr(not(feature = "ui-probe"), allow(dead_code))]
pub(crate) struct ResourceView<'a, T> {
    resource: &'a Resource<T>,
    is_empty: Option<EmptyCheck<'a, T>>,
    view: StateView,
}

#[cfg_attr(not(feature = "ui-probe"), allow(dead_code))]
impl<'a, T> ResourceView<'a, T> {
    pub(crate) fn new(id: impl Into<ElementId>, resource: &'a Resource<T>) -> Self {
        Self {
            view: StateView::new(id, ViewState::Loading),
            resource,
            is_empty: None,
        }
    }
    pub(crate) fn skeleton(mut self, skeleton: impl IntoElement) -> Self {
        self.view = self.view.skeleton(skeleton);
        self
    }
    /// What counts as an empty result (default: never empty).
    pub(crate) fn empty_when(mut self, is_empty: impl Fn(&T) -> bool + 'a) -> Self {
        self.is_empty = Some(Box::new(is_empty));
        self
    }
    pub(crate) fn empty(
        mut self,
        icon: IconName,
        title: impl Into<SharedString>,
        hint: impl Into<Option<SharedString>>,
    ) -> Self {
        self.view = self.view.empty(icon, title, hint);
        self
    }
    pub(crate) fn on_retry(mut self, retry: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.view = self.view.on_retry(retry);
        self
    }
    /// Current selection, before any content is built.
    pub(crate) fn selection(&self) -> ViewSelection {
        let value = self.resource.value();
        select_view(
            value.is_some(),
            value.is_some_and(|v| self.is_empty.as_ref().is_some_and(|f| f(v))),
            self.resource.error().is_some(),
            self.resource.busy(),
        )
    }
    /// Builds the view; `render` runs only when content is shown.
    pub(crate) fn content(self, render: impl FnOnce(&T) -> AnyElement) -> StateView {
        let selection = self.selection();
        let mut view = self.view.apply(selection);
        if let Some(error) = self.resource.error() {
            view = view.message(error.to_owned());
        }
        if selection.state == ViewState::Content
            && let Some(value) = self.resource.value()
        {
            view = view.content(render(value));
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::{Resource, ResourceView, ViewState, select_view};

    fn s(has: bool, empty: bool, failed: bool, busy: bool) -> (ViewState, bool, bool) {
        let v = select_view(has, empty, failed, busy);
        (v.state, v.refreshing, v.stale_error)
    }

    #[test]
    fn no_value_is_loading_until_failure() {
        assert_eq!(
            s(false, false, false, true),
            (ViewState::Loading, false, false)
        );
        assert_eq!(
            s(false, false, false, false),
            (ViewState::Loading, false, false)
        );
        assert_eq!(
            s(false, false, true, false),
            (ViewState::Error, false, false)
        );
    }

    #[test]
    fn successful_empty_is_not_an_error_or_loading() {
        assert_eq!(s(true, true, false, false).0, ViewState::Empty);
        assert_eq!(s(true, true, true, false).0, ViewState::Error);
        assert_eq!(s(true, true, false, true).0, ViewState::Loading);
    }

    #[test]
    fn stale_content_survives_refresh_and_failure() {
        assert_eq!(
            s(true, false, false, false),
            (ViewState::Content, false, false)
        );
        assert_eq!(
            s(true, false, false, true),
            (ViewState::Content, true, false)
        );
        assert_eq!(
            s(true, false, true, false),
            (ViewState::Content, false, true)
        );
    }

    #[test]
    fn resource_view_follows_resource_lifecycle() {
        let mut r = Resource::<Vec<u8>>::default();
        let view = |r: &Resource<Vec<u8>>| {
            ResourceView::new("t", r)
                .empty_when(|v| v.is_empty())
                .selection()
        };
        assert_eq!(view(&r).state, ViewState::Loading);
        let g = r.begin(true);
        assert_eq!(view(&r).state, ViewState::Loading);
        assert!(r.fetched(g, vec![], 1));
        assert_eq!(view(&r).state, ViewState::Empty);
        let g = r.begin(false);
        assert_eq!(view(&r).state, ViewState::Loading);
        assert!(r.fetched(g, vec![1], 2));
        assert_eq!(view(&r).state, ViewState::Content);
        let g = r.begin(false);
        assert!(view(&r).refreshing);
        assert!(r.failed(g, "offline"));
        let v = view(&r);
        assert!(v.stale_error && v.state == ViewState::Content);
        let g = r.begin(true);
        assert!(r.failed(g, "offline"));
        assert_eq!(view(&r).state, ViewState::Error);
    }
}
