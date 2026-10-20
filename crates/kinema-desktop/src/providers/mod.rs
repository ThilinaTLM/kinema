//! Credentialed provider adapters. No upstream error payloads escape this module.
mod all_debrid;
pub(crate) mod http;
mod real_debrid;
#[cfg(test)]
pub(crate) mod test_logging;

use crate::credentials::SecretString;
use kinema_core::sources::{DebridKind, MediaIdentity, RemoteFile, TorrentSource};
use reqwest::{Url, blocking::Client};
use std::{fmt, sync::atomic::AtomicBool};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebridError {
    CredentialRejected,
    Transport,
    Service,
    RateLimited,
    Malformed,
    TooLarge,
    Canceled,
    PendingTimeout,
    Failed,
    UnknownSize,
    InvalidUrl,
    FileNotFound,
    ProviderMismatch,
}
impl fmt::Display for DebridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CredentialRejected => "The provider rejected the credential.",
            Self::Transport => "Provider connection failed.",
            Self::Service => "Provider service failed.",
            Self::RateLimited => "Provider rate limit reached.",
            Self::Malformed => "Invalid provider response.",
            Self::TooLarge => "Provider response exceeds limits.",
            Self::Canceled => "Operation canceled.",
            Self::PendingTimeout => "Provider is still preparing this torrent; retry later.",
            Self::Failed => "Provider could not prepare this torrent.",
            Self::UnknownSize => "File size is unavailable.",
            Self::InvalidUrl => "Provider returned an invalid media URL.",
            Self::FileNotFound => "File is no longer available.",
            Self::ProviderMismatch => "The prepared torrent belongs to another provider.",
        })
    }
}
impl std::error::Error for DebridError {}

#[derive(Clone)]
pub struct SensitiveUrl(Url);
impl SensitiveUrl {
    pub fn new(url: Url) -> Result<Self, DebridError> {
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            tracing::warn!(
                category = "invalid_media_url",
                "Provider media URL validation failed"
            );
            return Err(DebridError::InvalidUrl);
        }
        Ok(Self(url))
    }
    /// Local media fixtures only; never compiled into production builds.
    #[cfg(test)]
    pub(crate) fn for_test(url: Url) -> Self {
        Self(url)
    }
    pub fn as_url(&self) -> &Url {
        &self.0
    }
}
impl fmt::Debug for SensitiveUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SensitiveUrl([REDACTED])")
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedTorrent {
    pub provider: DebridKind,
    pub source: TorrentSource,
    pub job_id: String,
    pub files: Vec<RemoteFile>,
}
impl fmt::Debug for PreparedTorrent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedTorrent")
            .field("provider", &self.provider)
            .field("torrent", &self.source.hash)
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct ResolvedMedia {
    pub identity: MediaIdentity,
    pub file_name: String,
    pub url: SensitiveUrl,
    pub expires_at: Option<u64>,
}

impl fmt::Debug for ResolvedMedia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedMedia")
            .field("torrent", &self.identity.torrent)
            .field("size", &self.identity.size)
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

pub struct DebridClient {
    kind: DebridKind,
    secret: SecretString,
    client: Client,
    base: Url,
    polls: usize,
}
impl DebridClient {
    pub fn new(kind: DebridKind, secret: SecretString) -> Result<Self, DebridError> {
        let _span = tracing::info_span!("debrid", operation = "new", provider = ?kind).entered();
        if secret.expose().trim().is_empty() {
            tracing::warn!(
                category = "empty_credential",
                "Debrid client initialization rejected"
            );
            return Err(DebridError::CredentialRejected);
        }
        let base = match kind {
            DebridKind::RealDebrid => "https://api.real-debrid.com/rest/1.0/",
            DebridKind::AllDebrid => "https://api.alldebrid.com/",
        };
        Ok(Self {
            kind,
            secret,
            client: http::client()?,
            base: Url::parse(base).map_err(|_| DebridError::InvalidUrl)?,
            polls: http::POLLS,
        })
    }
    /// Verifies authentication only, not premium entitlement or playback readiness.
    /// Account details are validated and discarded. Call from an owned background worker.
    pub fn check_account(&self, cancel: &AtomicBool) -> Result<(), DebridError> {
        let _span =
            tracing::info_span!("debrid", operation = "check_account", provider = ?self.kind)
                .entered();
        let started = std::time::Instant::now();
        let result = (|| {
            http::check(cancel)?;
            match self.kind {
                DebridKind::RealDebrid => real_debrid::check_account(self, cancel),
                DebridKind::AllDebrid => all_debrid::check_account(self, cancel),
            }
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "debrid operation completed"
            ),
            Err(category) => {
                if *category == DebridError::Canceled {
                    tracing::debug!("debrid operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "debrid operation failed"
                    );
                }
            }
        }
        result
    }
    pub fn prepare(
        &self,
        source: &TorrentSource,
        cancel: &AtomicBool,
    ) -> Result<PreparedTorrent, DebridError> {
        let _span =
            tracing::info_span!("debrid", operation = "prepare", provider = ?self.kind).entered();
        let started = std::time::Instant::now();
        let result = (|| {
            http::check(cancel)?;
            match self.kind {
                DebridKind::RealDebrid => real_debrid::prepare(self, source, cancel),
                DebridKind::AllDebrid => all_debrid::prepare(self, source, cancel),
            }
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "debrid operation completed"
            ),
            Err(category) => {
                if *category == DebridError::Canceled {
                    tracing::debug!("debrid operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "debrid operation failed"
                    );
                }
            }
        }
        if let Ok(prepared) = &result {
            tracing::debug!(files = prepared.files.len(), "Debrid files prepared");
        }
        result
    }
    pub fn resolve_file(
        &self,
        prepared: &PreparedTorrent,
        file_id: &str,
        cancel: &AtomicBool,
    ) -> Result<ResolvedMedia, DebridError> {
        let _span =
            tracing::info_span!("debrid", operation = "resolve_file", provider = ?self.kind)
                .entered();
        let started = std::time::Instant::now();
        let result = (|| {
            http::check(cancel)?;
            if prepared.provider != self.kind {
                return Err(DebridError::ProviderMismatch);
            }
            let file = prepared
                .files
                .iter()
                .find(|file| file.id == file_id)
                .ok_or(DebridError::FileNotFound)?;
            if file.size == 0 {
                return Err(DebridError::UnknownSize);
            }
            match self.kind {
                DebridKind::RealDebrid => real_debrid::resolve(self, prepared, file, cancel),
                DebridKind::AllDebrid => all_debrid::resolve(self, prepared, file, cancel),
            }
        })();
        match &result {
            Ok(_) => tracing::debug!(
                duration_ms = started.elapsed().as_millis() as u64,
                "debrid operation completed"
            ),
            Err(category) => {
                if *category == DebridError::Canceled {
                    tracing::debug!("debrid operation canceled");
                } else {
                    tracing::warn!(
                        ?category,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "debrid operation failed"
                    );
                }
            }
        }
        result
    }
    fn form(
        &self,
        segments: &[&str],
        fields: &[(&str, &str)],
    ) -> Result<reqwest::blocking::RequestBuilder, DebridError> {
        let mut encoded = self.base.clone();
        encoded.set_query(None);
        encoded
            .query_pairs_mut()
            .extend_pairs(fields.iter().copied());
        Ok(self
            .request(reqwest::Method::POST, segments)?
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(encoded.query().unwrap_or_default().to_owned()))
    }
    fn request(
        &self,
        method: reqwest::Method,
        segments: &[&str],
    ) -> Result<reqwest::blocking::RequestBuilder, DebridError> {
        Ok(self
            .client
            .request(method, http::endpoint(&self.base, segments)?)
            .bearer_auth(self.secret.expose()))
    }
}
fn resolved(
    prepared: &PreparedTorrent,
    file: &RemoteFile,
    link: &str,
) -> Result<ResolvedMedia, DebridError> {
    Ok(ResolvedMedia {
        identity: MediaIdentity {
            torrent: prepared.source.hash.clone(),
            file_path: file.path.clone(),
            size: file.size,
        },
        file_name: file
            .path
            .rsplit('/')
            .next()
            .filter(|n| !n.is_empty())
            .ok_or(DebridError::Malformed)?
            .to_owned(),
        url: SensitiveUrl::new(Url::parse(link).map_err(|_| DebridError::InvalidUrl)?)?,
        expires_at: None,
    })
}

#[cfg(test)]
pub(crate) mod tests;
