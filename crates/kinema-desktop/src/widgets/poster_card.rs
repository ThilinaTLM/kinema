//! Poster-only production card. Hosts supply metadata, artwork and truthful commands.
//! Subscribe to `CardOpened` for the mouse-up / Enter open handler.
use super::{
    chip::{self, Chip},
    menu::{ContextMenuExt, MenuItem},
    motion::{self, Motion},
    state_toggle::{StateToggle, ToggleKind, ToggleStatus},
    text::TypeExt,
    tokens::{self, Palette, radius, space as s, typeface as t},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc, sync::Arc};
fn col() -> Div {
    div().flex().flex_col().gap(s::S8)
}
fn focus_is_visible(focus: &FocusHandle, window: &Window) -> bool {
    window.last_input_was_keyboard() && focus.is_focused(window)
}
pub struct PosterCard {
    index: usize,
    title: SharedString,
    image: Option<Arc<RenderImage>>,
    art_generation: usize,
    art_from: f32,
    rating: Option<f32>,
    focus: FocusHandle,
    hovered: bool,
    disclosed: bool,
    generation: usize,
    progress: Rc<Cell<f32>>,
    from: f32,
    pub width: f32,
    pub facts: SharedString,
    pub genres: SharedString,
    pub synopsis: SharedString,
    pub year: SharedString,
    pub series: bool,
    pub menu: Vec<MenuItem>,
    /// Shared personal-state snapshot; `Loading` until the host has a successful read.
    pub membership: ToggleStatus,
}
pub struct CardOpened;
impl EventEmitter<CardOpened> for PosterCard {}
/// Desired membership from the top-right toggle; never emitted with `CardOpened`.
pub struct MembershipRequested(pub bool);
impl EventEmitter<MembershipRequested> for PosterCard {}
/// The personal-state read failed and the user asked to retry it.
pub struct MembershipRetry;
impl EventEmitter<MembershipRetry> for PosterCard {}

impl PosterCard {
    pub fn new(
        index: usize,
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle().tab_stop(true);
        let entity_id = cx.entity_id();
        window
            .on_focus_in(&focus, cx, move |_, cx| cx.notify(entity_id))
            .detach();
        window
            .on_focus_out(&focus, cx, move |_, _, cx| cx.notify(entity_id))
            .detach();
        Self {
            index,
            title: title.into(),
            image: None,
            art_generation: 0,
            art_from: 0.,
            rating: None,
            focus,
            hovered: false,
            disclosed: false,
            generation: 0,
            progress: Rc::new(Cell::new(0.)),
            from: 0.,
            width: 190.,
            facts: "".into(),
            genres: "".into(),
            synopsis: "".into(),
            year: "".into(),
            series: false,
            menu: Vec::new(),
            membership: ToggleStatus::Loading,
        }
    }
    pub fn set_title(&mut self, title: impl Into<SharedString>) {
        self.title = title.into();
    }
    pub fn set_art(&mut self, image: Option<Arc<RenderImage>>) {
        if image.as_ref().is_some_and(|next| {
            self.image
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, next))
        }) {
            self.art_generation += 1;
            self.art_from = 0.;
        }
        self.image = image;
    }
    pub fn set_rating(&mut self, rating: Option<f32>) {
        // IMDb scores start at 1.0; a zero is an unrated placeholder, not a score.
        self.rating = rating.filter(|score| *score > 0.0);
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }
}
impl Render for PosterCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let dark = Palette::dark();
        let focus = self.focus.clone();
        let target = self.hovered || focus.is_focused(window);
        if target != self.disclosed {
            self.disclosed = target;
            self.from = self.progress.get();
            self.generation += 1;
        }
        let duration = if cx.reduce_motion() {
            std::time::Duration::ZERO
        } else {
            motion::duration(Motion::Hover, cx)
        };
        let fade = if cx.reduce_motion() {
            std::time::Duration::ZERO
        } else {
            motion::duration(Motion::Crossfade, cx)
        };
        let from = self.from;
        let progress = self.progress.clone();
        let w = self.width;
        let h = w * 1.5;
        let placeholder = col()
            .absolute()
            .inset_0()
            .justify_center()
            .items_center()
            .p(s::S12)
            .rounded(radius::MD)
            .bg(p.bg_placeholder)
            .when(self.image.is_none(), |el| {
                el.child(
                    div()
                        .type_style(t::HEADING)
                        .text_center()
                        .clamp_lines(3)
                        .child(self.title.clone()),
                )
                .when(!self.year.is_empty(), |el| {
                    el.child(div().text_color(p.text_muted).child(self.year.clone()))
                })
            });
        let artwork = self.image.as_ref().map(|image| {
            let image = img(image.clone())
                .object_fit(ObjectFit::Cover)
                .rounded(radius::MD)
                .size_full();
            // `img` measures to its natural size even when absolutely positioned,
            // so pin it with a fixed-bounds wrapper instead of insets on the image.
            let frame = |child: AnyElement| {
                div()
                    .absolute()
                    .inset_0()
                    .size_full()
                    .overflow_hidden()
                    .child(child)
                    .into_any_element()
            };
            if fade.is_zero() {
                self.art_from = 1.;
                frame(image.into_any_element())
            } else {
                let art_from = self.art_from;
                frame(
                    image
                        .with_animation(
                            ("art-fade", self.art_generation),
                            Animation::new(fade).with_easing(motion::ease_out_cubic),
                            move |el, t| el.opacity(art_from + (1. - art_from) * t),
                        )
                        .into_any_element(),
                )
            }
        });
        let meta = {
            let base = if self.facts.is_empty() {
                self.year.to_string()
            } else {
                self.facts.to_string()
            };
            let score = chip::format_rating(self.rating).map(|r| format!("\u{2605} {r}"));
            match (base.is_empty(), score) {
                (false, Some(r)) => format!("{base} \u{b7} {r}"),
                (false, None) => base,
                (true, Some(r)) => r,
                (true, None) => String::new(),
            }
        };
        let panel = col()
            .gap(s::S4)
            .absolute()
            .bottom_0()
            .left_0()
            .w_full()
            .px(s::S12)
            .pb(s::S12)
            .pt(s::S32)
            .rounded_b(radius::MD)
            .text_color(dark.text_primary)
            .bg(linear_gradient(
                180.,
                linear_color_stop(dark.scrim_max.opacity(0.), 0.),
                linear_color_stop(dark.scrim_max.opacity(0.92), 0.55),
            ))
            .child(
                div()
                    .type_style(t::HEADING)
                    .clamp_lines(2)
                    .child(self.title.clone()),
            )
            .when(!meta.is_empty(), |el| {
                el.child(
                    div()
                        .type_style(t::SMALL)
                        .text_color(dark.text_secondary)
                        .child(meta),
                )
            })
            .when(!self.genres.is_empty(), |el| {
                el.child(
                    div()
                        .type_style(t::SMALL)
                        .text_color(dark.text_secondary)
                        .child(self.genres.clone()),
                )
            })
            .when(w >= 172. && !self.synopsis.is_empty(), |el| {
                el.child(
                    div()
                        .type_style(t::BODY)
                        .clamp_lines(2)
                        .overflow_hidden()
                        .child(self.synopsis.clone()),
                )
            });
        let end = if target { 1. } else { 0. };
        let shadow = tokens::elevation::card_hover(&p);
        let style_panel =
            move |el: Div, value: f32| el.opacity(value).bottom(px(-8. * (1. - value)));
        let style_lift = {
            let shadow = shadow.clone();
            move |el: Div, value: f32| {
                el.top(px(-3. * value)).shadow(
                    shadow
                        .iter()
                        .map(|s| {
                            let mut s = s.clone();
                            s.color = s.color.opacity(s.color.a * value);
                            s
                        })
                        .collect::<Vec<_>>(),
                )
            }
        };
        let panel = if duration.is_zero() {
            self.from = end;
            progress.set(end);
            style_panel(panel, end).into_any_element()
        } else {
            panel
                .with_animation(
                    ("disclose", self.generation),
                    Animation::new(duration).with_easing(motion::ease_out_cubic),
                    move |el, t| {
                        let value = from + (end - from) * t;
                        progress.set(value);
                        style_panel(el, value)
                    },
                )
                .into_any_element()
        };
        let this = cx.entity();
        let lift = div()
            .size_full()
            .relative()
            .rounded(radius::MD)
            .child(placeholder)
            .children(artwork)
            .child(panel)
            .child(
                // Independent anchors: the optional rating never moves the top-right stack.
                div()
                    .absolute()
                    .inset_0()
                    .child(
                        div()
                            .absolute()
                            .top(s::S8)
                            .left(s::S8)
                            .child(Chip::rating(self.rating).over_art(true)),
                    )
                    .child(
                        col()
                            .absolute()
                            .top(s::S8)
                            .right(s::S8)
                            .gap(s::S4)
                            .items_end()
                            .child(
                                StateToggle::new(
                                    ("poster-library", self.index),
                                    ToggleKind::Membership,
                                    self.membership,
                                    self.title.clone(),
                                )
                                .over_art(true)
                                .on_change({
                                    let this = this.clone();
                                    move |wanted, _, cx| {
                                        this.update(cx, |_, cx| {
                                            cx.emit(MembershipRequested(wanted))
                                        })
                                    }
                                })
                                .on_retry({
                                    let this = this.clone();
                                    move |_, cx| this.update(cx, |_, cx| cx.emit(MembershipRetry))
                                }),
                            )
                            .when(self.series, |el| el.child(Chip::series().over_art(true))),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius::MD)
                    .border_1()
                    .border_color(p.poster_edge),
            )
            .when(focus_is_visible(&self.focus, window), |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(radius::MD)
                        .border(s::FOCUS_RING)
                        .border_color(p.focus_ring),
                )
            });
        let lift = if duration.is_zero() {
            style_lift(lift, end).into_any_element()
        } else {
            lift.with_animation(
                ("lift", self.generation),
                Animation::new(duration).with_easing(motion::ease_out_cubic),
                move |el, t| style_lift(el, from + (end - from) * t),
            )
            .into_any_element()
        };
        let card = div()
            .id(("poster", self.index))
            .track_focus(&focus)
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(format!(
                "{}, {}, {}, IMDb {}",
                self.title,
                self.year,
                if self.series { "Series" } else { "Movie" },
                self.rating
                    .map(|score| format!("{score:.1}"))
                    .unwrap_or_else(|| "unknown".into())
            ))
            .w(px(w))
            .h(px(h))
            .flex_none()
            .relative()
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.hovered = *hovered;
                cx.notify();
            }))
            .on_click(cx.listener(move |_, _, window, cx| {
                focus.focus(window, cx);
                cx.emit(CardOpened);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // Enter on a child toggle bubbles here; only the card itself opens.
                if event.keystroke.key == "enter" && this.focus.is_focused(window) {
                    cx.emit(CardOpened);
                    cx.stop_propagation();
                }
            }))
            .child(lift);
        let items = self.menu.clone();
        let target_focus = self.focus.clone();
        card.context_menu(("poster-menu", self.index), move |window, cx| {
            target_focus.focus(window, cx);
            items.clone()
        })
    }
}

impl Focusable for PosterCard {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(
    clippy::unwrap_used,
    reason = "test setup and event dispatch must succeed"
)]
mod interaction_tests {
    use super::{CardOpened, MembershipRequested, PosterCard, ToggleStatus, tokens};
    use gpui::{
        AppContext, InputEvent, KeyDownEvent, KeyUpEvent, Keystroke, MouseButton, MouseDownEvent,
        MouseUpEvent, TestAppContext, WindowHandle, px,
    };
    use std::{cell::Cell, rc::Rc};

    type Fixture = (
        TestAppContext,
        WindowHandle<PosterCard>,
        Rc<Cell<usize>>,
        Rc<std::cell::RefCell<Vec<bool>>>,
    );
    fn setup(status: ToggleStatus) -> Fixture {
        setup_with(status, None, false)
    }
    fn setup_with(status: ToggleStatus, rating: Option<f32>, series: bool) -> Fixture {
        let mut cx = TestAppContext::single();
        cx.update(tokens::init);
        let window = cx.add_window(|window, cx| {
            let mut card = PosterCard::new(0, "Fixture", window, cx);
            card.membership = status;
            card.set_rating(rating);
            card.series = series;
            card
        });
        let opened = Rc::new(Cell::new(0));
        let wanted = Rc::new(std::cell::RefCell::new(Vec::new()));
        let entity = window.root(&mut cx).unwrap();
        let (o, w) = (opened.clone(), wanted.clone());
        cx.update(|cx| {
            cx.subscribe(&entity, move |_, _: &CardOpened, _| o.set(o.get() + 1))
                .detach();
            cx.subscribe(&entity, move |_, e: &MembershipRequested, _| {
                w.borrow_mut().push(e.0)
            })
            .detach();
        });
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        (cx, window, opened, wanted)
    }

    fn click(cx: &mut TestAppContext, window: WindowHandle<PosterCard>, x: f32, y: f32) {
        cx.update_window(window.into(), |_, window, cx| {
            let position = gpui::point(px(x), px(y));
            window.dispatch_event(
                MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseUpEvent {
                    position,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
    }

    #[test]
    fn toggle_pointer_click_requests_once_without_opening() {
        let (mut cx, window, opened, wanted) = setup(ToggleStatus::Known {
            on: false,
            pending: false,
        });
        // Top-right target: card is 190 px wide; the 28 px control sits 8 px in.
        click(&mut cx, window, 190. - 8. - 14., 8. + 14.);
        assert_eq!(*wanted.borrow(), vec![true]);
        assert_eq!(opened.get(), 0);
        // The card body still opens.
        click(&mut cx, window, 60., 200.);
        assert_eq!(opened.get(), 1);
        assert_eq!(wanted.borrow().len(), 1);
    }

    #[test]
    fn rated_series_cards_keep_toggle_top_right() {
        let (mut cx, window, opened, wanted) = setup_with(
            ToggleStatus::Known {
                on: false,
                pending: false,
            },
            Some(8.1),
            true,
        );
        click(&mut cx, window, 190. - 8. - 14., 8. + 14.);
        assert_eq!(*wanted.borrow(), vec![true]);
        assert_eq!(opened.get(), 0);
    }

    #[test]
    fn toggle_enter_and_space_each_request_once_without_opening() {
        for key in ["enter", "space"] {
            let (mut cx, window, opened, wanted) = setup(ToggleStatus::Known {
                on: true,
                pending: false,
            });
            cx.update_window(window.into(), |_, window, cx| {
                // Card is the first tab stop; the next stop is the toggle.
                window.focus_next(cx);
                window.focus_next(cx);
                window.draw(cx).clear(cx);
            })
            .unwrap();
            let keystroke = Keystroke::parse(key).unwrap();
            cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_event(
                    KeyDownEvent {
                        keystroke: keystroke.clone(),
                        is_held: false,
                        prefer_character_input: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
            })
            .unwrap();
            assert_eq!(*wanted.borrow(), vec![false], "{key}");
            assert_eq!(opened.get(), 0, "{key}");
        }
    }

    #[test]
    fn loading_and_pending_toggles_do_not_write_or_open() {
        for status in [
            ToggleStatus::Loading,
            ToggleStatus::Known {
                on: false,
                pending: true,
            },
        ] {
            let (mut cx, window, opened, wanted) = setup(status);
            click(&mut cx, window, 190. - 8. - 14., 8. + 14.);
            assert!(wanted.borrow().is_empty(), "{status:?}");
            assert_eq!(opened.get(), 0, "{status:?}");
        }
    }
}
