//! Application assets layered over the official gpui-kit assets: the approved
//! TMDB logo is embedded; every other path is delegated unchanged.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

/// Asset path used with `img(...)`; rendered with the logo's original colors.
pub(crate) const TMDB_LOGO: &str = "kinema/tmdb-logo.svg";
const TMDB_LOGO_SVG: &[u8] = include_bytes!("../assets/tmdb-logo.svg");

pub(crate) struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == TMDB_LOGO {
            return Ok(Some(Cow::Borrowed(TMDB_LOGO_SVG)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_logo_and_delegates_the_rest() {
        let logo = AppAssets.load(TMDB_LOGO).ok().flatten();
        assert!(logo.is_some_and(|bytes| bytes.starts_with(b"<svg")));
        assert!(AppAssets.load("kinema/missing").ok().flatten().is_none());
        assert!(AppAssets.load("icons/search.svg").ok().flatten().is_some());
    }
}
