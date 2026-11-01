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
    /// Opaque menu, popover and sheet surface.
    pub bg_overlay: Hsla,
    /// Decorative raised-surface edge, not an essential control boundary.
    pub line_subtle: Hsla,
    pub shadow: Hsla,
    /// Contrast-safe secondary ink derived from artwork; never the focus color.
    pub art_tint: Hsla,
    /// Darkening layer over blurred backdrops; not a text-contrast guarantee.
    pub backdrop_scrim: Hsla,
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
            bg_overlay: c(0x202228),
            line_subtle: ca(0xFFFFFF14),
            shadow: ca(0x00000052),
            art_tint: c(0xFF9B72),
            backdrop_scrim: ca(0x0B0C0EB8),
            ctl_rest: c(0x24272D),
            ctl_hover: c(0x2D3037),
            ctl_pressed: c(0x373B43),
            line_hairline: c(0x25282E),
            line_control: c(0x646A75),
            text_primary: c(0xF2F3F5),
            text_secondary: c(0xB3B8C1),
            text_muted: c(0x8C929D),
            accent: c(0xFF7A45),
            accent_hover: c(0xFF915F),
            accent_pressed: c(0xF06A35),
            on_accent: c(0x21130D),
            accent_text: c(0xFF9B72),
            accent_tint: ca(0xFF7A4529),
            focus_ring: c(0xFF9B72),
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
            bg_overlay: c(0xFFFFFF),
            line_subtle: ca(0x1112151A),
            shadow: ca(0x11121524),
            art_tint: c(0xA83B12),
            backdrop_scrim: ca(0x0B0C0E99),
            ctl_rest: c(0xEAEAEE),
            ctl_hover: c(0xE1E2E7),
            ctl_pressed: c(0xD7D9DF),
            line_hairline: c(0xDCDDE3),
            line_control: c(0x868C97),
            text_primary: c(0x111215),
            text_secondary: c(0x4B505A),
            text_muted: c(0x5E646F),
            accent: c(0xB54118),
            accent_hover: c(0x9D3512),
            accent_pressed: c(0x852B0D),
            on_accent: c(0xFFFFFF),
            accent_text: c(0xA83B12),
            accent_tint: ca(0xB541181A),
            focus_ring: c(0xB54118),
            rating: c(0x8A5D00),
            status_ok: c(0x1B7045),
            status_warn: c(0x9A5400),
            status_error: c(0xB32626),
            scrim_max: ca(0x0B0C0EEB),
            chip_over_art: ca(0x0B0C0ECC),
            poster_edge: ca(0x00000014),
            dialog_scrim: ca(0x0B0C0E66),
        }
    }

    /// Derive restrained background washes and secondary artwork ink.
    ///
    /// `None` returns this palette unchanged. `mode` must match the palette's
    /// theme. Surface lightness is clamped to preserve primary/secondary text
    /// contrast; brand actions, selected-label ink and focus remain unchanged.
    /// This is pure and does not sample/decode artwork or mutate global state.
    pub fn tinted(&self, art: Option<crate::artwork::ArtworkPalette>, mode: Mode) -> Palette {
        let Some(art) = art else { return *self };
        let mut result = *self;
        let color = |rgb: [u8; 3]| c(u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]]));
        let primary = color(art.primary);
        let secondary = color(art.secondary);
        let strength = match mode {
            Mode::Dark => 0.14,
            Mode::Light => 0.07,
        };
        let wash = |base: Hsla| {
            let base: Rgba = base.into();
            let art: Rgba = primary.into();
            let mut wash: Hsla = Rgba {
                r: base.r * (1.0 - strength) + art.r * strength,
                g: base.g * (1.0 - strength) + art.g * strength,
                b: base.b * (1.0 - strength) + art.b * strength,
                a: 1.0,
            }
            .into();
            wash.l = match mode {
                Mode::Dark => wash.l.min(0.13),
                Mode::Light => wash.l.max(0.94),
            };
            wash
        };
        result.bg_canvas = wash(self.bg_canvas);
        result.bg_surface = wash(self.bg_surface);
        result.bg_raised = wash(self.bg_raised);
        result.bg_overlay = wash(self.bg_overlay);
        result.art_tint = Hsla {
            h: secondary.h,
            s: secondary.s.min(0.65),
            l: match mode {
                Mode::Dark => 0.76,
                Mode::Light => 0.24,
            },
            a: 1.0,
        };
        result.accent_tint = Hsla {
            a: self.accent_tint.a,
            ..result.art_tint
        };
        result
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

/// Compact spacing and sizing tokens (§2.4); corner radii live in [`radius`].
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

    /// Side, top and bottom inset of every main-window page.
    pub const PAGE_INSET: Pixels = px(20.0);
    /// Space above the floating-header controls; matches the gap below them
    /// to page content ([`PAGE_INSET`]) so the header row reads balanced.
    pub const TOOLBAR_TOP: Pixels = PAGE_INSET;
    /// Floating header: top padding plus one control row.
    pub const TOOLBAR_HEIGHT: Pixels = px(52.0);
    /// Where page content starts beneath the floating header.
    pub const PAGE_TOP: Pixels = px(72.0);
    pub const CONTROL_HEIGHT: Pixels = px(28.0);
    pub const CONTROL_HEIGHT_LARGE: Pixels = px(32.0);
    /// Every floating-header control (chips, segmented, search, icon groups).
    pub const TOOLBAR_CONTROL: Pixels = px(32.0);
    pub const ICON: Pixels = px(16.0);
    pub const ICON_TARGET: Pixels = px(28.0);
    pub const ROW: Pixels = px(32.0);
    pub const ROW_TWO_LINE: Pixels = px(56.0);
    pub const CHIP_HEIGHT: Pixels = px(20.0);
    pub const GRID_INSET: Pixels = PAGE_INSET;
    pub const GRID_GAP: Pixels = px(12.0);
    pub const HAIRLINE: Pixels = px(1.0);
    pub const FOCUS_RING: Pixels = px(2.0);
    pub const ACTION_COLUMN: Pixels = px(400.0);
    pub const SHEET_WIDTH: Pixels = px(560.0);
    pub const POPOVER_WIDTH: Pixels = px(380.0);
}

/// Corner geometry (§2.4). Focus rings follow their host's radius.
pub mod radius {
    use gpui::{Pixels, px};

    /// Chips and badges.
    pub const XS: Pixels = px(4.0);
    /// Controls, inputs, menu items and list-row hover.
    pub const SM: Pixels = px(6.0);
    /// Cards and posters.
    pub const MD: Pixels = px(8.0);
    /// Sheets, popovers and menus.
    pub const LG: Pixels = px(12.0);
}

/// Soft shadows only for raised surfaces; pass directly to `Styled::shadow`.
pub mod elevation {
    use super::Palette;
    use gpui::{BoxShadow, px};

    fn soft(palette: &Palette, offset: f32, blur: f32) -> Vec<BoxShadow> {
        vec![BoxShadow::new(px(0.0), px(offset), palette.shadow).blur_radius(px(blur))]
    }

    pub fn raised(palette: &Palette) -> Vec<BoxShadow> {
        soft(palette, 2.0, 8.0)
    }

    pub fn overlay(palette: &Palette) -> Vec<BoxShadow> {
        soft(palette, 6.0, 20.0)
    }

    pub fn sheet(palette: &Palette) -> Vec<BoxShadow> {
        soft(palette, 12.0, 36.0)
    }

    pub fn card_hover(palette: &Palette) -> Vec<BoxShadow> {
        soft(palette, 4.0, 14.0)
    }
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

    fn luminance(color: Hsla) -> f32 {
        let color: Rgba = color.into();
        let linear = |c: f32| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn assert_text_contrast(p: &Palette) {
        for surface in [
            p.bg_canvas,
            p.bg_surface,
            p.bg_raised,
            p.bg_overlay,
            p.bg_placeholder,
        ] {
            for ink in [p.text_primary, p.text_secondary, p.art_tint, p.accent_text] {
                assert!(
                    contrast(ink, surface) >= 4.5,
                    "{ink:?} on {surface:?}: {}",
                    contrast(ink, surface)
                );
            }
        }
        for surface in [p.bg_canvas, p.bg_surface, p.bg_raised, p.bg_overlay] {
            for ink in [p.status_ok, p.status_warn, p.status_error] {
                assert!(
                    contrast(ink, surface) >= 4.5,
                    "status {ink:?} on {surface:?}: {}",
                    contrast(ink, surface)
                );
            }
        }
        for fill in [p.accent, p.accent_hover, p.accent_pressed] {
            assert!(contrast(p.on_accent, fill) >= 4.5);
        }
    }

    #[test]
    fn status_badge_washes_preserve_aa_on_canvas_and_surfaces() {
        for p in [Palette::dark(), Palette::light()] {
            for surface in [p.bg_canvas, p.bg_surface, p.bg_raised, p.bg_overlay] {
                for ink in [p.status_ok, p.status_warn, p.status_error] {
                    let fill = surface.blend(ink.opacity(0.10));
                    assert!(
                        contrast(ink, fill) >= 4.5,
                        "status badge {ink:?} on {fill:?}: {}",
                        contrast(ink, fill)
                    );
                }
            }
        }
    }

    #[test]
    fn radius_scale_is_ordered() {
        assert_eq!(
            [radius::XS, radius::SM, radius::MD, radius::LG],
            [px(4.0), px(6.0), px(8.0), px(12.0)]
        );
        assert!(radius::XS < radius::SM && radius::SM < radius::MD && radius::MD < radius::LG);
    }

    #[test]
    fn palettes_have_aa_text_and_primary_action_contrast() {
        for palette in [Palette::dark(), Palette::light()] {
            assert_text_contrast(&palette);
            for surface in [
                palette.bg_canvas,
                palette.bg_surface,
                palette.bg_raised,
                palette.bg_overlay,
            ] {
                assert!(contrast(palette.focus_ring, surface) >= 3.0);
            }
            assert_eq!(palette.art_tint, palette.accent_text);
            for shadow in [
                elevation::raised(&palette),
                elevation::overlay(&palette),
                elevation::sheet(&palette),
                elevation::card_hover(&palette),
            ] {
                assert_eq!(shadow.len(), 1);
                assert_eq!(shadow[0].color, palette.shadow);
                assert!(!shadow[0].inset);
            }
        }
    }

    #[test]
    fn artwork_tint_is_bounded_and_preserves_brand_and_contrast() {
        for (mode, base) in [
            (Mode::Dark, Palette::dark()),
            (Mode::Light, Palette::light()),
        ] {
            assert_eq!(base.tinted(None, mode).bg_canvas, base.bg_canvas);
            assert_eq!(base.tinted(None, mode).art_tint, base.art_tint);
            // Sample RGB cube corners and intermediate hues, including extremes.
            for r in [0, 64, 128, 192, 255] {
                for g in [0, 64, 128, 192, 255] {
                    for b in [0, 64, 128, 192, 255] {
                        let art = crate::artwork::ArtworkPalette {
                            primary: [r, g, b],
                            secondary: [b, r, g],
                        };
                        let tinted = base.tinted(Some(art), mode);
                        assert_text_contrast(&tinted);
                        assert_eq!(tinted.accent, base.accent);
                        assert_eq!(tinted.accent_hover, base.accent_hover);
                        assert_eq!(tinted.accent_pressed, base.accent_pressed);
                        assert_eq!(tinted.on_accent, base.on_accent);
                        assert_eq!(tinted.accent_text, base.accent_text);
                        assert_eq!(tinted.focus_ring, base.focus_ring);
                        for wash in [
                            tinted.bg_canvas,
                            tinted.bg_surface,
                            tinted.bg_raised,
                            tinted.bg_overlay,
                        ] {
                            assert!(match mode {
                                Mode::Dark => wash.l <= 0.13,
                                Mode::Light => wash.l >= 0.94,
                            });
                            assert_eq!(wash.a, 1.0);
                        }
                    }
                }
            }
        }
    }

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

    #[test]
    fn header_row_is_balanced_against_the_page_inset() {
        use super::space::*;
        assert_eq!(TOOLBAR_HEIGHT, TOOLBAR_TOP + TOOLBAR_CONTROL);
        assert_eq!(PAGE_TOP, TOOLBAR_HEIGHT + PAGE_INSET);
        // Same space above the header controls as between them and content.
        assert_eq!(TOOLBAR_TOP, PAGE_TOP - TOOLBAR_HEIGHT);
    }
}
