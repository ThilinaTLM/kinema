//! OS secure-storage access; never call from rendering or log backend errors.
//!
//! Keyring 4.2 caches its native store initialization result for the process.
//! If initialization failed, make secure storage available and restart Kinema
//! before retrying. Surface Unavailable; never fall back to ordinary files.
use kinema_core::sources::DebridKind;
use std::fmt;
use zeroize::Zeroizing;

pub struct SecretString(Zeroizing<String>);
impl SecretString {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}
impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialError {
    Missing,
    Unavailable,
    AccessDenied,
}
impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "credential is not configured",
            Self::Unavailable => "secure credential storage is unavailable; restore access and restart Kinema if it persists",
            Self::AccessDenied => "secure credential storage access was denied",
        })
    }
}
impl std::error::Error for CredentialError {}

trait Vault: Send + Sync {
    fn get(&self, account: &'static str) -> Result<SecretString, CredentialError>;
    fn set(&self, account: &'static str, secret: SecretString) -> Result<(), CredentialError>;
    fn delete(&self, account: &'static str) -> Result<(), CredentialError>;
}

pub struct CredentialStore {
    vault: Box<dyn Vault>,
}
impl Default for CredentialStore {
    fn default() -> Self {
        Self::new()
    }
}
impl CredentialStore {
    /// Does not contact the OS until get/set/delete/status is called.
    pub fn new() -> Self {
        Self {
            vault: Box::new(OsVault),
        }
    }
    pub fn get(&self, kind: DebridKind) -> Result<SecretString, CredentialError> {
        self.vault.get(debrid_account(kind))
    }
    pub fn set(&self, kind: DebridKind, secret: SecretString) -> Result<(), CredentialError> {
        self.vault.set(debrid_account(kind), secret)
    }
    pub fn delete(&self, kind: DebridKind) -> Result<(), CredentialError> {
        self.vault.delete(debrid_account(kind))
    }
    /// Checks presence without exposing a token to the UI; any temporary is zeroized.
    pub fn status(&self, kind: DebridKind) -> Result<bool, CredentialError> {
        match self.get(kind) {
            Ok(_) => Ok(true),
            Err(CredentialError::Missing) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
/// Separate TMDB credential namespace; never stores tokens in ordinary settings.
pub struct MetadataCredentialStore {
    vault: Box<dyn Vault>,
}
impl Default for MetadataCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}
impl MetadataCredentialStore {
    pub fn new() -> Self {
        Self {
            vault: Box::new(OsVault),
        }
    }
    pub fn get(&self) -> Result<SecretString, CredentialError> {
        self.vault.get("tmdb")
    }
    pub fn set(&self, secret: SecretString) -> Result<(), CredentialError> {
        self.vault.set("tmdb", secret)
    }
    pub fn delete(&self) -> Result<(), CredentialError> {
        self.vault.delete("tmdb")
    }
    pub fn status(&self) -> Result<bool, CredentialError> {
        match self.get() {
            Ok(_) => Ok(true),
            Err(CredentialError::Missing) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
fn debrid_account(kind: DebridKind) -> &'static str {
    match kind {
        DebridKind::RealDebrid => "real-debrid",
        DebridKind::AllDebrid => "all-debrid",
    }
}
struct OsVault;
fn entry(account: &str) -> Result<keyring::Entry, CredentialError> {
    keyring::Entry::new("kinema-next", account).map_err(safe_error)
}
fn safe_error(error: keyring::Error) -> CredentialError {
    match error {
        keyring::Error::NoEntry => CredentialError::Missing,
        keyring::Error::NoStorageAccess(_) => CredentialError::AccessDenied,
        // Some backends attach raw credential bytes to errors. Zeroize before dropping.
        keyring::Error::BadEncoding(mut bytes) | keyring::Error::BadDataFormat(mut bytes, _) => {
            zeroize::Zeroize::zeroize(&mut bytes);
            CredentialError::Unavailable
        }
        _ => CredentialError::Unavailable,
    }
}
impl Vault for OsVault {
    fn get(&self, account: &'static str) -> Result<SecretString, CredentialError> {
        entry(account)?
            .get_password()
            .map(SecretString::new)
            .map_err(safe_error)
    }
    fn set(&self, account: &'static str, secret: SecretString) -> Result<(), CredentialError> {
        entry(account)?
            .set_password(secret.expose())
            .map_err(safe_error)
    }
    fn delete(&self, account: &'static str) -> Result<(), CredentialError> {
        entry(account)?.delete_credential().map_err(safe_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };
    #[derive(Clone, Default)]
    struct FakeVault(Arc<Mutex<HashMap<&'static str, SecretString>>>);
    impl Vault for FakeVault {
        fn get(&self, account: &'static str) -> Result<SecretString, CredentialError> {
            self.0
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .get(account)
                .map(|secret| SecretString::new(secret.expose().into()))
                .ok_or(CredentialError::Missing)
        }
        fn set(&self, account: &'static str, secret: SecretString) -> Result<(), CredentialError> {
            self.0
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .insert(account, secret);
            Ok(())
        }
        fn delete(&self, account: &'static str) -> Result<(), CredentialError> {
            self.0
                .lock()
                .map_err(|_| CredentialError::Unavailable)?
                .remove(account)
                .map(|_| ())
                .ok_or(CredentialError::Missing)
        }
    }
    #[test]
    fn fake_roundtrip_and_status() -> Result<(), CredentialError> {
        let store = CredentialStore {
            vault: Box::<FakeVault>::default(),
        };
        assert!(!store.status(DebridKind::RealDebrid)?);
        store.set(
            DebridKind::RealDebrid,
            SecretString::new("private token".into()),
        )?;
        assert!(store.status(DebridKind::RealDebrid)?);
        assert!(!store.status(DebridKind::AllDebrid)?);
        assert_eq!(store.get(DebridKind::RealDebrid)?.expose(), "private token");
        store.delete(DebridKind::RealDebrid)?;
        assert!(!store.status(DebridKind::RealDebrid)?);
        Ok(())
    }
    #[test]
    fn metadata_bearer_stays_in_vault_not_settings() -> Result<(), Box<dyn std::error::Error>> {
        use crate::{
            paths::AppPaths,
            settings::{MetadataProvider, Settings, SettingsStore},
        };
        let vault = FakeVault::default();
        let metadata = MetadataCredentialStore {
            vault: Box::new(vault.clone()),
        };
        let debrid = CredentialStore {
            vault: Box::new(vault),
        };
        assert!(!metadata.status()?);
        let token = "tmdb-private-bearer-fixture";
        metadata.set(SecretString::new(token.into()))?;
        assert_eq!(metadata.get()?.expose(), token);
        assert!(metadata.status()?);
        assert!(!debrid.status(DebridKind::RealDebrid)?);
        debrid.set(
            DebridKind::RealDebrid,
            SecretString::new("separate debrid fixture".into()),
        )?;
        let root = tempfile::tempdir()?;
        let paths = AppPaths {
            data: root.path().into(),
            config: root.path().into(),
            cache: root.path().into(),
        };
        let store = SettingsStore::open(&paths)?;
        store.save(&Settings {
            metadata: MetadataProvider::Tmdb,
            ..Settings::default()
        })?;
        let text = std::fs::read_to_string(root.path().join("settings.toml"))?;
        assert!(text.contains("metadata = \"tmdb\""));
        assert!(!text.contains(token));
        assert!(!text.contains("bearer"));
        assert!(!format!("{:?}", metadata.get()?).contains(token));
        metadata.delete()?;
        assert!(!metadata.status()?);
        assert_eq!(metadata.delete(), Err(CredentialError::Missing));
        assert!(debrid.status(DebridKind::RealDebrid)?);
        Ok(())
    }

    #[test]
    fn debug_and_errors_never_include_backend_payload() {
        let secret = SecretString::new("private token".into());
        assert_eq!(format!("{secret:?}"), "SecretString([REDACTED])");
        let error = safe_error(keyring::Error::Invalid(
            "private token".into(),
            "private token".into(),
        ));
        assert!(!format!("{error:?}: {error}").contains("private token"));
        assert_eq!(
            safe_error(keyring::Error::NoEntry),
            CredentialError::Missing
        );
        assert_eq!(
            safe_error(keyring::Error::NoStorageAccess(Box::new(
                std::io::Error::other("private token")
            ))),
            CredentialError::AccessDenied
        );
    }
}
