//! Desktop v3 design tokens (see `docs/design/desktop-v3.md` §2).
//!
//! This is the only source of color, type and geometry for `widgets` and v3
//! screens. Screens must not use raw color or size literals.

use gpui::{App, Global, Hsla, Pixels, Rgba, px, rgb, rgba};

/// Effective color scheme. Dark is the product default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

/// Semantic palette (§2.1). All values are contrast-checked in the design doc.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg_canvas: Hsla,
    pub bg_surface: Hsla,
    pub bg_raised: Hsla,
    pub bg_placeholder: Hsla,
    pub ctl_rest: Hsla,
    pub ctl_hover: Hsla,
    pub ctl_pressed: Hsla,
    pub line_hairline: Hsla,
    pub line_control: Hsla,
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_pressed: Hsla,
    pub on_accent: Hsla,
    pub accent_text: Hsla,
    pub accent_tint: Hsla,
    pub focus_ring: Hsla,
    pub rating: Hsla,
    pub status_ok: Hsla,
    pub status_warn: Hsla,
    pub status_error: Hsla,
    /// Text-bearing scrim over artwork (same in both themes).
    pub scrim_max: Hsla,
    /// Chip backing over artwork (same in both themes).
    pub chip_over_art: Hsla,
    /// 1 px inner poster edge.
    pub poster_edge: Hsla,
    pub dialog_scrim: Hsla,
}

fn c(hex: u32) -> Hsla {
    let color: Rgba = rgb(hex);
    color.into()
}

fn ca(hex_rgba: u32) -> Hsla {
    let color: Rgba = rgba(hex_rgba);
    color.into()
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            bg_canvas: c(0x0B0C0E),
            bg_surface: c(0x131418),
            bg_raised: c(0x1B1D22),
            bg_placeholder: c(0x1B1D22),
            ctl_rest: c(0x24272D),
            ctl_hover: c(0x2D3037),
            ctl_pressed: c(0x373B43),
            line_hairline: c(0x25282E),
            line_control: c(0x646A75),
            text_primary: c(0xF2F3F5),
            text_secondary: c(0xB3B8C1),
            text_muted: c(0x8C929D),
            accent: c(0x6B4EF0),
            accent_hover: c(0x5D41E2),
            accent_pressed: c(0x4F34CF),
            on_accent: c(0xFFFFFF),
            accent_text: c(0xA996FF),
            accent_tint: ca(0x6B4EF029),
            focus_ring: c(0xB4A6FF),
            rating: c(0xF5C451),
            status_ok: c(0x46C98A),
            status_warn: c(0xF2A33A),
            status_error: c(0xFF6B6B),
            scrim_max: ca(0x0B0C0EEB),
            chip_over_art: ca(0x0B0C0ECC),
            poster_edge: ca(0xFFFFFF0F),
            dialog_scrim: ca(0x00000099),
        }
    }

    pub fn light() -> Self {
        Self {
            bg_canvas: c(0xF3F3F5),
            bg_surface: c(0xFFFFFF),
            bg_raised: c(0xFFFFFF),
            bg_placeholder: c(0xE4E5EA),
            ctl_rest: c(0xEAEAEE),
            ctl_hover: c(0xE1E2E7),
            ctl_pressed: c(0xD7D9DF),
            line_hairline: c(0xDCDDE3),
            line_control: c(0x868C97),
            text_primary: c(0x111215),
            text_secondary: c(0x4B505A),
            text_muted: c(0x5E646F),
            accent: c(0x5B3FE0),
            accent_hover: c(0x4F34CF),
            accent_pressed: c(0x4429BB),
            on_accent: c(0xFFFFFF),
            accent_text: c(0x5B3FE0),
            accent_tint: ca(0x5B3FE01A),
            focus_ring: c(0x5B3FE0),
            rating: c(0x8A5D00),
            status_ok: c(0x1E7A4C),
            status_warn: c(0x9A5400),
            status_error: c(0xC02B2B),
            scrim_max: ca(0x0B0C0EEB),
            chip_over_art: ca(0x0B0C0ECC),
            poster_edge: ca(0x00000014),
            dialog_scrim: ca(0x0B0C0E66),
        }
    }

    /// Rating ink for chips drawn over artwork (always on a dark chip).
    pub fn rating_over_art(&self) -> Hsla {
        c(0xF5C451)
    }
}

/// One step of the type scale (§2.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypeStyle {
    pub size: Pixels,
    pub line_height: Pixels,
    pub weight: gpui::FontWeight,
    pub mono: bool,
}

const fn ts(size: f32, line: f32, weight: f32, mono: bool) -> TypeStyle {
    TypeStyle {
        size: px(size),
        line_height: px(line),
        weight: gpui::FontWeight(weight),
        mono,
    }
}

pub mod typeface {
    use super::{TypeStyle, ts};

    pub const UI_FAMILY: &str = "Inter";
    pub const MONO_FAMILY: &str = "JetBrains Mono";

    pub const DISPLAY: TypeStyle = ts(30.0, 36.0, 700.0, false);
    pub const TITLE: TypeStyle = ts(18.0, 24.0, 600.0, false);
    pub const HEADING: TypeStyle = ts(15.0, 20.0, 600.0, false);
    pub const BODY: TypeStyle = ts(13.0, 18.0, 400.0, false);
    pub const BODY_LONG: TypeStyle = ts(14.0, 21.0, 400.0, false);
    pub const BODY_STRONG: TypeStyle = ts(13.0, 18.0, 600.0, false);
    pub const SMALL: TypeStyle = ts(12.0, 16.0, 400.0, false);
    pub const SMALL_MEDIUM: TypeStyle = ts(12.0, 16.0, 500.0, false);
    pub const CAPTION: TypeStyle = ts(11.0, 14.0, 600.0, false);
    pub const MONO: TypeStyle = ts(12.0, 16.0, 400.0, true);
}

/// Geometry tokens (§2.4). Radius is always zero, so there is no radius token.
pub mod space {
    use gpui::{Pixels, px};

    pub const S2: Pixels = px(2.0);
    pub const S4: Pixels = px(4.0);
    pub const S6: Pixels = px(6.0);
    pub const S8: Pixels = px(8.0);
    pub const S12: Pixels = px(12.0);
    pub const S16: Pixels = px(16.0);
    pub const S20: Pixels = px(20.0);
    pub const S24: Pixels = px(24.0);
    pub const S32: Pixels = px(32.0);
    pub const S48: Pixels = px(48.0);

    pub const TOOLBAR_HEIGHT: Pixels = px(48.0);
    pub const CONTROL_HEIGHT: Pixels = px(28.0);
    pub const CONTROL_HEIGHT_LARGE: Pixels = px(32.0);
    pub const ICON: Pixels = px(16.0);
    pub const ICON_TARGET: Pixels = px(28.0);
    pub const ROW: Pixels = px(32.0);
    pub const ROW_TWO_LINE: Pixels = px(56.0);
    pub const CHIP_HEIGHT: Pixels = px(20.0);
    pub const GRID_INSET: Pixels = px(20.0);
    pub const GRID_GAP: Pixels = px(12.0);
    pub const HAIRLINE: Pixels = px(1.0);
    pub const FOCUS_RING: Pixels = px(2.0);
    pub const ACTION_COLUMN: Pixels = px(400.0);
    pub const SHEET_WIDTH: Pixels = px(560.0);
    pub const POPOVER_WIDTH: Pixels = px(380.0);
}

/// Global theme state: effective mode + palette + reduced-motion preference.
#[derive(Clone, Debug)]
pub struct Theme {
    pub mode: Mode,
    pub palette: Palette,
    /// Effective reduced motion (OS request or Settings override).
    pub reduce_motion: bool,
}

impl Global for Theme {}

impl Theme {
    pub fn new(mode: Mode, reduce_motion: bool) -> Self {
        let palette = match mode {
            Mode::Dark => Palette::dark(),
            Mode::Light => Palette::light(),
        };
        Self {
            mode,
            palette,
            reduce_motion,
        }
    }
}

/// Install the v3 theme global (dark, full motion) if absent.
pub fn init(cx: &mut App) {
    if !cx.has_global::<Theme>() {
        cx.set_global(Theme::new(Mode::Dark, false));
    }
}

pub fn theme(cx: &App) -> &Theme {
    cx.global::<Theme>()
}

pub fn palette(cx: &App) -> &Palette {
    &cx.global::<Theme>().palette
}

pub fn set_mode(mode: Mode, cx: &mut App) {
    let reduce = cx
        .try_global::<Theme>()
        .map(|t| t.reduce_motion)
        .unwrap_or(false);
    cx.set_global(Theme::new(mode, reduce));
    cx.refresh_windows();
}

pub fn set_reduce_motion(reduce: bool, cx: &mut App) {
    let mode = cx.try_global::<Theme>().map(|t| t.mode).unwrap_or_default();
    cx.set_global(Theme::new(mode, reduce));
    cx.refresh_windows();
}

/// Resolve System against GPUI's current native window appearance.
pub fn resolve_mode(
    choice: crate::settings::ThemeChoice,
    appearance: gpui::WindowAppearance,
) -> Mode {
    use crate::settings::ThemeChoice;
    match choice {
        ThemeChoice::Dark => Mode::Dark,
        ThemeChoice::Light => Mode::Light,
        ThemeChoice::System => match appearance {
            gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark => Mode::Dark,
            _ => Mode::Light,
        },
    }
}

#[derive(Clone, Copy)]
struct AppearancePreference(crate::settings::ThemeChoice);
impl Global for AppearancePreference {}

/// gpui-pre 0.3.9 exposes no OS reduced-motion preference. System therefore
/// resolves to false; Always still settles running widget animations instantly.
pub fn apply_preferences(
    theme: crate::settings::ThemeChoice,
    motion: crate::settings::MotionChoice,
    cx: &mut App,
) {
    cx.set_global(AppearancePreference(theme));
    let mode = resolve_mode(theme, cx.window_appearance());
    if cx
        .try_global::<Theme>()
        .is_none_or(|current| current.mode != mode)
    {
        set_mode(mode, cx);
    }
    let reduce = motion == crate::settings::MotionChoice::Always;
    if cx
        .try_global::<Theme>()
        .is_none_or(|current| current.reduce_motion != reduce)
    {
        set_reduce_motion(reduce, cx);
    }
}

/// Keep this subscription for the lifetime of a native window. Register on the
/// main window as well, so System continues following the OS with Settings closed.
pub fn observe_appearance(window: &gpui::Window) -> gpui::Subscription {
    window.observe_window_appearance(|_, cx| {
        if let Some(preference) = cx.try_global::<AppearancePreference>() {
            let mode = resolve_mode(preference.0, cx.window_appearance());
            set_mode(mode, cx);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_appearance_and_explicit_override() {
        use crate::settings::ThemeChoice;
        use gpui::WindowAppearance;
        assert_eq!(
            resolve_mode(ThemeChoice::System, WindowAppearance::VibrantDark),
            Mode::Dark
        );
        assert_eq!(
            resolve_mode(ThemeChoice::System, WindowAppearance::Light),
            Mode::Light
        );
        assert_eq!(
            resolve_mode(ThemeChoice::Dark, WindowAppearance::Light),
            Mode::Dark
        );
        assert_eq!(
            resolve_mode(ThemeChoice::Light, WindowAppearance::Dark),
            Mode::Light
        );
    }

    #[test]
    fn dark_is_default_and_palettes_differ() {
        assert_eq!(Mode::default(), Mode::Dark);
        assert_ne!(Palette::dark().bg_canvas, Palette::light().bg_canvas);
        // Artwork scrims are theme-independent by design.
        assert_eq!(Palette::dark().scrim_max, Palette::light().scrim_max);
    }
}
