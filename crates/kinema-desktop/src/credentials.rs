//! OS secure-storage access; never call from rendering or log backend errors.
//!
//! Keyring 4.2 caches its native store initialization result for the process.
//! If initialization failed, make secure storage available and restart Kinema
//! before retrying. Surface Unavailable; never fall back to ordinary files.
use kinema_core::sources::DebridKind;
use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(test)]
pub(crate) static METADATA_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

static METADATA_GENERATION: AtomicU64 = AtomicU64::new(0);
pub(crate) fn metadata_generation() -> u64 {
    METADATA_GENERATION.load(Ordering::Acquire)
}
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

#[derive(Clone, Copy)]
enum CredentialOperation {
    Get,
    OptionalGet,
    Status,
    Set,
    Delete,
}
impl CredentialOperation {
    fn label(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::OptionalGet => "optional_get",
            Self::Status => "status",
            Self::Set => "set",
            Self::Delete => "delete",
        }
    }
}

fn diagnose<T>(
    account: &'static str,
    operation: CredentialOperation,
    result: Result<T, CredentialError>,
) -> Result<T, CredentialError> {
    let label = operation.label();
    match &result {
        Err(CredentialError::Missing)
            if matches!(
                operation,
                CredentialOperation::Status | CredentialOperation::OptionalGet
            ) =>
        {
            tracing::debug!(
                provider = account,
                operation = label,
                category = ?CredentialError::Missing,
                "Optional secure credential is not configured"
            );
        }
        Ok(_) => tracing::debug!(
            provider = account,
            operation = label,
            "Secure credential operation completed"
        ),
        Err(category) => tracing::warn!(
            provider = account,
            operation = label,
            ?category,
            "Secure credential operation failed"
        ),
    }
    result
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
        diagnose(
            debrid_account(kind),
            CredentialOperation::Get,
            self.vault.get(debrid_account(kind)),
        )
    }
    pub fn set(&self, kind: DebridKind, secret: SecretString) -> Result<(), CredentialError> {
        diagnose(
            debrid_account(kind),
            CredentialOperation::Set,
            self.vault.set(debrid_account(kind), secret),
        )
    }
    pub fn delete(&self, kind: DebridKind) -> Result<(), CredentialError> {
        diagnose(
            debrid_account(kind),
            CredentialOperation::Delete,
            self.vault.delete(debrid_account(kind)),
        )
    }
    /// Checks optional presence without exposing a token to the UI; any temporary is zeroized.
    /// Missing is expected absence, but secure-storage service failures remain errors.
    pub fn status(&self, kind: DebridKind) -> Result<bool, CredentialError> {
        match diagnose(
            debrid_account(kind),
            CredentialOperation::Status,
            self.vault.get(debrid_account(kind)),
        ) {
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
        diagnose("tmdb", CredentialOperation::Get, self.vault.get("tmdb"))
    }
    /// Optional provider discovery: absence is normal, service failures are not.
    pub fn get_optional(&self) -> Result<Option<SecretString>, CredentialError> {
        match diagnose(
            "tmdb",
            CredentialOperation::OptionalGet,
            self.vault.get("tmdb"),
        ) {
            Ok(secret) => Ok(Some(secret)),
            Err(CredentialError::Missing) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn set(&self, secret: SecretString) -> Result<(), CredentialError> {
        diagnose(
            "tmdb",
            CredentialOperation::Set,
            self.vault.set("tmdb", secret),
        )?;
        METADATA_GENERATION.fetch_add(1, Ordering::AcqRel);
        tracing::debug!(
            provider = "tmdb",
            reason = "credential_mutation",
            "Metadata credential generation advanced"
        );
        Ok(())
    }
    pub fn delete(&self) -> Result<(), CredentialError> {
        diagnose(
            "tmdb",
            CredentialOperation::Delete,
            self.vault.delete("tmdb"),
        )?;
        METADATA_GENERATION.fetch_add(1, Ordering::AcqRel);
        tracing::debug!(
            provider = "tmdb",
            reason = "credential_mutation",
            "Metadata credential generation advanced"
        );
        Ok(())
    }
    /// Checks optional presence; missing is not a secure-storage service failure.
    pub fn status(&self) -> Result<bool, CredentialError> {
        match diagnose("tmdb", CredentialOperation::Status, self.vault.get("tmdb")) {
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
pub(crate) fn test_metadata_store() -> MetadataCredentialStore {
    MetadataCredentialStore {
        vault: Box::<tests::FakeVault>::default(),
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
    pub(super) struct FakeVault(Arc<Mutex<HashMap<&'static str, SecretString>>>);
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
    fn credentials_diagnostics_never_format_tokens_or_backend_payloads()
    -> Result<(), CredentialError> {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "credentials_diagnostics_never_format_tokens_or_backend_payloads",
        ) {
            return Ok(());
        }

        use crate::providers::test_logging::{capture, excludes};
        let store = CredentialStore {
            vault: Box::<FakeVault>::default(),
        };
        let (result, output) = capture(|| {
            store.set(
                DebridKind::RealDebrid,
                SecretString::new("debrid-token-secret".into()),
            )?;
            let _secret = store.get(DebridKind::RealDebrid)?;
            let category = safe_error(keyring::Error::BadEncoding(
                b"backend-token-secret".to_vec(),
            ));
            assert_eq!(
                diagnose::<()>("tmdb", CredentialOperation::Get, Err(category)),
                Err(CredentialError::Unavailable)
            );
            let category = safe_error(keyring::Error::NoStorageAccess(Box::new(
                std::io::Error::other(
                    "backend-error-marker https://fixture.invalid/private?token=url-secret /synthetic/private/keyring",
                ),
            )));
            assert_eq!(
                diagnose::<()>("tmdb", CredentialOperation::Status, Err(category)),
                Err(CredentialError::AccessDenied)
            );
            store.delete(DebridKind::RealDebrid)?;
            Ok(())
        });
        result?;
        assert!(output.contains("Unavailable"));
        assert!(output.contains("AccessDenied"));
        excludes(
            &output,
            &[
                "debrid-token-secret",
                "backend-token-secret",
                "backend-error-marker",
                "fixture.invalid",
                "url-secret",
                "/synthetic/private/keyring",
            ],
        );
        Ok(())
    }

    struct FailedVault(CredentialError);
    impl Vault for FailedVault {
        fn get(&self, _: &'static str) -> Result<SecretString, CredentialError> {
            Err(self.0)
        }
        fn set(&self, _: &'static str, _: SecretString) -> Result<(), CredentialError> {
            Err(self.0)
        }
        fn delete(&self, _: &'static str) -> Result<(), CredentialError> {
            Err(self.0)
        }
    }

    #[test]
    fn optional_presence_and_required_operations_have_distinct_severity() {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "optional_presence_and_required_operations_have_distinct_severity",
        ) {
            return;
        }
        use crate::providers::test_logging::capture;
        for category in [
            CredentialError::Missing,
            CredentialError::Unavailable,
            CredentialError::AccessDenied,
        ] {
            let debrid = CredentialStore {
                vault: Box::new(FailedVault(category)),
            };
            let metadata = MetadataCredentialStore {
                vault: Box::new(FailedVault(category)),
            };
            let (results, output) = capture(|| {
                match metadata.get_optional() {
                    Ok(None) => assert_eq!(category, CredentialError::Missing),
                    Err(error) => {
                        assert_ne!(category, CredentialError::Missing);
                        assert_eq!(error, category);
                    }
                    Ok(Some(_)) => panic!("failing fixture vault returned a credential"),
                }
                [
                    debrid.status(DebridKind::RealDebrid),
                    debrid.status(DebridKind::AllDebrid),
                    metadata.status(),
                ]
            });
            let missing = category == CredentialError::Missing;
            let expected = if missing { Ok(false) } else { Err(category) };
            assert_eq!(results, [expected; 3]);
            assert_eq!(output.matches("WARN").count(), if missing { 0 } else { 4 });
            assert_eq!(output.matches("DEBUG").count(), if missing { 4 } else { 0 });
            assert_eq!(output.matches("operation=\"status\"").count(), 3);
            assert_eq!(output.matches("operation=\"optional_get\"").count(), 1);
            assert!(output.contains("real-debrid"));
            assert!(output.contains("all-debrid"));
            assert!(output.contains("tmdb"));

            let (_, output) = capture(|| {
                assert_eq!(debrid.get(DebridKind::RealDebrid).err(), Some(category));
                assert_eq!(metadata.get().err(), Some(category));
                assert_eq!(debrid.delete(DebridKind::RealDebrid), Err(category));
                assert_eq!(metadata.delete(), Err(category));
                assert_eq!(
                    debrid.set(
                        DebridKind::RealDebrid,
                        SecretString::new("fixture-secret".into())
                    ),
                    Err(category)
                );
                assert_eq!(
                    metadata.set(SecretString::new("fixture-secret".into())),
                    Err(category)
                );
            });
            assert_eq!(output.matches("WARN").count(), 6);
            assert!(!output.contains("fixture-secret"));
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
        let _guard = METADATA_TEST_LOCK
            .lock()
            .map_err(|_| "metadata test lock")?;
        use crate::{
            paths::AppPaths,
            settings::{Settings, SettingsStore},
        };
        let vault = FakeVault::default();
        let metadata = MetadataCredentialStore {
            vault: Box::new(vault.clone()),
        };
        let debrid = CredentialStore {
            vault: Box::new(vault),
        };
        assert!(!metadata.status()?);
        assert!(metadata.get_optional()?.is_none());
        let token = "tmdb-private-bearer-fixture";
        let before = metadata_generation();
        metadata.set(SecretString::new(token.into()))?;
        assert_eq!(metadata_generation(), before + 1);
        assert_eq!(metadata.get()?.expose(), token);
        assert_eq!(
            metadata.get_optional()?.as_ref().map(SecretString::expose),
            Some(token)
        );
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
            metadata_priority: kinema_core::provider::ProviderPriority::new([
                kinema_core::provider::ProviderId::Tmdb,
            ]),
            ..Settings::default()
        })?;
        let text = std::fs::read_to_string(root.path().join("settings.toml"))?;
        assert!(text.contains("metadata_providers = [\"tmdb\", \"cinemeta\"]"));
        assert!(!text.contains(token));
        assert!(!text.contains("bearer"));
        assert!(!format!("{:?}", metadata.get()?).contains(token));
        let before = metadata_generation();
        metadata.delete()?;
        assert_eq!(metadata_generation(), before + 1);
        assert!(!metadata.status()?);
        assert!(metadata.get_optional()?.is_none());
        assert_eq!(metadata.delete(), Err(CredentialError::Missing));
        assert_eq!(metadata_generation(), before + 1);
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
