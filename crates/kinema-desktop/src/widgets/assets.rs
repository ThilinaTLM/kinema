//! Embedded v3 assets, independent of the legacy Kit asset source.
//!
//! Install with `application().with_assets(Assets)` and call [`init`] at startup.
//! Sources, licenses and SHA-256 checksums are recorded in `assets/README.md`.

use std::{borrow::Cow, fmt, sync::Once};

use gpui::{App, AssetSource, SharedString};

macro_rules! icons {
    ($($name:literal),+ $(,)?) => {
        &[$((concat!("icons/", $name, ".svg"),
            include_bytes!(concat!("../../assets/icons/", $name, ".svg")).as_slice())),+]
    };
}

const ICONS: &[(&str, &[u8])] = icons![
    "arrow-left",
    "check",
    "circle",
    "circle-dot",
    "chevron-down",
    "chevron-left",
    "chevron-right",
    "x",
    "copy",
    "download",
    "external-link",
    "film",
    "folder-open",
    "info",
    "key-round",
    "menu",
    "ellipsis",
    "play",
    "refresh-cw",
    "search",
    "settings",
    "star",
    "star-filled",
    "square",
    "arrow-down-up",
    "tv",
    "triangle-alert",
    "circle-alert",
];

const LOGO: &[u8] = include_bytes!("../../assets/tmdb-logo.svg");
const FONTS: [&[u8]; 5] = [
    include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
];

/// The complete asset source for v3 windows; unknown paths are not delegated.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let bytes = if path == "kinema/tmdb-logo.svg" {
            Some(LOGO)
        } else {
            ICONS
                .iter()
                .find(|(name, _)| *name == path)
                .map(|(_, bytes)| *bytes)
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(match path {
            "icons" | "icons/" => ICONS.iter().map(|(name, _)| (*name).into()).collect(),
            _ => Vec::new(),
        })
    }
}

/// A bundled-font registration failure. UI startup may continue with platform fallback.
#[derive(Debug)]
pub struct FontRegistrationError(String);

impl fmt::Display for FontRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FontRegistrationError {}

/// Register only the five bundled faces. Failures are logged once per process.
///
/// Registration is intentionally not process-cached: each App owns its text system.
pub fn register_fonts(cx: &mut App) -> Result<(), FontRegistrationError> {
    static LOG_FAILURE: Once = Once::new();
    cx.text_system()
        .add_fonts(FONTS.into_iter().map(Cow::Borrowed).collect())
        .map_err(|error| {
            LOG_FAILURE.call_once(|| {
                tracing::warn!(%error, "Bundled font registration failed; using platform fallback");
            });
            FontRegistrationError(error.to_string())
        })
}

/// Initialize v3 fonts and theme tokens; font failures do not prevent startup.
pub fn init(cx: &mut App) {
    let _ = register_fonts(cx);
    super::tokens::init(cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::icon::IconName;

    #[test]
    fn every_icon_loads_and_is_listed() -> gpui::Result<()> {
        let listing = Assets.list("icons/")?;
        assert_eq!(listing.len(), IconName::ALL.len());
        for icon in IconName::ALL {
            let path = icon.path();
            assert!(
                matches!(Assets.load(path)?, Some(bytes) if bytes.starts_with(b"<svg")),
                "{path}"
            );
            assert!(listing.iter().any(|name| name.as_ref() == path), "{path}");
        }
        assert_eq!(Assets.list("icons")?, listing);
        Ok(())
    }

    #[test]
    fn logo_loads_and_unknown_paths_are_empty() -> gpui::Result<()> {
        assert!(matches!(
            Assets.load("kinema/tmdb-logo.svg")?,
            Some(bytes) if bytes.starts_with(b"<svg")
        ));
        for path in [
            "kinema/missing.svg",
            "icons/missing.svg",
            "fonts/Inter-Regular.ttf",
            "",
        ] {
            assert!(Assets.load(path)?.is_none(), "{path}");
            assert!(Assets.list(path)?.is_empty(), "{path}");
        }
        Ok(())
    }

    #[test]
    fn fonts_are_nonempty_truetype_faces() {
        for bytes in FONTS {
            assert!(bytes.len() > 12);
            assert_eq!(&bytes[..4], b"\x00\x01\x00\x00");
        }
    }
}
