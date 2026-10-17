//! Poster-only production card. Hosts supply metadata, artwork and truthful commands.
//! Subscribe to `CardOpened` for the mouse-up / Enter open handler.
use super::{
    chip::Chip,
    menu::{ContextMenuExt, MenuItem},
    motion::{self, Motion},
    text::TypeExt,
    tokens::{self, Palette, space as s, typeface as t},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc, sync::Arc};
fn col() -> Div {
    div().flex().flex_col().gap(s::S8)
}
fn row() -> Div {
    div().flex().items_center().gap(s::S8)
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
}
pub struct CardOpened;
impl EventEmitter<CardOpened> for PosterCard {}

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
        self.rating = rating;
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
            motion::duration(Motion::Disclose, cx)
        };
        let from = self.from;
        let progress = self.progress.clone();
        let w = self.width;
        let h = w * 1.5;
        let artwork = if let Some(image) = &self.image {
            let image = img(image.clone()).object_fit(ObjectFit::Cover).size_full();
            if duration.is_zero() {
                self.art_from = 1.;
                image.into_any_element()
            } else {
                let art_from = self.art_from;
                image
                    .with_animation(
                        ("art-fade", self.art_generation),
                        Animation::new(duration).with_easing(motion::ease_out_cubic),
                        move |el, t| el.opacity(art_from + (1. - art_from) * t),
                    )
                    .into_any_element()
            }
        } else {
            col()
                .size_full()
                .justify_center()
                .items_center()
                .p(s::S12)
                .bg(p.bg_placeholder)
                .child(
                    div()
                        .type_style(t::HEADING)
                        .text_center()
                        .line_clamp(3)
                        .child(self.title.clone()),
                )
                .when(!self.year.is_empty(), |el| {
                    el.child(div().text_color(p.text_muted).child(self.year.clone()))
                })
                .into_any_element()
        };
        let image = div().absolute().inset_0().child(artwork);
        let panel = col()
            .gap_0()
            .absolute()
            .bottom_0()
            .left_0()
            .w_full()
            .text_color(dark.text_primary)
            .child(div().h(s::S48).flex_none().bg(linear_gradient(
                180.,
                linear_color_stop(dark.scrim_max.opacity(0.), 0.),
                linear_color_stop(dark.scrim_max, 1.),
            )))
            .child(
                col()
                    .flex_none()
                    .gap(s::S4)
                    .p(s::S12)
                    .bg(dark.scrim_max)
                    .child(
                        div()
                            .type_style(t::HEADING)
                            .line_clamp(2)
                            .child(self.title.clone()),
                    )
                    .when(!self.facts.is_empty(), |el| {
                        el.child(
                            div()
                                .type_style(t::SMALL)
                                .text_color(dark.text_secondary)
                                .child(self.facts.clone()),
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
                    .when(self.rating.is_none(), |el| {
                        el.child(div().type_style(t::SMALL).child("IMDb —"))
                    })
                    .when(w >= 172. && !self.synopsis.is_empty(), |el| {
                        el.child(
                            div()
                                .type_style(t::BODY)
                                .line_clamp(2)
                                .overflow_hidden()
                                .child(self.synopsis.clone()),
                        )
                    }),
            );
        let end = if target { 1. } else { 0. };
        let style_image = move |el: Div, value: f32| {
            el.w(px(w * (1. + value * 0.05)))
                .h(px(h * (1. + value * 0.05)))
                .left(px(-w * value * 0.025))
                .top(px(-h * value * 0.025))
        };
        let style_panel =
            move |el: Div, value: f32| el.opacity(value).bottom(px(-8. * (1. - value)));
        let (image, panel) = if duration.is_zero() {
            self.from = end;
            progress.set(end);
            (
                style_image(image, end).into_any_element(),
                style_panel(panel, end).into_any_element(),
            )
        } else {
            (
                image
                    .with_animation(
                        ("zoom", self.generation),
                        Animation::new(duration).with_easing(motion::ease_out_cubic),
                        move |el, t| style_image(el, from + (end - from) * t),
                    )
                    .into_any_element(),
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
                    .into_any_element(),
            )
        };
        let visual = div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(image)
            .child(panel);
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
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.hovered = *hovered;
                cx.notify();
            }))
            .on_click(cx.listener(move |_, _, window, cx| {
                focus.focus(window, cx);
                cx.emit(CardOpened);
            }))
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" {
                    cx.emit(CardOpened);
                    cx.stop_propagation();
                }
            }))
            .child(visual)
            .child(
                row()
                    .absolute()
                    .top(s::S8)
                    .left(s::S8)
                    .right(s::S8)
                    .justify_between()
                    .child(Chip::rating(self.rating).over_art(true))
                    .when(self.series, |el| el.child(Chip::series().over_art(true))),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .border_1()
                    .border_color(p.poster_edge),
            )
            .when(focus_is_visible(&self.focus, window), |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border(s::FOCUS_RING)
                        .border_color(p.focus_ring),
                )
            });
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
