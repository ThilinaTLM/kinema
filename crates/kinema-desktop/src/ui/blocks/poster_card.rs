//! Poster-only production card. Hosts supply metadata, artwork and truthful commands.
//! Subscribe to `CardOpened` for the mouse-up / Enter open handler.
use crate::ui::theme::opacity;
use crate::{
    ui::blocks::state_toggle::{StateToggle, ToggleKind, ToggleStatus},
    ui::motion::{self, AnimateExt, CardHover, card_hover as hv, transition::CROSSFADE},
    ui::primitives::badge::{self, Badge},
    ui::primitives::menu::{ContextMenuExt, MenuItem},
    ui::theme::typography::{TextSize, TypeExt, Weight},
    ui::theme::{self, Palette, radius, size, space as s},
};
use gpui::{prelude::*, *};
use std::{sync::Arc, time::Instant};
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
    hover: CardHover,
    origin: Instant,
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
            hover: CardHover::default(),
            origin: Instant::now(),
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
        let p = *theme::palette(cx);
        let dark = Palette::dark();
        let focus = self.focus.clone();
        let focused = focus.is_focused(window);
        let target = self.hovered || focused;
        let now = self.origin.elapsed().as_secs_f32() * 1000.;
        let reduced = cx.reduce_motion() || motion::reduced(cx);
        self.hover
            .set_target(now, target, focused && !self.hovered, reduced);
        let hover = self.hover.values(now);
        if !self.hover.settled(now) {
            // Frame-driven so a reversal mid-flight continues from the current value.
            window.request_animation_frame();
        }
        let w = self.width;
        let h = w * 1.5;
        let placeholder = col()
            .absolute()
            .inset_0()
            .justify_center()
            .items_center()
            .p(s::S12)
            .rounded(radius::LG)
            .bg(p.muted)
            .when(self.image.is_none(), |el| {
                el.child(
                    div()
                        .text(TextSize::Lg)
                        .weight(Weight::Semibold)
                        .text_center()
                        .clamp_lines(3)
                        .child(self.title.clone()),
                )
                .when(!self.year.is_empty(), |el| {
                    el.child(
                        div()
                            .text_color(p.subtle_foreground)
                            .child(self.year.clone()),
                    )
                })
            });
        let artwork = self.image.as_ref().map(|image| {
            let image = img(image.clone())
                .object_fit(ObjectFit::Cover)
                .rounded(radius::LG)
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
            if reduced {
                self.art_from = 1.;
                frame(image.into_any_element())
            } else {
                let art_from = self.art_from;
                frame(image.animate_with(
                    ("art-fade", self.art_generation),
                    CROSSFADE,
                    cx,
                    move |el, t| el.opacity(art_from + (1. - art_from) * t),
                ))
            }
        });
        let meta = {
            let base = if self.facts.is_empty() {
                self.year.to_string()
            } else {
                self.facts.to_string()
            };
            let score = badge::format_rating(self.rating).map(|r| format!("\u{2605} {r}"));
            match (base.is_empty(), score) {
                (false, Some(r)) => format!("{base} \u{b7} {r}"),
                (false, None) => base,
                (true, Some(r)) => r,
                (true, None) => String::new(),
            }
        };
        let row = |el: Div, value: f32| {
            el.relative()
                .top(hv::ROW_RISE * (1. - value))
                .opacity(value)
        };
        let [r_title, r_meta, r_genres, r_synopsis] = hover.rows;
        let panel = col()
            .gap(s::S4)
            .absolute()
            .bottom_0()
            .left_0()
            .w_full()
            .px(s::S12)
            .pb(s::S12)
            .pt(s::S32)
            .rounded_b(radius::LG)
            .text_color(dark.foreground)
            .child(
                // Scrim fades on its own so the text rows can rise independently.
                div()
                    .absolute()
                    .inset_0()
                    .rounded_b(radius::LG)
                    .opacity(hover.scrim)
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(theme::transparent(dark.scrim), 0.),
                        linear_color_stop(dark.scrim.opacity(opacity::POSTER_SCRIM_PEAK), 0.55),
                    )),
            )
            .child(row(
                div()
                    .text(TextSize::Lg)
                    .weight(Weight::Semibold)
                    .clamp_lines(2)
                    .child(self.title.clone()),
                r_title,
            ))
            .when(!meta.is_empty(), |el| {
                el.child(row(
                    div()
                        .text(TextSize::Sm)
                        .text_color(dark.muted_foreground)
                        .child(meta),
                    r_meta,
                ))
            })
            .when(!self.genres.is_empty(), |el| {
                el.child(row(
                    div()
                        .text(TextSize::Sm)
                        .text_color(dark.muted_foreground)
                        .child(self.genres.clone()),
                    r_genres,
                ))
            })
            .when(w >= 172. && !self.synopsis.is_empty(), |el| {
                el.child(row(
                    div()
                        .text(TextSize::Base)
                        .clamp_lines(2)
                        .overflow_hidden()
                        .child(self.synopsis.clone()),
                    r_synopsis,
                ))
            });
        let shadow = theme::elevation::md(&p);
        let panel_visible = hover.scrim > 0. || hover.rows.iter().any(|v| *v > 0.);
        let style_lift = |el: Div, value: f32| {
            el.top(-hv::LIFT * value)
                .shadow(crate::ui::blocks::media_card::lift_shadow(
                    shadow.clone(),
                    value,
                ))
        };
        let this = cx.entity();
        let lift = div()
            .size_full()
            .relative()
            .rounded(radius::LG)
            .child(placeholder)
            .children(artwork)
            .when(panel_visible, |el| el.child(panel))
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
                            .child(Badge::rating(self.rating).over_art(true)),
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
                            .when(self.series, |el| el.child(Badge::series().over_art(true))),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius::LG)
                    .border_1()
                    .border_color(p.poster_edge),
            )
            .when(focus_is_visible(&self.focus, window), |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(radius::LG)
                        .border(size::FOCUS_RING)
                        .border_color(p.ring),
                )
            });
        let lift = style_lift(lift, hover.lift);
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
    use super::{CardOpened, MembershipRequested, PosterCard, ToggleStatus};
    use crate::ui::theme;
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
        cx.update(theme::init);
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
