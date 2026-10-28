//! Embedded monochrome brand assets. Loading/decoding belongs on a worker.

#[derive(Clone)]
pub struct IconData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy)]
enum Platform {
    #[cfg(any(test, target_os = "linux"))]
    Linux,
    #[cfg(any(test, target_os = "windows"))]
    Windows,
    #[cfg(any(test, target_os = "macos"))]
    Macos,
}

fn asset(platform: Platform, _system_uses_light_theme: Option<bool>) -> &'static [u8] {
    match platform {
        #[cfg(any(test, target_os = "macos"))]
        Platform::Macos => {
            include_bytes!("../../assets/brand/tray/dev.tlmtech.kinema-tray-dark-36.png")
        }
        #[cfg(any(test, target_os = "windows"))]
        Platform::Windows if _system_uses_light_theme == Some(true) => {
            include_bytes!("../../assets/brand/tray/dev.tlmtech.kinema-tray-dark-32.png")
        }
        #[cfg(any(test, target_os = "windows"))]
        Platform::Windows => {
            include_bytes!("../../assets/brand/tray/dev.tlmtech.kinema-tray-light-32.png")
        }
        #[cfg(any(test, target_os = "linux"))]
        Platform::Linux => {
            include_bytes!("../../assets/brand/tray/dev.tlmtech.kinema-tray-light-48.png")
        }
    }
}

impl IconData {
    /// Decode the embedded logo and (on Windows) query taskbar theme once.
    /// Run outside the UI thread. Unknown theme defaults to white on dark.
    pub fn load_for_current_platform() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        let platform = Platform::Linux;
        #[cfg(target_os = "windows")]
        let platform = Platform::Windows;
        #[cfg(target_os = "macos")]
        let platform = Platform::Macos;
        #[cfg(target_os = "windows")]
        let light_theme = windows_system_uses_light_theme();
        #[cfg(not(target_os = "windows"))]
        let light_theme = None;
        decode(asset(platform, light_theme))
    }
}

fn decode(png: &[u8]) -> Result<IconData, String> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| format!("Tray logo decoding: {error}"))?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Ok(IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

#[cfg(target_os = "windows")]
fn windows_system_uses_light_theme() -> Option<bool> {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, KEY_READ},
    };
    // winreg is already locked through embed-resource. Its safe wrapper keeps this
    // crate's forbid(unsafe_code) policy intact; no shell or registry polling.
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            KEY_READ,
        )
        .ok()?;
    key.get_value::<u32, _>("SystemUsesLightTheme")
        .ok()
        .map(|value| value != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_and_taskbar_theme_select_correct_monochrome_logo() -> Result<(), String> {
        for (platform, light_theme, size, color) in [
            (Platform::Macos, None, 36, [35, 38, 41]),
            (Platform::Windows, Some(true), 32, [35, 38, 41]),
            (Platform::Windows, Some(false), 32, [255, 255, 255]),
            (Platform::Windows, None, 32, [255, 255, 255]),
            (Platform::Linux, None, 48, [255, 255, 255]),
            (Platform::Linux, Some(true), 48, [255, 255, 255]),
        ] {
            let icon = decode(asset(platform, light_theme))?;
            assert_eq!((icon.width, icon.height), (size, size));
            assert_eq!(icon.rgba.len(), (size * size * 4) as usize);
            assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 0));
            assert!(icon.rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
            // Raster antialiasing can round unpremultiplied edge RGB; opaque
            // pixels preserve the exact brand tone and alpha preserves shape.
            for pixel in icon.rgba.chunks_exact(4).filter(|pixel| pixel[3] == 255) {
                assert_eq!(&pixel[..3], &color);
            }
        }
        Ok(())
    }

    #[test]
    fn invalid_png_is_a_failure_not_an_empty_icon() {
        assert!(decode(b"not a PNG").is_err());
    }

    #[test]
    fn current_platform_asset_decodes() {
        assert!(IconData::load_for_current_platform().is_ok());
    }
}
