//! `tokens` scene: every design token for the active mode, labelled by name.
use gpui::{prelude::*, *};

use crate::ui::theme::{
    Mode, Palette, elevation, opacity, radius, space as s,
    typography::{TextSize, TypeExt, Weight},
};

fn row() -> Div {
    div().flex().items_center().gap(s::S8)
}
fn col() -> Div {
    div().flex().flex_col().gap(s::S8)
}
fn section(title: &'static str, p: Palette) -> Div {
    col().gap(s::S12).child(
        div()
            .pb(s::S4)
            .border_b_1()
            .border_color(p.border)
            .text(TextSize::Lg)
            .weight(Weight::Semibold)
            .child(title),
    )
}
fn label(text: impl Into<SharedString>, p: Palette) -> Div {
    div()
        .text(TextSize::Sm)
        .mono()
        .text_color(p.muted_foreground)
        .child(text.into())
}

fn swatch(name: &'static str, color: Hsla, p: Palette) -> Div {
    col()
        .w(px(132.))
        .gap(s::S4)
        .child(
            div()
                .h(s::S48)
                .rounded(radius::MD)
                .border_1()
                .border_color(p.input)
                .bg(p.muted)
                .child(div().size_full().rounded(radius::MD).bg(color)),
        )
        .child(label(name, p))
}

fn group(title: &'static str, colors: &[(&'static str, Hsla)], p: Palette) -> Div {
    let mut swatches = div().flex().flex_wrap().gap(s::S16);
    for (name, color) in colors {
        swatches = swatches.child(swatch(name, *color, p));
    }
    col()
        .gap(s::S8)
        .child(div().text(TextSize::Sm).weight(Weight::Medium).child(title))
        .child(swatches)
}

fn palette_section(p: Palette, mode: Mode) -> Div {
    section(
        match mode {
            Mode::Dark => "Color · dark",
            Mode::Light => "Color · light",
        },
        p,
    )
    .child(group(
        "Surfaces",
        &[
            ("background", p.background),
            ("surface", p.surface),
            ("card", p.card),
            ("popover", p.popover),
            ("muted", p.muted),
        ],
        p,
    ))
    .child(group(
        "Text",
        &[
            ("foreground", p.foreground),
            ("muted_foreground", p.muted_foreground),
            ("subtle_foreground", p.subtle_foreground),
            ("primary_text", p.primary_text),
        ],
        p,
    ))
    .child(group(
        "Actions",
        &[
            ("primary", p.primary),
            ("primary_foreground", p.primary_foreground),
            ("secondary", p.secondary),
            ("accent", p.accent),
            ("selection", p.selection),
        ],
        p,
    ))
    .child(group(
        "Lines",
        &[("border", p.border), ("input", p.input), ("ring", p.ring)],
        p,
    ))
    .child(group(
        "Status",
        &[
            ("success", p.success),
            ("warning", p.warning),
            ("destructive", p.destructive),
        ],
        p,
    ))
    .child(group(
        "Over art & overlays",
        &[
            ("shadow", p.shadow),
            ("overlay", p.overlay),
            ("scrim", p.scrim),
            ("chip_on_art", p.chip_on_art),
            ("poster_edge", p.poster_edge),
        ],
        p,
    ))
    .child(group(
        "Rating & brand",
        &[
            ("rating", p.rating),
            ("rating_on_art", p.rating_on_art),
            ("brand_wash", p.brand_wash),
        ],
        p,
    ))
    .child(group(
        "Derived states (computed, not tokens)",
        &[
            ("hover(secondary)", p.hover(p.secondary)),
            ("pressed(secondary)", p.pressed(p.secondary)),
            ("primary_hover()", p.primary_hover()),
            ("primary_pressed()", p.primary_pressed()),
        ],
        p,
    ))
}

fn type_section(p: Palette) -> Div {
    let mut sizes = col().gap(s::S8);
    for size in TextSize::ALL {
        sizes = sizes.child(
            row()
                .items_baseline()
                .gap(s::S16)
                .child(
                    label(
                        format!(
                            "{size:?} {}/{}",
                            f32::from(size.size()),
                            f32::from(size.line_height())
                        ),
                        p,
                    )
                    .w(px(120.))
                    .flex_none(),
                )
                .child(div().text(size).child("Northern lights over the harbour")),
        );
    }
    let mut weights = row().gap(s::S24);
    for (name, weight) in [
        ("Regular", Weight::Regular),
        ("Medium", Weight::Medium),
        ("Semibold", Weight::Semibold),
        ("Bold", Weight::Bold),
    ] {
        weights = weights.child(
            col()
                .gap(s::S2)
                .child(div().text(TextSize::Xl).weight(weight).child("Aa Gg 0123"))
                .child(label(format!("{name} {}", weight.value().0), p)),
        );
    }
    section("Typography", p).child(sizes).child(weights).child(
        row()
            .gap(s::S24)
            .child(div().text(TextSize::Base).mono().child("mono · 1080p.mkv"))
            .child(
                div()
                    .text(TextSize::Base)
                    .tabular()
                    .child("tabular · 1111 / 0000"),
            ),
    )
}

fn space_section(p: Palette) -> Div {
    let mut bars = col().gap(s::S6);
    for (name, value) in [
        ("S2", s::S2),
        ("S4", s::S4),
        ("S6", s::S6),
        ("S8", s::S8),
        ("S12", s::S12),
        ("S16", s::S16),
        ("S20", s::S20),
        ("S24", s::S24),
        ("S32", s::S32),
        ("S48", s::S48),
    ] {
        bars = bars.child(
            row()
                .gap(s::S12)
                .child(label(format!("{name} · {}px", f32::from(value)), p).w(px(96.)))
                .child(div().w(value).h(s::S12).rounded(radius::SM).bg(p.primary)),
        );
    }
    section("Spacing", p).child(bars)
}

fn radius_section(p: Palette) -> Div {
    let mut samples = row().gap(s::S24).items_start();
    for (name, value) in [
        ("SM", radius::SM),
        ("MD", radius::MD),
        ("LG", radius::LG),
        ("XL", radius::XL),
        ("FULL", radius::FULL),
    ] {
        samples = samples.child(
            col()
                .gap(s::S6)
                .child(
                    div()
                        .size(px(72.))
                        .rounded(value)
                        .bg(p.card)
                        .border_1()
                        .border_color(p.input),
                )
                .child(label(
                    if name == "FULL" {
                        "FULL · pill".to_string()
                    } else {
                        format!("{name} · {}px", f32::from(value))
                    },
                    p,
                )),
        );
    }
    section("Radius", p).child(samples)
}

fn elevation_section(p: Palette) -> Div {
    let mut samples = row().gap(s::S32).items_start().py(s::S16);
    for (name, shadow) in [
        ("sm · 0/2/8", elevation::sm(&p)),
        ("md · 0/4/14", elevation::md(&p)),
        ("lg · 0/6/20", elevation::lg(&p)),
        ("xl · 0/12/36", elevation::xl(&p)),
    ] {
        samples = samples.child(
            col()
                .gap(s::S12)
                .child(
                    div()
                        .size(px(96.))
                        .rounded(radius::LG)
                        .bg(p.card)
                        .shadow(shadow),
                )
                .child(label(name, p)),
        );
    }
    section("Elevation", p).child(samples)
}

fn opacity_section(p: Palette) -> Div {
    let mut steps = div().flex().flex_wrap().gap(s::S16);
    for (name, value) in [
        ("PULSE_MIN", opacity::PULSE_MIN),
        ("HOVER_MIX", opacity::HOVER_MIX),
        ("PRESSED_MIX", opacity::PRESSED_MIX),
        ("BADGE_WASH", opacity::BADGE_WASH),
        ("ALERT_EDGE", opacity::ALERT_EDGE),
        ("SELECTED_EDGE", opacity::SELECTED_EDGE),
        ("DIM_MUTED", opacity::DIM_MUTED),
        ("DIM_FALLBACK", opacity::DIM_FALLBACK),
        ("TRACK", opacity::TRACK),
        ("INK_SOFT", opacity::INK_SOFT),
        ("GLASS", opacity::GLASS),
        ("CARD_TRANSLUCENT", opacity::CARD_TRANSLUCENT),
        ("HEADER_SCRIM", opacity::HEADER_SCRIM),
    ] {
        steps = steps.child(
            col()
                .w(px(132.))
                .gap(s::S4)
                .child(
                    div()
                        .h(s::S32)
                        .rounded(radius::MD)
                        .border_1()
                        .border_color(p.border)
                        .bg(p.muted)
                        .child(
                            div()
                                .size_full()
                                .rounded(radius::MD)
                                .bg(p.primary)
                                .opacity(value),
                        ),
                )
                .child(label(format!("{name} {value}"), p)),
        );
    }
    section("Opacity (primary over muted)", p).child(steps)
}

pub(super) fn render(p: Palette, mode: Mode) -> Div {
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
                        .child("Design tokens"),
                )
                .child(label(
                    match mode {
                        Mode::Dark => "mode: dark",
                        Mode::Light => "mode: light",
                    },
                    p,
                )),
        )
        .child(palette_section(p, mode))
        .child(type_section(p))
        .child(space_section(p))
        .child(radius_section(p))
        .child(elevation_section(p))
        .child(opacity_section(p))
}
