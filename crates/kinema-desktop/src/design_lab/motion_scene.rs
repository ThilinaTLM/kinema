//! `motion` scene: replayable specimens of every transition preset, a Presence
//! panel, the poster-card hover choreography and the skeleton pulse loop.
use gpui::{prelude::*, *};

use crate::{
    ui::blocks::poster_card::PosterCard,
    ui::motion::{self, AnimateExt, Axis, Effect, Presence, Transition, lerp, transition::*},
    ui::primitives::{
        Skeleton,
        button::{Button, ButtonSize, ButtonVariant},
    },
    ui::theme::{
        self, Palette,
        motion::timing,
        opacity, radius, space as s,
        typography::{TextSize, TypeExt, Weight},
    },
};

const PRESETS: [(&str, Transition); 12] = [
    ("FEEDBACK", FEEDBACK),
    ("HOVER", HOVER),
    ("DISCLOSE", DISCLOSE),
    ("POPUP_IN", POPUP_IN),
    ("POPUP_OUT", POPUP_OUT),
    ("CONTENT", CONTENT),
    ("COLUMN", COLUMN),
    ("CROSSFADE", CROSSFADE),
    ("SHEET", SHEET),
    ("FLIGHT", FLIGHT),
    ("CARD_OVERLAY", CARD_OVERLAY),
    ("CARD_OVERLAY_OUT", CARD_OVERLAY_OUT),
];

#[derive(Clone, Copy)]
struct Replay {
    enter: bool,
    run: u64,
}

pub(super) struct MotionScene {
    replay: [Option<Replay>; PRESETS.len()],
    runs: u64,
    presence: Entity<Presence>,
    card: Entity<PosterCard>,
    _observe: Subscription,
}

fn row() -> Div {
    div().flex().items_center().gap(s::S12)
}
fn col() -> Div {
    div().flex().flex_col().gap(s::S8)
}
fn muted(text: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text(TextSize::Sm)
        .mono()
        .text_color(p.muted_foreground)
        .child(text.into())
}
fn section(title: &'static str, note: &'static str, p: Palette) -> Div {
    col().gap(s::S12).child(
        div()
            .pb(s::S4)
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .text(TextSize::Lg)
                    .weight(Weight::Semibold)
                    .child(title),
            )
            .child(muted(note, p)),
    )
}

fn describe(preset: Transition) -> String {
    let effect = match preset.effect {
        Effect::None => "timing only".to_string(),
        Effect::Fade => "fade".to_string(),
        Effect::Rise(d) => format!("rise {}px", f32::from(d)),
        Effect::Slide(Axis::X, d) => format!("slide x {}px", f32::from(d)),
        Effect::Slide(Axis::Y, d) => format!("slide y {}px", f32::from(d)),
        Effect::Lift(d) => format!("lift {}px", f32::from(d)),
    };
    let delay = if preset.delay.is_zero() {
        String::new()
    } else {
        format!(" · +{} ms", preset.delay.as_millis())
    };
    format!(
        "{} ms · {:?}{delay} · {effect}",
        preset.duration.as_millis(),
        preset.ease
    )
}

impl MotionScene {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let presence = cx.new(|_| Presence::opened(POPUP_IN, POPUP_OUT));
        let observe = cx.observe(&presence, |_, _, cx| cx.notify());
        let card = cx.new(|cx| {
            let mut card = PosterCard::new(0, "Hover this poster", window, cx);
            card.set_art(Some(super::art(2, 240, 360)));
            card.set_rating(Some(7.9));
            card.year = "2024".into();
            card.facts = "2024 · 118 min".into();
            card.genres = "Drama · Mystery".into();
            card.synopsis = "A visitor uncovers a secret by the distant shore.".into();
            card.width = 172.;
            card
        });
        Self {
            replay: [None; PRESETS.len()],
            runs: 0,
            presence,
            card,
            _observe: observe,
        }
    }

    fn play(&mut self, index: usize, enter: bool, cx: &mut Context<Self>) {
        self.runs += 1;
        self.replay[index] = Some(Replay {
            enter,
            run: self.runs,
        });
        cx.notify();
    }

    fn specimen(&self, index: usize, p: Palette, cx: &App) -> AnyElement {
        let (name, preset) = PRESETS[index];
        let effect = preset.effect;
        let sample = div()
            .relative()
            .w(px(168.))
            .h(s::S32)
            .rounded(radius::MD)
            .bg(p.primary)
            .flex()
            .items_center()
            .justify_center()
            .text(TextSize::Sm)
            .text_color(p.primary_foreground)
            .child(name);
        let apply = move |el: Div, shown: f32| match effect {
            Effect::None => el.opacity(lerp(opacity::PULSE_MIN, 1.0, shown)),
            _ => effect.apply(el, shown),
        };
        match self.replay[index] {
            None => apply(
                sample,
                if matches!(effect, Effect::Lift(_)) {
                    0.0
                } else {
                    1.0
                },
            )
            .into_any_element(),
            Some(Replay { enter, run }) => {
                let id = ElementId::NamedInteger(format!("motion-{name}").into(), run);
                sample.animate_with(id, preset, cx, move |el, shown| {
                    apply(el, if enter { shown } else { 1.0 - shown })
                })
            }
        }
    }

    fn preset_rows(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let mut rows = col().gap(s::S6);
        for (i, (name, preset)) in PRESETS.into_iter().enumerate() {
            rows = rows.child(
                row()
                    .h(s::S48)
                    .child(
                        col()
                            .gap(s::S2)
                            .w(px(420.))
                            .flex_none()
                            .child(
                                div()
                                    .text(TextSize::Base)
                                    .weight(Weight::Medium)
                                    .child(name),
                            )
                            .child(muted(describe(preset), p)),
                    )
                    .child(
                        Button::new(("enter", i), "Enter")
                            .size(ButtonSize::Sm)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(move |this, _, _, cx| this.play(i, true, cx))),
                    )
                    .child(
                        Button::new(("exit", i), "Exit")
                            .size(ButtonSize::Sm)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(move |this, _, _, cx| this.play(i, false, cx))),
                    )
                    .child(
                        div()
                            .w(px(220.))
                            .h(s::S48)
                            .flex()
                            .items_center()
                            .px(s::S12)
                            .child(self.specimen(i, p, cx)),
                    ),
            );
        }
        rows
    }

    fn presence_demo(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let presence = self.presence.clone();
        let state = self.presence.read(cx);
        let visible = state.is_visible();
        let panel = visible.then(|| {
            state.animate(
                div()
                    .relative()
                    .w(px(280.))
                    .p(s::S16)
                    .rounded(radius::XL)
                    .bg(p.popover)
                    .border_1()
                    .border_color(p.border)
                    .shadow(theme::elevation::lg(&p))
                    .child(
                        div()
                            .text(TextSize::Base)
                            .weight(Weight::Medium)
                            .child("Presence panel"),
                    )
                    .child(muted("POPUP_IN on open, POPUP_OUT on close", p)),
                "lab-presence",
                cx,
            )
        });
        row()
            .items_start()
            .gap(s::S24)
            .child(
                Button::new("presence-toggle", "Toggle panel")
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_, _, cx| {
                        presence.update(cx, |presence, cx| {
                            if presence.is_open() {
                                presence.close(cx)
                            } else {
                                presence.open(cx)
                            }
                        })
                    }),
            )
            .child(div().w(px(320.)).h(px(110.)).pt(s::S8).children(panel))
    }

    fn pulse_demo(&self, p: Palette, cx: &App) -> Div {
        let reduced = motion::reduced(cx);
        let pulse = pulsating_between(opacity::PULSE_MIN, 1.0);
        let block = div().w(px(168.)).h(s::S32).rounded(radius::MD).bg(p.muted);
        let loop_block = if reduced {
            block.opacity(1.0).into_any_element()
        } else {
            block.animate_loop("lab-pulse", timing::PULSE_PERIOD, cx, move |el, t| {
                el.opacity(pulse(t))
            })
        };
        row()
            .gap(s::S24)
            .child(loop_block)
            .child(Skeleton::line().w(px(240.)))
            .child(muted(
                format!(
                    "PULSE_PERIOD {} ms · opacity {}–1{}",
                    timing::PULSE_PERIOD.as_millis(),
                    opacity::PULSE_MIN,
                    if reduced { " · static (reduced)" } else { "" }
                ),
                p,
            ))
    }
}

impl Render for MotionScene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *theme::palette(cx);
        let reduced = motion::reduced(cx);
        col()
            .p(s::S24)
            .gap(s::S32)
            .child(
                row()
                    .justify_between()
                    .child(
                        div()
                            .text(TextSize::Xxl)
                            .weight(Weight::Semibold)
                            .child("Motion"),
                    )
                    .child(muted(
                        if reduced {
                            "reduced motion: ON · every preset resolves to 0 ms"
                        } else {
                            "reduced motion: off (use --reduce-motion)"
                        },
                        p,
                    )),
            )
            .child(
                section(
                    "Transition presets",
                    "Enter / Exit replays the preset on its sample.",
                    p,
                )
                .child(self.preset_rows(p, cx)),
            )
            .child(
                section(
                    "Presence",
                    "Open → Closing (exit plays) → Closed; reopen mid-exit continues.",
                    p,
                )
                .child(self.presence_demo(p, cx)),
            )
            .child(
                section(
                    "CardHover",
                    "Lift, then scrim and four staggered rows after hover intent.",
                    p,
                )
                .child(div().pt(s::S8).child(self.card.clone())),
            )
            .child(
                section("Pulse loop", "Skeleton / loading shimmer.", p)
                    .child(self.pulse_demo(p, cx)),
            )
    }
}
