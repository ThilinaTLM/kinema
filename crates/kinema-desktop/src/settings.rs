//! Versioned nonsecret settings. All methods doing I/O belong on background workers.
use crate::{
    paths::AppPaths,
    player::{PlayerChoice, validate_choice},
};
use kinema_core::sources::{DebridKind, IndexerKind};
use serde::{Deserialize, Serialize};
use std::{fmt, io::Write, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetadataProvider {
    #[default]
    Cinemeta,
    Tmdb,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub metadata: MetadataProvider,
    pub indexer: IndexerKind,
    pub debrid: DebridKind,
    pub media_limit_bytes: u64,
    pub player: PlayerChoice,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            metadata: MetadataProvider::Cinemeta,
            indexer: IndexerKind::Torrentio,
            debrid: DebridKind::RealDebrid,
            media_limit_bytes: 20 * 1024 * 1024 * 1024,
            player: PlayerChoice::Mpv,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsError {
    Io,
    DurabilityUncertain,
    Invalid,
    UnsupportedVersion,
    InvalidLimit,
    InvalidPlayer,
}
impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io => "settings could not be read or saved",
            Self::DurabilityUncertain => {
                "settings were replaced, but crash durability could not be confirmed"
            }
            Self::Invalid => "settings file is invalid; existing file was preserved",
            Self::UnsupportedVersion => "settings version is not supported",
            Self::InvalidLimit => "media limit must be greater than zero",
            Self::InvalidPlayer => "custom player must be an absolute executable path",
        })
    }
}
impl std::error::Error for SettingsError {}

pub struct SettingsStore {
    path: PathBuf,
}
impl SettingsStore {
    /// Creates the config directory, but never replaces an existing settings file.
    pub fn open(paths: &AppPaths) -> Result<Self, SettingsError> {
        std::fs::create_dir_all(&paths.config).map_err(|_| SettingsError::Io)?;
        Ok(Self {
            path: paths.config.join("settings.toml"),
        })
    }
    pub fn load(&self) -> Result<Settings, SettingsError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settings::default());
            }
            Err(_) => return Err(SettingsError::Io),
        };
        let document: Document = toml::from_str(&text).map_err(|_| SettingsError::Invalid)?;
        document.into_settings()
    }
    /// A durability error occurs after replacement: the new settings remain visible.
    /// Never reset or roll back the file in response to that error.
    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        self.save_with_parent_sync(settings, sync_parent)
    }

    fn save_with_parent_sync(
        &self,
        settings: &Settings,
        sync: impl FnOnce(&std::path::Path) -> std::io::Result<()>,
    ) -> Result<(), SettingsError> {
        validate(settings)?;
        // Refuse to overwrite unknown/corrupt formats, even on an explicit save.
        self.load()?;
        let text = toml::to_string(&Document::from_settings(settings)?)
            .map_err(|_| SettingsError::Invalid)?;
        let parent = self.path.parent().ok_or(SettingsError::Io)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| SettingsError::Io)?;
        temporary
            .write_all(text.as_bytes())
            .map_err(|_| SettingsError::Io)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| SettingsError::Io)?;
        temporary
            .persist(&self.path)
            .map_err(|_| SettingsError::Io)?;
        sync(parent).map_err(|_| SettingsError::DurabilityUncertain)?;
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent(parent: &std::path::Path) -> std::io::Result<()> {
    std::fs::File::open(parent)?.sync_all()
}

// Opening directories as files is not portable to Windows.
#[cfg(not(unix))]
fn sync_parent(_parent: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

fn validate(settings: &Settings) -> Result<(), SettingsError> {
    if settings.media_limit_bytes == 0 {
        return Err(SettingsError::InvalidLimit);
    }
    validate_choice(&settings.player).map_err(|_| SettingsError::InvalidPlayer)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    #[serde(default)]
    metadata: Option<MetadataProvider>,
    indexer: String,
    debrid: String,
    media_limit_bytes: u64,
    player: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    player_path: Option<String>,
}
impl Document {
    fn from_settings(settings: &Settings) -> Result<Self, SettingsError> {
        let (player, player_path) = match &settings.player {
            PlayerChoice::Mpv => ("mpv", None),
            PlayerChoice::Vlc => ("vlc", None),
            PlayerChoice::Custom(path) => (
                "custom",
                Some(
                    path.to_str()
                        .ok_or(SettingsError::InvalidPlayer)?
                        .to_owned(),
                ),
            ),
        };
        Ok(Self {
            version: 1,
            metadata: Some(settings.metadata),
            indexer: match settings.indexer {
                IndexerKind::Torrentio => "torrentio",
                IndexerKind::Peerflix => "peerflix",
            }
            .into(),
            debrid: match settings.debrid {
                DebridKind::RealDebrid => "real-debrid",
                DebridKind::AllDebrid => "all-debrid",
            }
            .into(),
            media_limit_bytes: settings.media_limit_bytes,
            player: player.into(),
            player_path,
        })
    }
    fn into_settings(self) -> Result<Settings, SettingsError> {
        if self.version != 1 {
            return Err(SettingsError::UnsupportedVersion);
        }
        let settings = Settings {
            metadata: self.metadata.unwrap_or_default(),
            indexer: match self.indexer.as_str() {
                "torrentio" => IndexerKind::Torrentio,
                "peerflix" => IndexerKind::Peerflix,
                _ => return Err(SettingsError::Invalid),
            },
            debrid: match self.debrid.as_str() {
                "real-debrid" => DebridKind::RealDebrid,
                "all-debrid" => DebridKind::AllDebrid,
                _ => return Err(SettingsError::Invalid),
            },
            media_limit_bytes: self.media_limit_bytes,
            player: match (self.player.as_str(), self.player_path) {
                ("mpv", None) => PlayerChoice::Mpv,
                ("vlc", None) => PlayerChoice::Vlc,
                ("custom", Some(path)) => PlayerChoice::Custom(path.into()),
                _ => return Err(SettingsError::Invalid),
            },
        };
        validate(&settings)?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn paths(root: &std::path::Path) -> AppPaths {
        AppPaths {
            data: root.into(),
            config: root.into(),
            cache: root.into(),
        }
    }
    #[test]
    fn old_v1_defaults_metadata_and_explicit_selection_roundtrips()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        let old = "version = 1\nindexer = \"peerflix\"\ndebrid = \"all-debrid\"\nmedia_limit_bytes = 1234\nplayer = \"vlc\"\n";
        std::fs::write(&store.path, old)?;
        let mut settings = store.load()?;
        assert_eq!(settings.metadata, MetadataProvider::Cinemeta);
        assert_eq!(settings.indexer, IndexerKind::Peerflix);
        assert_eq!(settings.player, PlayerChoice::Vlc);
        settings.metadata = MetadataProvider::Tmdb;
        store.save(&settings)?;
        assert_eq!(store.load()?.metadata, MetadataProvider::Tmdb);
        assert_eq!(store.load()?.media_limit_bytes, 1234);
        settings.metadata = MetadataProvider::Cinemeta;
        store.save(&settings)?;
        assert!(std::fs::read_to_string(&store.path)?.contains("metadata = \"cinemeta\""));
        for extra in [
            "metadata = \"unknown\"\n",
            "tmdb_bearer = \"not-allowed\"\n",
        ] {
            let invalid = format!("{old}{extra}");
            std::fs::write(&store.path, &invalid)?;
            assert!(store.load().is_err());
            assert!(store.save(&Settings::default()).is_err());
            assert_eq!(std::fs::read_to_string(&store.path)?, invalid);
        }
        Ok(())
    }

    #[test]
    fn defaults_and_unicode_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        assert_eq!(store.load()?.media_limit_bytes, 20 * 1024 * 1024 * 1024);
        let settings = Settings {
            player: PlayerChoice::Custom(dir.path().join("播放器 spaces.exe")),
            ..Settings::default()
        };
        store.save(&settings)?;
        assert_eq!(store.load()?.player, settings.player);
        let text = std::fs::read_to_string(&store.path)?;
        assert!(!text.contains("token"));
        Ok(())
    }
    #[test]
    fn invalid_settings_never_reset_and_failed_save_preserves_file()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        store.save(&Settings::default())?;
        let original = std::fs::read(&store.path)?;
        let invalid = Settings {
            media_limit_bytes: 0,
            ..Settings::default()
        };
        assert_eq!(store.save(&invalid), Err(SettingsError::InvalidLimit));
        assert_eq!(std::fs::read(&store.path)?, original);
        std::fs::write(&store.path, "broken = [")?;
        assert!(store.load().is_err());
        assert!(store.save(&Settings::default()).is_err());
        assert_eq!(std::fs::read_to_string(&store.path)?, "broken = [");
        Ok(())
    }
    #[test]
    fn parent_sync_failure_reports_replacement_without_reset()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        store.save(&Settings::default())?;
        let changed = Settings {
            media_limit_bytes: 1234,
            ..Settings::default()
        };
        let result = store.save_with_parent_sync(&changed, |parent| {
            assert_eq!(parent, dir.path());
            // Replacement must have completed before synchronizing its directory.
            assert_eq!(
                store.load().map(|settings| settings.media_limit_bytes),
                Ok(1234)
            );
            Err(std::io::Error::other("injected directory sync failure"))
        });
        assert_eq!(result, Err(SettingsError::DurabilityUncertain));
        assert_eq!(store.load()?.media_limit_bytes, 1234);
        // Retrying a save is explicit; the failure never creates a default config.
        store.save(&changed)?;
        assert_eq!(store.load()?.media_limit_bytes, 1234);
        Ok(())
    }

    #[test]
    fn failed_validation_does_not_sync_or_replace() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        std::fs::write(&store.path, "broken = [")?;
        let synchronized = std::cell::Cell::new(false);
        let result = store.save_with_parent_sync(&Settings::default(), |_| {
            synchronized.set(true);
            Ok(())
        });
        assert_eq!(result, Err(SettingsError::Invalid));
        assert!(!synchronized.get());
        assert_eq!(std::fs::read_to_string(&store.path)?, "broken = [");
        Ok(())
    }

    #[test]
    fn missing_parent_fails_without_partial_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore {
            path: dir.path().join("missing/settings.toml"),
        };
        assert_eq!(store.save(&Settings::default()), Err(SettingsError::Io));
        assert!(!store.path.exists());
        Ok(())
    }
}
