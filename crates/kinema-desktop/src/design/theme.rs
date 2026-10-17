//! Applies the semantic palette to the pinned Kit theme in one `Theme::update`.
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::{App, Global, SharedString, px};

use super::tokens::{Mode, Tokens, metrics};

struct Active {
    tokens: Tokens,
    family: Option<SharedString>,
    fixture_mono_family: Option<SharedString>,
}

impl Global for Active {}

/// Initialise after `gpui_kit::init`. Resolves one native UI font family
/// (no CSS stacks) and starts in dark mode.
pub fn init(cx: &mut App) {
    init_with_family(cx, None);
}

/// Like [`init`], with an explicit already-registered family (offline fixtures).
pub fn init_with_family(cx: &mut App, family: Option<&str>) {
    // Production retains Kit's monospace default; controlled fixtures explicitly
    // register a single family and may use it for both roles.
    let fixture_mono_family = family.map(|name| SharedString::from(name.to_owned()));
    let family = family
        .map(|name| SharedString::from(name.to_owned()))
        .or_else(|| resolve_family(cx));
    cx.set_global(Active {
        tokens: Tokens::dark(),
        family,
        fixture_mono_family,
    });
    apply(cx);
}

/// Current semantic tokens. Panics only if [`init`] was never called.
pub fn tokens(cx: &App) -> Tokens {
    cx.global::<Active>().tokens
}

pub fn mode(cx: &App) -> Mode {
    tokens(cx).mode
}

/// Switch the complete palette; callers keep route, focus and data untouched.
pub fn set_mode(cx: &mut App, mode: Mode) {
    cx.global_mut::<Active>().tokens = Tokens::for_mode(mode);
    apply(cx);
}

/// First installed family from a per-platform preference list, else `None`
/// (the Kit's own system-font default stays in place).
pub fn resolve_family(cx: &App) -> Option<SharedString> {
    let installed = cx.text_system().all_font_names();
    let preferred: &[&str] = if cfg!(target_os = "windows") {
        &["Segoe UI Variable Text", "Segoe UI"]
    } else if cfg!(target_os = "macos") {
        &[".AppleSystemUIFont", "SF Pro Text", "Helvetica Neue"]
    } else {
        &["Noto Sans", "Cantarell", "DejaVu Sans"]
    };
    pick_family(preferred, &installed).map(SharedString::from)
}

fn pick_family(preferred: &[&str], installed: &[String]) -> Option<String> {
    preferred
        .iter()
        .find(|name| installed.iter().any(|have| have == **name))
        .map(|name| (*name).to_owned())
}

fn apply(cx: &mut App) {
    let t = tokens(cx);
    let family = cx.global::<Active>().family.clone();
    let fixture_mono_family = cx.global::<Active>().fixture_mono_family.clone();
    // Loads the built-in config for the mode first; colours are edited after it.
    Theme::change(
        match t.mode {
            Mode::Dark => ThemeMode::Dark,
            Mode::Light => ThemeMode::Light,
        },
        None,
        cx,
    );
    Theme::update(cx, |theme| {
        if let Some(family) = family {
            theme.font_family = family;
        }
        if let Some(family) = fixture_mono_family {
            theme.mono_font_family = family;
        }
        theme.font_size = px(metrics::FONT_BODY);
        theme.radius = px(metrics::CONTROL_RADIUS);
        theme.radius_lg = px(metrics::PANEL_RADIUS);
        theme.shadow = false;
        let c = &mut theme.colors;
        c.background = t.canvas;
        c.foreground = t.text;
        c.border = t.border;
        // Kit documents `input` as the Input/Select/default-button outline.
        c.input = t.control_border;
        c.ring = t.focus;
        c.caret = t.text;
        c.link = t.focus;
        c.link_hover = t.focus;
        c.link_active = t.focus;
        c.selection = t.selection_subtle;
        c.accent = t.hover;
        c.accent_foreground = t.text;
        c.muted = t.raised;
        c.muted_foreground = t.muted;
        c.popover = t.surface;
        c.popover_foreground = t.text;
        c.overlay = t.overlay;
        c.skeleton = t.raised;
        c.list = t.surface;
        c.list_hover = t.hover;
        c.list_active = t.selection_subtle;
        c.list_active_border = t.focus;
        c.list_even = t.surface;
        c.list_head = t.surface;
        c.secondary = t.raised;
        c.secondary_hover = t.hover;
        c.secondary_active = t.control_pressed;
        c.secondary_foreground = t.text;
        c.button = t.control;
        c.button_hover = t.control_hover;
        c.button_active = t.control_pressed;
        c.button_foreground = t.text;
        c.button_secondary = t.raised;
        c.button_secondary_hover = t.hover;
        c.button_secondary_active = t.control_pressed;
        c.button_secondary_foreground = t.text;
        c.primary = t.primary;
        c.primary_hover = t.primary_hover;
        c.primary_active = t.primary_pressed;
        c.primary_foreground = t.on_primary;
        c.button_primary = t.primary;
        c.button_primary_hover = t.primary_hover;
        c.button_primary_active = t.primary_pressed;
        c.button_primary_foreground = t.on_primary;
        c.danger = t.error;
        c.danger_hover = t.error;
        c.danger_active = t.error;
        c.danger_foreground = t.text;
        c.button_danger = t.danger_fill;
        c.button_danger_hover = t.danger_fill;
        c.button_danger_active = t.danger_fill;
        c.button_danger_foreground = t.on_primary;
        c.success = t.success;
        c.warning = t.warning;
        c.info = t.focus;
        c.tab = t.canvas;
        c.tab_active = t.surface;
        c.tab_bar = t.canvas;
        c.tab_foreground = t.secondary;
        c.tab_active_foreground = t.text;
        c.title_bar = t.canvas;
        c.title_bar_border = t.border;
        c.sidebar = t.surface;
        c.sidebar_border = t.border;
        c.sidebar_foreground = t.text;
    });
}

#[cfg(test)]
mod tests {
    use super::pick_family;

    #[test]
    fn first_installed_preference_wins_and_absent_is_none() {
        let installed = vec!["Cantarell".to_owned(), "Noto Sans".to_owned()];
        assert_eq!(
            pick_family(&["Missing", "Noto Sans", "Cantarell"], &installed).as_deref(),
            Some("Noto Sans")
        );
        assert_eq!(pick_family(&["Missing"], &installed), None);
    }
}
