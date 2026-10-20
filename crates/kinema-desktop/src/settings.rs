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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PosterSize {
    Small,
    #[default]
    Medium,
    Large,
}
impl PosterSize {
    /// Preferred poster width in logical pixels; the grid may stretch cards to fit.
    #[allow(
        dead_code,
        reason = "appearance model groundwork; consumed by the v3 grid"
    )]
    pub fn preferred_width(self) -> f32 {
        match self {
            Self::Small => 140.0,
            Self::Medium => 172.0,
            Self::Large => 212.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MotionChoice {
    #[default]
    System,
    Always,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub metadata: MetadataProvider,
    pub indexer: IndexerKind,
    pub debrid: DebridKind,
    pub media_limit_bytes: u64,
    pub player: PlayerChoice,
    pub theme: ThemeChoice,
    pub poster_size: PosterSize,
    pub reduce_motion: MotionChoice,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            metadata: MetadataProvider::Cinemeta,
            indexer: IndexerKind::Torrentio,
            debrid: DebridKind::RealDebrid,
            media_limit_bytes: 20 * 1024 * 1024 * 1024,
            player: PlayerChoice::Mpv,
            theme: ThemeChoice::default(),
            poster_size: PosterSize::default(),
            reduce_motion: MotionChoice::default(),
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
        let _span = tracing::info_span!("settings", operation = "open").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            std::fs::create_dir_all(&paths.config).map_err(|error| {
                tracing::warn!(io_kind = ?error.kind(), "Settings I/O failed");
                SettingsError::Io
            })?;
            Ok(Self {
                path: paths.config.join("settings.toml"),
            })
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "settings operation completed"
            ),
            Err(category) => {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "settings operation failed"
                );
            }
        }
        result
    }
    pub fn load(&self) -> Result<Settings, SettingsError> {
        let _span = tracing::info_span!("settings", operation = "load").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            let text = match std::fs::read_to_string(&self.path) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tracing::info!(source = "defaults", "Settings loaded");
                    return Ok(Settings::default());
                }
                Err(error) => {
                    tracing::warn!(io_kind = ?error.kind(), "Settings read failed");
                    return Err(SettingsError::Io);
                }
            };
            let document: Document = toml::from_str(&text).map_err(|_| SettingsError::Invalid)?;
            let settings = document.into_settings()?;
            tracing::info!(source = "disk", "Settings loaded");
            Ok(settings)
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "settings operation completed"
            ),
            Err(category) => {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "settings operation failed"
                );
            }
        }
        result
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
        let _span = tracing::info_span!("settings", operation = "save_with_parent_sync").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            validate(settings)?;
            // Refuse to overwrite unknown/corrupt formats, even on an explicit save.
            self.load()?;
            let text = toml::to_string(&Document::from_settings(settings)?)
                .map_err(|_| SettingsError::Invalid)?;
            let parent = self.path.parent().ok_or(SettingsError::Io)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
                tracing::warn!(io_kind = ?error.kind(), "Settings I/O failed");
                SettingsError::Io
            })?;
            temporary.write_all(text.as_bytes()).map_err(|error| {
                tracing::warn!(io_kind = ?error.kind(), "Settings I/O failed");
                SettingsError::Io
            })?;
            temporary.as_file().sync_all().map_err(|error| {
                tracing::warn!(io_kind = ?error.kind(), "Settings I/O failed");
                SettingsError::Io
            })?;
            temporary.persist(&self.path).map_err(|error| {
                tracing::warn!(io_kind = ?error.error.kind(), "Settings replacement failed");
                SettingsError::Io
            })?;
            sync(parent).map_err(|error| {
                tracing::warn!(io_kind = ?error.kind(), "Settings durability uncertain");
                SettingsError::DurabilityUncertain
            })?;
            tracing::info!("Settings saved");
            Ok(())
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "settings operation completed"
            ),
            Err(category) => {
                tracing::warn!(
                    ?category,
                    duration_ms = started.elapsed().as_millis() as u64,
                    "settings operation failed"
                );
            }
        }
        result
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
    #[serde(default)]
    theme: ThemeChoice,
    #[serde(default)]
    poster_size: PosterSize,
    #[serde(default)]
    reduce_motion: MotionChoice,
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
            theme: settings.theme,
            poster_size: settings.poster_size,
            reduce_motion: settings.reduce_motion,
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
            theme: self.theme,
            poster_size: self.poster_size,
            reduce_motion: self.reduce_motion,
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
    fn appearance_defaults_and_widths() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        for settings in [Settings::default(), store.load()?] {
            assert_eq!(settings.theme, ThemeChoice::Dark);
            assert_eq!(settings.poster_size, PosterSize::Medium);
            assert_eq!(settings.reduce_motion, MotionChoice::System);
        }
        assert_eq!(PosterSize::Small.preferred_width(), 140.0);
        assert_eq!(PosterSize::Medium.preferred_width(), 172.0);
        assert_eq!(PosterSize::Large.preferred_width(), 212.0);
        Ok(())
    }

    #[test]
    fn appearance_choices_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        for theme in [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System] {
            for poster_size in [PosterSize::Small, PosterSize::Medium, PosterSize::Large] {
                for reduce_motion in [MotionChoice::System, MotionChoice::Always] {
                    let settings = Settings {
                        theme,
                        poster_size,
                        reduce_motion,
                        ..Settings::default()
                    };
                    store.save(&settings)?;
                    let loaded = store.load()?;
                    assert_eq!(loaded.theme, theme);
                    assert_eq!(loaded.poster_size, poster_size);
                    assert_eq!(loaded.reduce_motion, reduce_motion);
                    let document: toml::Value =
                        toml::from_str(&std::fs::read_to_string(&store.path)?)?;
                    assert_eq!(document["version"].as_integer(), Some(1));
                    assert_eq!(
                        document["theme"].as_str(),
                        Some(match theme {
                            ThemeChoice::Dark => "dark",
                            ThemeChoice::Light => "light",
                            ThemeChoice::System => "system",
                        })
                    );
                    assert_eq!(
                        document["poster_size"].as_str(),
                        Some(match poster_size {
                            PosterSize::Small => "small",
                            PosterSize::Medium => "medium",
                            PosterSize::Large => "large",
                        })
                    );
                    assert_eq!(
                        document["reduce_motion"].as_str(),
                        Some(match reduce_motion {
                            MotionChoice::System => "system",
                            MotionChoice::Always => "always",
                        })
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn missing_appearance_fields_default_independently() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        let old = "version = 1\nindexer = \"torrentio\"\ndebrid = \"real-debrid\"\nmedia_limit_bytes = 1234\nplayer = \"mpv\"\n";
        for (extra, theme, poster_size, reduce_motion) in [
            (
                "",
                ThemeChoice::Dark,
                PosterSize::Medium,
                MotionChoice::System,
            ),
            (
                "theme = \"light\"\n",
                ThemeChoice::Light,
                PosterSize::Medium,
                MotionChoice::System,
            ),
            (
                "poster_size = \"large\"\n",
                ThemeChoice::Dark,
                PosterSize::Large,
                MotionChoice::System,
            ),
            (
                "reduce_motion = \"always\"\n",
                ThemeChoice::Dark,
                PosterSize::Medium,
                MotionChoice::Always,
            ),
        ] {
            std::fs::write(&store.path, format!("{old}{extra}"))?;
            let settings = store.load()?;
            assert_eq!(settings.theme, theme);
            assert_eq!(settings.poster_size, poster_size);
            assert_eq!(settings.reduce_motion, reduce_motion);
        }
        Ok(())
    }

    #[test]
    fn invalid_appearance_values_preserve_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = SettingsStore::open(&paths(dir.path()))?;
        let old = "version = 1\nindexer = \"torrentio\"\ndebrid = \"real-debrid\"\nmedia_limit_bytes = 1234\nplayer = \"mpv\"\n";
        for field in ["theme", "poster_size", "reduce_motion"] {
            for value in ["\"unknown\"", "\"Dark\"", "42", "true"] {
                let invalid = format!("{old}{field} = {value}\n");
                std::fs::write(&store.path, &invalid)?;
                assert!(matches!(store.load(), Err(SettingsError::Invalid)));
                assert_eq!(
                    store.save(&Settings::default()),
                    Err(SettingsError::Invalid)
                );
                assert_eq!(std::fs::read_to_string(&store.path)?, invalid);
            }
        }
        Ok(())
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

#[cfg(test)]
mod logging_tests {
    use super::*;
    use crate::providers::test_logging::{capture, excludes};

    #[test]
    fn settings_diagnostics_exclude_values_and_malformed_input()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let paths = AppPaths {
            data: root.path().into(),
            config: root.path().into(),
            cache: root.path().into(),
        };
        let store = SettingsStore::open(&paths)?;
        let settings = Settings {
            player: PlayerChoice::Custom(root.path().join("player-path-secret.exe")),
            ..Settings::default()
        };
        let (result, output) = capture(|| {
            store.save(&settings)?;
            std::fs::write(&store.path, "token = 'bearer-secret'\nurl = 'https://cdn.invalid/path?signature=query-secret'\ninvalid toml")
                .map_err(|_| SettingsError::Io)?;
            assert!(matches!(store.load(), Err(SettingsError::Invalid)));
            Ok::<_, SettingsError>(())
        });
        result?;
        assert!(output.contains("Settings saved"));
        assert!(output.contains("settings operation failed"));
        excludes(
            &output,
            &[
                "player-path-secret",
                "bearer-secret",
                "query-secret",
                "signature=",
                "invalid toml",
            ],
        );
        Ok(())
    }
}
