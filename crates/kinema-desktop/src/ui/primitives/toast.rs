//! Toasts: the one place for transient outcomes and page conditions.
//!
//! Callers do not push toasts. Each render they *derive* the toasts that
//! should exist now as [`ToastSpec`]s and hand them to a [`ToastHost`] with
//! [`ToastHost::sync`]. The host owns presentation only: first-seen order, the
//! visible limit, timers, dismissal, hover pause and enter/exit motion.
//!
//! * **Identity.** A toast is its `key` (one slot per key; the last spec with a
//!   key wins) plus its title and body. Changed text is a new occurrence and
//!   shows again.
//! * **Closing.** The close button and a timer both close the occurrence. A
//!   closed occurrence stays closed while its spec keeps being requested, and
//!   `on_close` runs once so the owner can clear transient state. A spec that
//!   stops being requested is forgotten, so the same condition coming back later
//!   shows again.
//! * **Lifetime.** Specs are sticky unless given a timeout. Hovering the region
//!   pauses every timer; leaving restarts them with their full duration.
//! * **Limit.** At most `TOAST_MAX_VISIBLE` open toasts are shown, newest
//!   nearest the corner. Older open ones wait for a free slot, and their timers
//!   only start once they are visible.
//!
//! Accessibility: a card exposes `Role::Alert` for warnings and errors and
//! `Role::Status` otherwise, named by its title and body. The tone is paired with
//! a distinct icon. The action and the close button are ordinary [`Button`]s with
//! their own focus and names. Esc is not bound: in the main window it means Back.
use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    time::Duration,
};

use gpui::{prelude::*, *};

use crate::{
    ui::motion::{
        self, AnimateExt,
        transition::{POPUP_IN, POPUP_OUT},
    },
    ui::primitives::alert::AlertTone,
    ui::primitives::button::Button,
    ui::primitives::{Icon, IconName},
    ui::theme::layout::controls::{TOAST_ACCENT, TOAST_MAX_VISIBLE, TOAST_WIDTH},
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{elevation, palette, radius, size, space},
};

type Run = Rc<dyn Fn(&mut Window, &mut App)>;
type OnClose = Rc<dyn Fn(&mut App)>;

/// One button on a toast.
#[derive(Clone)]
pub struct ToastAction {
    label: SharedString,
    run: Run,
}

/// What a toast should show now. Built fresh by its owner on every sync.
#[derive(Clone)]
pub struct ToastSpec {
    key: SharedString,
    tone: AlertTone,
    title: SharedString,
    body: Option<SharedString>,
    action: Option<ToastAction>,
    timeout: Option<Duration>,
    on_close: Option<OnClose>,
}

impl ToastSpec {
    /// A sticky toast in slot `key`.
    pub fn new(
        key: impl Into<SharedString>,
        tone: AlertTone,
        title: impl Into<SharedString>,
    ) -> Self {
        Self {
            key: key.into(),
            tone,
            title: title.into(),
            body: None,
            action: None,
            timeout: None,
            on_close: None,
        }
    }
    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }
    /// At most one action. It does not close the toast; the owner's state does.
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some(ToastAction {
            label: label.into(),
            run: Rc::new(run),
        });
        self
    }
    /// Close by itself after `timeout` of visible, unhovered time.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    /// Runs once when the user or the timer closes this occurrence.
    pub fn on_close(mut self, on_close: impl Fn(&mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(on_close));
        self
    }
    pub fn key(&self) -> &SharedString {
        &self.key
    }
    pub fn tone(&self) -> AlertTone {
        self.tone
    }
    pub fn title(&self) -> &SharedString {
        &self.title
    }
    fn id(&self) -> ToastId {
        ToastId {
            key: self.key.clone(),
            title: self.title.clone(),
            body: self.body.clone(),
        }
    }
}

/// One occurrence: slot key plus the text it showed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ToastId {
    key: SharedString,
    title: SharedString,
    body: Option<SharedString>,
}

impl ToastId {
    fn hash64(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// Pure presence policy: first-seen order, closed occurrences, visible limit.
#[derive(Default)]
struct Queue {
    seen: Vec<ToastId>,
    closed: HashSet<ToastId>,
}

impl Queue {
    /// Record what is requested now and return the visible occurrences, oldest
    /// first. Occurrences no longer requested are forgotten.
    fn present(&mut self, requested: &[ToastId], max: usize) -> Vec<ToastId> {
        self.seen.retain(|id| requested.contains(id));
        self.closed.retain(|id| requested.contains(id));
        for id in requested {
            if !self.seen.contains(id) {
                self.seen.push(id.clone());
            }
        }
        let open: Vec<ToastId> = self
            .seen
            .iter()
            .filter(|id| !self.closed.contains(id))
            .cloned()
            .collect();
        let skip = open.len().saturating_sub(max);
        open[skip..].to_vec()
    }
    /// Close a requested, open occurrence. False when there was nothing to close.
    fn close(&mut self, id: &ToastId) -> bool {
        self.seen.contains(id) && self.closed.insert(id.clone())
    }
}

/// Keep the last spec per key, in request order of that last spec.
fn dedupe(specs: Vec<ToastSpec>) -> Vec<ToastSpec> {
    let mut out: Vec<ToastSpec> = Vec::with_capacity(specs.len());
    for spec in specs {
        out.retain(|s| s.key != spec.key);
        out.push(spec);
    }
    out
}

struct Shown {
    id: ToastId,
    hash: u64,
    spec: ToastSpec,
    /// Exit animation in progress; dropping the task cancels the removal.
    leaving: Option<Task<()>>,
}

/// Stateful toast region. Render it once per window, bottom-right.
pub struct ToastHost {
    queue: Queue,
    specs: Vec<ToastSpec>,
    shown: Vec<Shown>,
    timers: HashMap<ToastId, Task<()>>,
    hovered: bool,
}

impl Default for ToastHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastHost {
    pub fn new() -> Self {
        Self {
            queue: Queue::default(),
            specs: Vec::new(),
            shown: Vec::new(),
            timers: HashMap::new(),
            hovered: false,
        }
    }

    /// Replace the requested toasts. Never notifies: call it from the owner's
    /// render or observer, which already redraws the window.
    pub fn sync(&mut self, specs: Vec<ToastSpec>, cx: &mut Context<Self>) {
        self.specs = dedupe(specs);
        self.refresh(cx);
    }

    /// Titles of the toasts on screen now (not leaving), oldest first.
    pub fn visible_titles(&self) -> Vec<SharedString> {
        self.shown
            .iter()
            .filter(|s| s.leaving.is_none())
            .map(|s| s.spec.title.clone())
            .collect()
    }

    /// Close the visible toast in slot `key`, as its close button would.
    pub fn dismiss_key(&mut self, key: &str, cx: &mut Context<Self>) {
        let id = self
            .shown
            .iter()
            .find(|s| s.leaving.is_none() && s.id.key.as_ref() == key)
            .map(|s| s.id.clone());
        if let Some(id) = id {
            self.close(&id, cx);
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<ToastId> = self.specs.iter().map(ToastSpec::id).collect();
        let visible = self.queue.present(&ids, TOAST_MAX_VISIBLE);
        self.timers.retain(|id, _| visible.contains(id));
        let instant_exit = motion::duration(POPUP_OUT, cx).is_zero();
        let exit = motion::duration(POPUP_OUT, cx);
        // Shown entries that are no longer visible start leaving.
        let mut removed = Vec::new();
        for shown in &mut self.shown {
            if visible.contains(&shown.id) {
                shown.leaving = None;
                continue;
            }
            if shown.leaving.is_some() {
                continue;
            }
            if instant_exit {
                removed.push(shown.id.clone());
                continue;
            }
            let id = shown.id.clone();
            shown.leaving = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(exit).await;
                let _ = this.update(cx, |this, cx| {
                    this.shown.retain(|s| !(s.id == id && s.leaving.is_some()));
                    cx.notify();
                });
            }));
        }
        self.shown.retain(|s| !removed.contains(&s.id));
        for id in &visible {
            let Some(spec) = self.specs.iter().find(|s| &s.id() == id).cloned() else {
                continue;
            };
            if let Some(shown) = self.shown.iter_mut().find(|s| &s.id == id) {
                shown.spec = spec.clone();
            } else {
                self.shown.push(Shown {
                    id: id.clone(),
                    hash: id.hash64(),
                    spec: spec.clone(),
                    leaving: None,
                });
            }
            if let Some(timeout) = spec.timeout
                && !self.hovered
                && !self.timers.contains_key(id)
            {
                self.arm(id.clone(), timeout, cx);
            }
        }
    }

    fn arm(&mut self, id: ToastId, after: Duration, cx: &mut Context<Self>) {
        let key = id.clone();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(after).await;
            let _ = this.update(cx, |this, cx| this.close(&key, cx));
        });
        self.timers.insert(id, task);
    }

    fn close(&mut self, id: &ToastId, cx: &mut Context<Self>) {
        if !self.queue.close(id) {
            return;
        }
        self.timers.remove(id);
        let on_close = self
            .specs
            .iter()
            .find(|s| &s.id() == id)
            .and_then(|s| s.on_close.clone());
        self.refresh(cx);
        cx.notify();
        if let Some(on_close) = on_close {
            on_close(cx);
        }
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        if hovered {
            self.timers.clear();
        } else {
            self.refresh(cx);
        }
    }
}

impl Render for ToastHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host = cx.entity().downgrade();
        let cards: Vec<AnyElement> = self
            .shown
            .iter()
            .map(|shown| {
                let leaving = shown.leaving.is_some();
                let spec = &shown.spec;
                let mut toast = Toast::new(
                    ElementId::NamedInteger("toast".into(), shown.hash),
                    spec.tone,
                    spec.title.clone(),
                );
                toast.body = spec.body.clone();
                if !leaving {
                    if let Some(action) = spec.action.clone() {
                        toast.action = Some(
                            Button::new(
                                ElementId::NamedInteger("toast-action".into(), shown.hash),
                                action.label,
                            )
                            .on_click(move |_, window, cx| (action.run)(window, cx)),
                        );
                    }
                    let host = host.clone();
                    let id = shown.id.clone();
                    toast.close = Some(Rc::new(move |_, cx| {
                        let _ = host.update(cx, |h, cx| h.close(&id, cx));
                    }));
                }
                let card = div().w_full().child(toast);
                if leaving {
                    card.animate_exit(
                        ElementId::NamedInteger("toast-out".into(), shown.hash),
                        POPUP_OUT,
                        cx,
                    )
                } else {
                    card.animate_enter(
                        ElementId::NamedInteger("toast-in".into(), shown.hash),
                        POPUP_IN,
                        cx,
                    )
                }
            })
            .collect();
        div()
            .id("toast-region")
            .w(TOAST_WIDTH)
            .flex()
            .flex_col()
            .gap(space::S8)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.set_hovered(*hovered, cx)))
            .children(cards)
    }
}

/// One toast card. Built by [`ToastHost`]; public for the design lab.
#[derive(IntoElement)]
pub struct Toast {
    id: ElementId,
    tone: AlertTone,
    title: SharedString,
    body: Option<SharedString>,
    action: Option<Button>,
    close: Option<Run>,
}

impl Toast {
    pub fn new(id: impl Into<ElementId>, tone: AlertTone, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            tone,
            title: title.into(),
            body: None,
            action: None,
            close: None,
        }
    }
    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }
    pub fn action(mut self, action: Button) -> Self {
        self.action = Some(action);
        self
    }
    pub fn on_close(mut self, close: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.close = Some(Rc::new(close));
        self
    }
}

/// Icon and ink for a tone; neutral tones use muted ink.
fn tone_style(tone: AlertTone, p: &crate::ui::theme::Palette) -> (IconName, Hsla) {
    match tone {
        AlertTone::Info => (IconName::Info, p.muted_foreground),
        AlertTone::Success => (IconName::Check, p.success),
        AlertTone::Warning => (IconName::Warning, p.warning),
        AlertTone::Error => (IconName::Error, p.destructive),
        AlertTone::Empty => (IconName::Search, p.subtle_foreground),
    }
}

impl RenderOnce for Toast {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let (icon, ink) = tone_style(self.tone, &p);
        let name: SharedString = match &self.body {
            Some(body) => format!("{}. {body}", self.title).into(),
            None => self.title.clone(),
        };
        let close_id = match &self.id {
            ElementId::NamedInteger(_, n) => ElementId::NamedInteger("toast-close".into(), *n),
            other => ElementId::NamedChild(std::sync::Arc::new(other.clone()), "close".into()),
        };
        div()
            .id(self.id)
            .occlude()
            .role(match self.tone {
                AlertTone::Warning | AlertTone::Error => Role::Alert,
                _ => Role::Status,
            })
            .aria_label(name)
            .relative()
            .w_full()
            .flex()
            .items_start()
            .gap(space::S12)
            .pl(space::S16)
            .pr(space::S8)
            .py(space::S12)
            .rounded(radius::LG)
            .bg(p.popover)
            .border(size::HAIRLINE)
            .border_color(p.border)
            .shadow(elevation::lg(&p))
            .child(
                div()
                    .absolute()
                    .left(space::S6)
                    .top(space::S12)
                    .bottom(space::S12)
                    .w(TOAST_ACCENT)
                    .rounded(radius::FULL)
                    .bg(ink),
            )
            .child(
                div()
                    .flex_none()
                    .pt(space::S2)
                    .child(Icon::new(icon).color(ink)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(space::S2)
                    .pt(space::S2)
                    .child(
                        div()
                            .text(TextSize::Base)
                            .weight(Weight::Semibold)
                            .text_color(p.foreground)
                            .child(self.title),
                    )
                    .when_some(self.body, |el, body| {
                        el.child(
                            div()
                                .text(TextSize::Sm)
                                .text_color(p.muted_foreground)
                                .child(body),
                        )
                    }),
            )
            .when_some(self.action, |el, action| {
                el.child(div().flex_none().child(action))
            })
            .when_some(self.close, |el, close| {
                el.child(
                    div().flex_none().child(
                        Button::icon_button(close_id, IconName::Close, "Dismiss")
                            .on_click(move |_, window, cx| close(window, cx)),
                    ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Queue, ToastId, ToastSpec, dedupe};
    use crate::ui::primitives::alert::AlertTone;

    fn id(key: &str, title: &str) -> ToastId {
        ToastId {
            key: key.to_owned().into(),
            title: title.to_owned().into(),
            body: None,
        }
    }

    #[test]
    fn newest_open_toasts_are_visible_oldest_first() {
        let mut q = Queue::default();
        let all = [id("a", "1"), id("b", "2"), id("c", "3"), id("d", "4")];
        assert_eq!(q.present(&all, 3), all[1..].to_vec());
        // Closing a visible one frees a slot for the oldest waiting one.
        assert!(q.close(&all[3]));
        assert_eq!(q.present(&all, 3), all[..3].to_vec());
    }

    #[test]
    fn closed_stays_closed_while_requested_and_returns_after_absence() {
        let mut q = Queue::default();
        let a = id("a", "1");
        q.present(std::slice::from_ref(&a), 3);
        assert!(q.close(&a));
        assert!(!q.close(&a), "closing twice is a no-op");
        assert!(q.present(std::slice::from_ref(&a), 3).is_empty());
        assert!(q.present(&[], 3).is_empty());
        assert_eq!(q.present(std::slice::from_ref(&a), 3), vec![a]);
    }

    #[test]
    fn changed_text_is_a_new_occurrence() {
        let mut q = Queue::default();
        let first = id("slot", "Couldn't save");
        q.present(std::slice::from_ref(&first), 3);
        q.close(&first);
        let second = id("slot", "Couldn't save again");
        assert_eq!(q.present(std::slice::from_ref(&second), 3), vec![second]);
        assert!(!q.close(&first), "the old occurrence is forgotten");
    }

    #[test]
    fn last_spec_per_key_wins() {
        let specs = dedupe(vec![
            ToastSpec::new("a", AlertTone::Info, "old"),
            ToastSpec::new("b", AlertTone::Info, "b"),
            ToastSpec::new("a", AlertTone::Error, "new"),
        ]);
        let titles: Vec<_> = specs.iter().map(|s| s.title().to_string()).collect();
        assert_eq!(titles, ["b", "new"]);
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
mod timer_tests {
    use super::{ToastHost, ToastSpec};
    use crate::ui::primitives::alert::AlertTone;
    use gpui::{AppContext, Entity, TestAppContext};
    use std::{cell::Cell, rc::Rc, time::Duration};

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn setup(cx: &mut TestAppContext) -> (Entity<ToastHost>, Rc<Cell<usize>>) {
        cx.update(|cx| {
            crate::ui::theme::init(cx);
            crate::ui::theme::set_reduce_motion(true, cx);
        });
        (cx.new(|_| ToastHost::new()), Rc::new(Cell::new(0)))
    }

    fn spec(closed: &Rc<Cell<usize>>, timed: bool) -> ToastSpec {
        let count = closed.clone();
        let spec = ToastSpec::new("k", AlertTone::Success, "Saved")
            .on_close(move |_| count.set(count.get() + 1));
        if timed { spec.timeout(TIMEOUT) } else { spec }
    }

    #[gpui::test]
    fn timed_toasts_close_once_and_sticky_ones_stay(cx: &mut TestAppContext) {
        for timed in [true, false] {
            let (host, closed) = setup(cx);
            host.update(cx, |h, cx| h.sync(vec![spec(&closed, timed)], cx));
            cx.executor().advance_clock(TIMEOUT * 2);
            cx.run_until_parked();
            let visible = host.read_with(cx, |h, _| h.visible_titles().len());
            assert_eq!(visible, usize::from(!timed));
            assert_eq!(closed.get(), usize::from(timed));
            // Still requested after closing: it stays closed.
            host.update(cx, |h, cx| h.sync(vec![spec(&closed, timed)], cx));
            assert_eq!(host.read_with(cx, |h, _| h.visible_titles().len()), visible);
        }
    }

    #[gpui::test]
    fn hover_pauses_and_restarts_timers(cx: &mut TestAppContext) {
        let (host, closed) = setup(cx);
        host.update(cx, |h, cx| h.sync(vec![spec(&closed, true)], cx));
        host.update(cx, |h, cx| h.set_hovered(true, cx));
        cx.executor().advance_clock(TIMEOUT * 3);
        cx.run_until_parked();
        assert_eq!(closed.get(), 0, "paused while hovered");
        host.update(cx, |h, cx| h.set_hovered(false, cx));
        cx.executor()
            .advance_clock(TIMEOUT - Duration::from_millis(10));
        cx.run_until_parked();
        assert_eq!(closed.get(), 0, "full duration after leaving");
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        assert_eq!(closed.get(), 1);
    }

    #[gpui::test]
    fn dismiss_closes_sticky_toasts(cx: &mut TestAppContext) {
        let (host, closed) = setup(cx);
        host.update(cx, |h, cx| h.sync(vec![spec(&closed, false)], cx));
        host.update(cx, |h, cx| h.dismiss_key("k", cx));
        assert_eq!(closed.get(), 1);
        assert!(host.read_with(cx, |h, _| h.visible_titles().is_empty()));
    }
}
