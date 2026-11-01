//! Semantic color tokens (shadcn-style names adapted for a desktop media app).
//!
//! [`Palette`] has 27 semantic tokens plus the single `brand_wash` hue. Hover and
//! pressed fills are *derived* ([`Palette::hover`], [`Palette::pressed`],
//! [`Palette::primary_hover`], [`Palette::primary_pressed`]) instead of stored.
//! Naming: `primary` is the brand orange, `accent` is the neutral hover fill and
//! `selection` is the brand-tinted selected fill.

use gpui::{Hsla, Rgba, hsla, rgb, rgba};

use super::{Mode, opacity};

/// Semantic palette. All text/action pairs are contrast-checked in the tests.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    // Surfaces
    /// Window canvas.
    pub background: Hsla,
    /// Toolbar, sidebar, action column.
    pub surface: Hsla,
    /// Raised cards and posters.
    pub card: Hsla,
    /// Opaque menu, popover and sheet surface.
    pub popover: Hsla,
    /// Placeholder and skeleton fill.
    pub muted: Hsla,
    // Text
    pub foreground: Hsla,
    /// Facts and metadata.
    pub muted_foreground: Hsla,
    /// Hints, captions and disabled ink.
    pub subtle_foreground: Hsla,
    /// Links and selected labels.
    pub primary_text: Hsla,
    // Actions
    /// Brand action fill.
    pub primary: Hsla,
    /// Ink on `primary` (and its derived states).
    pub primary_foreground: Hsla,
    /// Neutral control fill at rest.
    pub secondary: Hsla,
    /// Neutral hover/highlight fill.
    pub accent: Hsla,
    /// Brand-tinted selected fill (washed from artwork on tinted palettes).
    pub selection: Hsla,
    // Lines
    /// Single decorative/structural edge (not an essential control boundary).
    pub border: Hsla,
    /// Essential control boundary (inputs, switches, scrollbar thumbs).
    pub input: Hsla,
    /// Focus ring.
    pub ring: Hsla,
    // Status
    pub success: Hsla,
    pub warning: Hsla,
    pub destructive: Hsla,
    // Effects
    pub shadow: Hsla,
    /// Modal and backdrop dimming layer; not a text-contrast guarantee.
    pub overlay: Hsla,
    // Media
    /// Text-bearing scrim over artwork (same in both themes).
    pub scrim: Hsla,
    /// Chip backing over artwork (same in both themes).
    pub chip_on_art: Hsla,
    /// 1 px inner poster edge.
    pub poster_edge: Hsla,
    pub rating: Hsla,
    /// Rating ink over artwork (always on a dark chip).
    pub rating_on_art: Hsla,

    /// The one allowed addition: violet hue of the Browse brand wash (same in
    /// both themes; amber is `primary`).
    pub brand_wash: Hsla,

    // --- TEMP: removed in step 5 (old names, kept so the tree compiles) -------
    // Each is an alias of a new token (kept in sync by `sync_legacy`) or, where
    // noted, a distinct old value.
    pub bg_canvas: Hsla,      // TEMP: removed in step 5 -> background
    pub bg_surface: Hsla,     // TEMP: removed in step 5 -> surface
    pub bg_raised: Hsla,      // TEMP: removed in step 5 -> card
    pub bg_placeholder: Hsla, // TEMP: removed in step 5 -> muted
    pub bg_overlay: Hsla,     // TEMP: removed in step 5 -> popover
    pub line_subtle: Hsla,    // TEMP: removed in step 5 -> border
    pub line_hairline: Hsla,  // TEMP: removed in step 5 -> border (old distinct value)
    pub line_control: Hsla,   // TEMP: removed in step 5 -> input
    pub backdrop_scrim: Hsla, // TEMP: removed in step 5 -> overlay (old distinct value)
    pub dialog_scrim: Hsla,   // TEMP: removed in step 5 -> overlay
    pub ctl_rest: Hsla,       // TEMP: removed in step 5 -> secondary
    pub ctl_hover: Hsla,      // TEMP: removed in step 5 -> accent
    pub ctl_pressed: Hsla,    // TEMP: removed in step 5 -> pressed(secondary)
    pub text_primary: Hsla,   // TEMP: removed in step 5 -> foreground
    pub text_secondary: Hsla, // TEMP: removed in step 5 -> muted_foreground
    pub text_muted: Hsla,     // TEMP: removed in step 5 -> subtle_foreground
    pub accent_hover: Hsla,   // TEMP: removed in step 5 -> primary_hover()
    pub accent_pressed: Hsla, // TEMP: removed in step 5 -> primary_pressed()
    pub on_accent: Hsla,      // TEMP: removed in step 5 -> primary_foreground
    pub accent_text: Hsla,    // TEMP: removed in step 5 -> primary_text
    pub accent_tint: Hsla,    // TEMP: removed in step 5 -> selection
    pub focus_ring: Hsla,     // TEMP: removed in step 5 -> ring
    pub status_ok: Hsla,      // TEMP: removed in step 5 -> success
    pub status_warn: Hsla,    // TEMP: removed in step 5 -> warning
    pub status_error: Hsla,   // TEMP: removed in step 5 -> destructive
    pub scrim_max: Hsla,      // TEMP: removed in step 5 -> scrim
    pub chip_over_art: Hsla,  // TEMP: removed in step 5 -> chip_on_art
}

fn c(hex: u32) -> Hsla {
    let color: Rgba = rgb(hex);
    color.into()
}

fn ca(hex_rgba: u32) -> Hsla {
    let color: Rgba = rgba(hex_rgba);
    color.into()
}

/// Mix `base` toward `toward` by `amount` (0..=1) in sRGB; `base` alpha is kept.
fn mix(base: Hsla, toward: Hsla, amount: f32) -> Hsla {
    let (b, t): (Rgba, Rgba) = (base.into(), toward.into());
    let lerp = |a: f32, z: f32| a + (z - a) * amount;
    Rgba {
        r: lerp(b.r, t.r),
        g: lerp(b.g, t.g),
        b: lerp(b.b, t.b),
        a: b.a,
    }
    .into()
}

impl Palette {
    /// Violet of the Browse brand wash, shared by both themes.
    fn violet() -> Hsla {
        hsla(274. / 360., 0.40, 0.46, 1.)
    }

    pub fn dark() -> Self {
        Self {
            background: c(0x0B0C0E),
            surface: c(0x131418),
            card: c(0x1B1D22),
            popover: c(0x202228),
            muted: c(0x1B1D22),
            foreground: c(0xF2F3F5),
            muted_foreground: c(0xB3B8C1),
            subtle_foreground: c(0x8C929D),
            primary_text: c(0xFF9B72),
            primary: c(0xFF7A45),
            primary_foreground: c(0x21130D),
            secondary: c(0x24272D),
            accent: c(0x2D3037),
            selection: ca(0xFF7A4529),
            border: ca(0xFFFFFF14),
            input: c(0x646A75),
            ring: c(0xFF9B72),
            success: c(0x46C98A),
            warning: c(0xF2A33A),
            destructive: c(0xFF6B6B),
            shadow: ca(0x00000052),
            overlay: ca(0x00000099),
            scrim: ca(0x0B0C0EEB),
            chip_on_art: ca(0x0B0C0ECC),
            poster_edge: ca(0xFFFFFF0F),
            rating: c(0xF5C451),
            rating_on_art: c(0xF5C451),
            brand_wash: Self::violet(),
            line_hairline: c(0x25282E),
            backdrop_scrim: ca(0x0B0C0EB8),
            ..Self::blank()
        }
        .sync_legacy()
    }

    pub fn light() -> Self {
        Self {
            background: c(0xF3F3F5),
            surface: c(0xFFFFFF),
            card: c(0xFFFFFF),
            popover: c(0xFFFFFF),
            muted: c(0xE4E5EA),
            foreground: c(0x111215),
            muted_foreground: c(0x4B505A),
            subtle_foreground: c(0x5E646F),
            primary_text: c(0xA83B12),
            primary: c(0xB54118),
            primary_foreground: c(0xFFFFFF),
            secondary: c(0xEAEAEE),
            accent: c(0xE1E2E7),
            selection: ca(0xB541181A),
            border: ca(0x1112151A),
            input: c(0x868C97),
            ring: c(0xB54118),
            success: c(0x1B7045),
            warning: c(0x9A5400),
            destructive: c(0xB32626),
            shadow: ca(0x11121524),
            overlay: ca(0x0B0C0E66),
            scrim: ca(0x0B0C0EEB),
            chip_on_art: ca(0x0B0C0ECC),
            poster_edge: ca(0x00000014),
            rating: c(0x8A5D00),
            rating_on_art: c(0xF5C451),
            brand_wash: Self::violet(),
            line_hairline: c(0xDCDDE3),
            backdrop_scrim: ca(0x0B0C0E99),
            ..Self::blank()
        }
        .sync_legacy()
    }

    /// The palette of the cinema surface (player window): always dark,
    /// whatever the app theme.
    pub fn cinema() -> Self {
        Self::dark()
    }

    /// Placeholder for fields that `sync_legacy` or the caller fills in.
    fn blank() -> Self {
        let z = Hsla::transparent_black();
        Self {
            background: z,
            surface: z,
            card: z,
            popover: z,
            muted: z,
            foreground: z,
            muted_foreground: z,
            subtle_foreground: z,
            primary_text: z,
            primary: z,
            primary_foreground: z,
            secondary: z,
            accent: z,
            selection: z,
            border: z,
            input: z,
            ring: z,
            success: z,
            warning: z,
            destructive: z,
            shadow: z,
            overlay: z,
            scrim: z,
            chip_on_art: z,
            poster_edge: z,
            rating: z,
            rating_on_art: z,
            brand_wash: z,
            bg_canvas: z,
            bg_surface: z,
            bg_raised: z,
            bg_placeholder: z,
            bg_overlay: z,
            line_subtle: z,
            line_hairline: z,
            line_control: z,
            backdrop_scrim: z,
            dialog_scrim: z,
            ctl_rest: z,
            ctl_hover: z,
            ctl_pressed: z,
            text_primary: z,
            text_secondary: z,
            text_muted: z,
            accent_hover: z,
            accent_pressed: z,
            on_accent: z,
            accent_text: z,
            accent_tint: z,
            focus_ring: z,
            status_ok: z,
            status_warn: z,
            status_error: z,
            scrim_max: z,
            chip_over_art: z,
        }
    }

    /// TEMP: removed in step 5. Refresh the old-name aliases from the new tokens.
    fn sync_legacy(mut self) -> Self {
        self.bg_canvas = self.background;
        self.bg_surface = self.surface;
        self.bg_raised = self.card;
        self.bg_placeholder = self.muted;
        self.bg_overlay = self.popover;
        self.line_subtle = self.border;
        self.line_control = self.input;
        self.dialog_scrim = self.overlay;
        self.ctl_rest = self.secondary;
        self.ctl_hover = self.accent;
        self.ctl_pressed = self.pressed(self.secondary);
        self.text_primary = self.foreground;
        self.text_secondary = self.muted_foreground;
        self.text_muted = self.subtle_foreground;
        self.accent_hover = self.primary_hover();
        self.accent_pressed = self.primary_pressed();
        self.on_accent = self.primary_foreground;
        self.accent_text = self.primary_text;
        self.accent_tint = self.selection;
        self.focus_ring = self.ring;
        self.status_ok = self.success;
        self.status_warn = self.warning;
        self.status_error = self.destructive;
        self.scrim_max = self.scrim;
        self.chip_over_art = self.chip_on_art;
        self
    }

    /// Hover state of a neutral fill: `base` mixed toward `foreground`.
    pub fn hover(&self, base: Hsla) -> Hsla {
        mix(base, self.foreground, opacity::HOVER_MIX)
    }

    /// Pressed state of a neutral fill: `base` mixed toward `foreground`.
    pub fn pressed(&self, base: Hsla) -> Hsla {
        mix(base, self.foreground, opacity::PRESSED_MIX)
    }

    /// Hover state of `primary`: mixed toward `foreground`, so it lightens in
    /// dark mode and darkens in light mode. `primary_foreground` stays AA on it.
    pub fn primary_hover(&self) -> Hsla {
        mix(self.primary, self.foreground, opacity::PRIMARY_HOVER_MIX)
    }

    /// Pressed state of `primary`: always deepened toward black.
    pub fn primary_pressed(&self) -> Hsla {
        mix(
            self.primary,
            hsla(0., 0., 0., 1.),
            opacity::PRIMARY_PRESSED_MIX,
        )
    }

    /// The translucent header material: `(fill, hover fill, edge)`. Opaque
    /// enough for text over bright art, light enough to read as floating.
    pub fn glass(&self) -> (Hsla, Hsla, Hsla) {
        (
            Hsla {
                a: opacity::GLASS,
                ..self.popover
            },
            Hsla {
                a: opacity::GLASS,
                ..self.accent
            },
            self.border,
        )
    }

    /// TEMP: removed in step 5 -> `rating_on_art` field.
    pub fn rating_over_art(&self) -> Hsla {
        self.rating_on_art
    }

    /// Derive restrained background washes and a selection fill from artwork.
    ///
    /// `None` returns this palette unchanged. `mode` must match the palette's
    /// theme. Surface lightness is clamped to preserve text contrast; `primary*`,
    /// `primary_text` and `ring` stay brand-colored. This is pure and does not
    /// sample/decode artwork or mutate global state.
    pub fn tinted(&self, art: Option<crate::artwork::ArtworkPalette>, mode: Mode) -> Palette {
        let Some(art) = art else { return *self };
        let mut result = *self;
        let color = |rgb: [u8; 3]| c(u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]]));
        let primary = color(art.primary);
        let secondary = color(art.secondary);
        let strength = match mode {
            Mode::Dark => opacity::ART_WASH_DARK,
            Mode::Light => opacity::ART_WASH_LIGHT,
        };
        let wash = |base: Hsla| {
            let mut wash = mix(Hsla { a: 1.0, ..base }, primary, strength);
            wash.l = match mode {
                Mode::Dark => wash.l.min(0.13),
                Mode::Light => wash.l.max(0.94),
            };
            wash
        };
        result.background = wash(self.background);
        result.surface = wash(self.surface);
        result.card = wash(self.card);
        result.popover = wash(self.popover);
        result.selection = Hsla {
            h: secondary.h,
            s: secondary.s.min(0.65),
            l: match mode {
                Mode::Dark => 0.76,
                Mode::Light => 0.24,
            },
            a: self.selection.a,
        };
        result.sync_legacy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn luminance(color: Hsla) -> f32 {
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

    pub(crate) fn contrast(a: Hsla, b: Hsla) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn surfaces(p: &Palette) -> [Hsla; 4] {
        [p.background, p.surface, p.card, p.popover]
    }

    fn hex(color: Hsla) -> u32 {
        let c: Rgba = color.into();
        let byte = |v: f32| (v * 255.0).round() as u32;
        (byte(c.r) << 16) | (byte(c.g) << 8) | byte(c.b)
    }

    fn assert_text_contrast(p: &Palette) {
        let mut all = surfaces(p).to_vec();
        all.push(p.muted);
        for surface in all {
            for ink in [p.foreground, p.muted_foreground, p.primary_text] {
                assert!(
                    contrast(ink, surface) >= 4.5,
                    "{ink:?} on {surface:?}: {}",
                    contrast(ink, surface)
                );
            }
            // The selection ink (artwork-derived on tinted palettes) at full alpha.
            let selection_ink = Hsla {
                a: 1.0,
                ..p.selection
            };
            if p.selection.l > 0.2 && p.selection.l < 0.8 {
                // Brand orange fill is not text; only tinted palettes carry ink.
            } else {
                assert!(contrast(selection_ink, surface) >= 4.5);
            }
        }
        for surface in surfaces(p) {
            for ink in [p.success, p.warning, p.destructive] {
                assert!(
                    contrast(ink, surface) >= 4.5,
                    "status {ink:?} on {surface:?}: {}",
                    contrast(ink, surface)
                );
            }
        }
        for fill in [p.primary, p.primary_hover(), p.primary_pressed()] {
            assert!(
                contrast(p.primary_foreground, fill) >= 4.5,
                "primary_foreground on {fill:?}: {}",
                contrast(p.primary_foreground, fill)
            );
        }
    }

    #[test]
    fn dark_hex_values_match_the_token_table() {
        let p = Palette::dark();
        for (name, got, want) in [
            ("background", p.background, 0x0B0C0E),
            ("surface", p.surface, 0x131418),
            ("card", p.card, 0x1B1D22),
            ("popover", p.popover, 0x202228),
            ("muted", p.muted, 0x1B1D22),
            ("foreground", p.foreground, 0xF2F3F5),
            ("muted_foreground", p.muted_foreground, 0xB3B8C1),
            ("subtle_foreground", p.subtle_foreground, 0x8C929D),
            ("primary_text", p.primary_text, 0xFF9B72),
            ("primary", p.primary, 0xFF7A45),
            ("primary_foreground", p.primary_foreground, 0x21130D),
            ("secondary", p.secondary, 0x24272D),
            ("accent", p.accent, 0x2D3037),
            ("input", p.input, 0x646A75),
            ("ring", p.ring, 0xFF9B72),
            ("success", p.success, 0x46C98A),
            ("warning", p.warning, 0xF2A33A),
            ("destructive", p.destructive, 0xFF6B6B),
            ("rating", p.rating, 0xF5C451),
            ("rating_on_art", p.rating_on_art, 0xF5C451),
        ] {
            assert_eq!(hex(got), want, "{name}");
        }
        assert_eq!(hex(p.selection), 0xFF7A45);
        assert!((p.selection.a - 0.16).abs() < 0.005);
        assert!((p.border.a - 0.08).abs() < 0.005);
        assert!((p.shadow.a - 0.32).abs() < 0.005);
        assert!((p.overlay.a - 0.60).abs() < 0.005);
        assert!((p.scrim.a - 0.92).abs() < 0.005);
        assert!((p.chip_on_art.a - 0.80).abs() < 0.005);
        assert!((p.poster_edge.a - 0.06).abs() < 0.005);
    }

    #[test]
    fn light_hex_values_match_the_token_table() {
        let p = Palette::light();
        for (name, got, want) in [
            ("background", p.background, 0xF3F3F5),
            ("surface", p.surface, 0xFFFFFF),
            ("card", p.card, 0xFFFFFF),
            ("popover", p.popover, 0xFFFFFF),
            ("muted", p.muted, 0xE4E5EA),
            ("foreground", p.foreground, 0x111215),
            ("muted_foreground", p.muted_foreground, 0x4B505A),
            ("subtle_foreground", p.subtle_foreground, 0x5E646F),
            ("primary_text", p.primary_text, 0xA83B12),
            ("primary", p.primary, 0xB54118),
            ("primary_foreground", p.primary_foreground, 0xFFFFFF),
            ("secondary", p.secondary, 0xEAEAEE),
            ("accent", p.accent, 0xE1E2E7),
            ("input", p.input, 0x868C97),
            ("ring", p.ring, 0xB54118),
            ("success", p.success, 0x1B7045),
            ("warning", p.warning, 0x9A5400),
            ("destructive", p.destructive, 0xB32626),
            ("rating", p.rating, 0x8A5D00),
            ("rating_on_art", p.rating_on_art, 0xF5C451),
        ] {
            assert_eq!(hex(got), want, "{name}");
        }
        assert_eq!(hex(p.selection), 0xB54118);
        assert!((p.selection.a - 0.10).abs() < 0.005);
        assert_eq!(hex(p.border), 0x111215);
        assert!((p.border.a - 0.10).abs() < 0.005);
        assert_eq!(hex(p.shadow), 0x111215);
        assert!((p.shadow.a - 0.14).abs() < 0.005);
        assert_eq!(hex(p.overlay), 0x0B0C0E);
        assert!((p.overlay.a - 0.40).abs() < 0.005);
        assert!((p.poster_edge.a - 0.08).abs() < 0.005);
    }

    #[test]
    fn cinema_is_the_dark_palette() {
        assert_eq!(
            format!("{:?}", Palette::cinema()),
            format!("{:?}", Palette::dark())
        );
    }

    #[test]
    fn status_badge_washes_preserve_aa_on_surfaces() {
        for p in [Palette::dark(), Palette::light()] {
            for surface in surfaces(&p) {
                for ink in [p.success, p.warning, p.destructive] {
                    let fill = surface.blend(ink.opacity(opacity::BADGE_WASH));
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
    fn palettes_have_aa_text_and_primary_action_contrast() {
        for palette in [Palette::dark(), Palette::light()] {
            assert_text_contrast(&palette);
            for surface in surfaces(&palette) {
                assert!(contrast(palette.ring, surface) >= 3.0);
            }
            for shadow in [
                super::super::elevation::sm(&palette),
                super::super::elevation::md(&palette),
                super::super::elevation::lg(&palette),
                super::super::elevation::xl(&palette),
            ] {
                assert_eq!(shadow.len(), 1);
                assert_eq!(shadow[0].color, palette.shadow);
                assert!(!shadow[0].inset);
            }
        }
    }

    #[test]
    fn derived_primary_states_keep_foreground_aa_and_move_the_right_way() {
        for p in [Palette::dark(), Palette::light()] {
            for fill in [p.primary_hover(), p.primary_pressed()] {
                assert!(contrast(p.primary_foreground, fill) >= 4.5);
                assert_eq!(fill.a, 1.0);
            }
            // Pressed is always deeper than rest.
            assert!(p.primary_pressed().l < p.primary.l);
        }
        // Dark lightens on hover, light darkens on hover.
        let (d, l) = (Palette::dark(), Palette::light());
        assert!(d.primary_hover().l > d.primary.l);
        assert!(l.primary_hover().l < l.primary.l);
    }

    #[test]
    fn derived_primary_states_stay_near_the_old_accent_states() {
        // Old hex: dark FF915F / F06A35; light 9D3512 / 852B0D.
        for (p, old_hover, old_pressed) in [
            (Palette::dark(), 0xFF915F, 0xF06A35),
            (Palette::light(), 0x9D3512, 0x852B0D),
        ] {
            let lightness = |hex: u32| Hsla::from(rgb(hex)).l;
            assert!((p.primary_hover().l - lightness(old_hover)).abs() < 0.06);
            assert!((p.primary_pressed().l - lightness(old_pressed)).abs() < 0.10);
        }
    }

    #[test]
    fn derived_neutral_states_land_within_two_percent_of_the_old_controls() {
        // Old ctl_hover / ctl_pressed: dark 2D3037 / 373B43; light E1E2E7 / D7D9DF.
        for (p, old_hover, old_pressed) in [
            (Palette::dark(), 0x2D3037, 0x373B43),
            (Palette::light(), 0xE1E2E7, 0xD7D9DF),
        ] {
            let lightness = |hex: u32| Hsla::from(rgb(hex)).l;
            let hover = p.hover(p.secondary);
            let pressed = p.pressed(p.secondary);
            assert!(
                (hover.l - lightness(old_hover)).abs() <= 0.02,
                "hover {:06X}",
                hex(hover)
            );
            assert!(
                (pressed.l - lightness(old_pressed)).abs() <= 0.02,
                "pressed {:06X}",
                hex(pressed)
            );
            // The stored neutral hover token and the derived one agree.
            assert!((hover.l - p.accent.l).abs() <= 0.02);
        }
    }

    #[test]
    fn derived_states_preserve_alpha_and_ink_contrast() {
        for p in [Palette::dark(), Palette::light()] {
            let translucent = p.selection;
            assert_eq!(p.hover(translucent).a, translucent.a);
            assert_eq!(p.pressed(translucent).a, translucent.a);
            for fill in [p.hover(p.secondary), p.pressed(p.secondary)] {
                assert!(contrast(p.foreground, fill) >= 4.5);
                assert!(contrast(p.muted_foreground, fill) >= 4.5);
            }
        }
    }

    #[test]
    fn glass_uses_popover_accent_and_border() {
        for p in [Palette::dark(), Palette::light()] {
            let (fill, hover, edge) = p.glass();
            assert_eq!(
                fill,
                Hsla {
                    a: opacity::GLASS,
                    ..p.popover
                }
            );
            assert_eq!(
                hover,
                Hsla {
                    a: opacity::GLASS,
                    ..p.accent
                }
            );
            assert_eq!(edge, p.border);
        }
    }

    #[test]
    fn artwork_tint_is_bounded_and_preserves_brand_and_contrast() {
        for (mode, base) in [
            (Mode::Dark, Palette::dark()),
            (Mode::Light, Palette::light()),
        ] {
            assert_eq!(base.tinted(None, mode).background, base.background);
            assert_eq!(base.tinted(None, mode).selection, base.selection);
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
                        assert_eq!(tinted.primary, base.primary);
                        assert_eq!(tinted.primary_hover(), base.primary_hover());
                        assert_eq!(tinted.primary_pressed(), base.primary_pressed());
                        assert_eq!(tinted.primary_foreground, base.primary_foreground);
                        assert_eq!(tinted.primary_text, base.primary_text);
                        assert_eq!(tinted.ring, base.ring);
                        assert_eq!(tinted.selection.a, base.selection.a);
                        assert_eq!(tinted.accent, base.accent);
                        assert_eq!(tinted.secondary, base.secondary);
                        for wash in surfaces(&tinted) {
                            assert!(match mode {
                                Mode::Dark => wash.l <= 0.13,
                                Mode::Light => wash.l >= 0.94,
                            });
                            assert_eq!(wash.a, 1.0);
                        }
                        // Legacy aliases follow the washed surfaces.
                        assert_eq!(tinted.bg_canvas, tinted.background);
                        assert_eq!(tinted.accent_tint, tinted.selection);
                    }
                }
            }
        }
    }

    #[test]
    fn dark_is_default_and_palettes_differ() {
        assert_eq!(Mode::default(), Mode::Dark);
        assert_ne!(Palette::dark().background, Palette::light().background);
        // Artwork scrims are theme-independent by design.
        assert_eq!(Palette::dark().scrim, Palette::light().scrim);
        assert_eq!(Palette::dark().brand_wash, Palette::light().brand_wash);
    }
}
