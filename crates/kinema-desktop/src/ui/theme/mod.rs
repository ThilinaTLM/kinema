//! Desktop v3 design tokens (see `docs/design/desktop-v3.md` §2).
//!
//! This is the only source of color, type and geometry for `ui` primitives, blocks
//! and views. Views must not use raw color or size literals. Pure values and
//! functions only; no GPUI elements.

use gpui::{App, Global};

pub mod color;
pub mod elevation;
pub mod layout;
pub mod motion;
pub mod opacity;
pub mod radius;
pub mod size;
pub mod space;
pub mod typography;

pub use color::{Palette, transparent};

/// Effective color scheme. Dark is the product default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Dark,
    Light,
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
    fn theme_new_selects_the_palette_for_the_mode() {
        let dark = Theme::new(Mode::Dark, false);
        let light = Theme::new(Mode::Light, true);
        assert_eq!(dark.palette.background, Palette::dark().background);
        assert_eq!(light.palette.background, Palette::light().background);
        assert!(light.reduce_motion && !dark.reduce_motion);
    }
}
