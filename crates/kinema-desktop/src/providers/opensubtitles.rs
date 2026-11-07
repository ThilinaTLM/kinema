//! OpenSubtitles.com REST adapter (blocking). Call only from an owned background worker.
//!
//! Auth model: the user's own `Api-Key` authorises every request; an optional
//! `POST /login` token (`Authorization: Bearer`) authorises the user and, via the
//! returned `base_url`, may move later requests to `vip-api.opensubtitles.com`.
//!
//! Logging allowlist: hosts, status codes, counts, durations and fixed categories.
//! Never the key, token, credentials, query, file names, or download links.
//!
//! Assumptions about behaviour the public reference leaves ambiguous:
//! - 401/403 on search or user info is an application-key problem when anonymous;
//!   with a session, 401 means the 24-hour token expired (`SessionExpired`).
//! - `406` from `/download` is the allowance-exhausted reply; its body carries
//!   `reset_time_utc`. A `200` with `remaining: 0` still returns a link (the
//!   allowance was spent on that call), so it succeeds with `remaining: Some(0)`.
//! - A `403` on `/login` is a rejected application key; only `401` is bad credentials.

use crate::credentials::SecretString;
use crate::logging::{http_error_kind, safe_url};
use kinema_core::{
    subtitle_search::{SubtitleQuery, SubtitleResult, SubtitleTarget, rank_results},
    subtitles::MAX_SUBTITLE_BYTES,
};
use reqwest::{
    StatusCode, Url,
    blocking::{Client, RequestBuilder, Response},
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT},
};
use serde_json::Value;
use std::{
    collections::hash_map::DefaultHasher,
    fmt,
    hash::{Hash, Hasher},
    io::Read,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const API_HOST: &str = "api.opensubtitles.com";
const VIP_HOST: &str = "vip-api.opensubtitles.com";
const LINK_HOST: &str = "www.opensubtitles.com";
const MAX_JSON: u64 = super::http::MAX_BODY;
/// Error bodies are only mined for a reset time; keep them small.
const MAX_ERROR_BODY: u64 = 64 * 1024;
const MAX_ITEMS: usize = 500;
const MAX_FILES_PER_ITEM: usize = 32;
/// Documented limit: 5 requests per second per IP.
const REQUEST_SPACING: Duration = Duration::from_millis(200);
/// Documented limit: one login per second.
const LOGIN_SPACING: Duration = Duration::from_secs(1);

/// `Kinema v<version>`, the form the service requires.
pub fn default_user_agent() -> String {
    format!("Kinema v{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenSubtitlesError {
    /// Local configuration (blank key, unusable user agent, unusable query).
    Configuration,
    ServiceFailure,
    RateLimited,
    QuotaExhausted {
        reset_time_utc: Option<String>,
    },
    ApiKeyRejected,
    /// Anonymous download refused: the user must sign in.
    SignInRequired,
    /// The bearer token is no longer accepted (24-hour lifetime).
    SessionExpired,
    LoginRejected,
    Network,
    Cancelled,
    InvalidResponse,
}
impl fmt::Display for OpenSubtitlesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Configuration => "OpenSubtitles is not configured correctly.",
            Self::ServiceFailure => "OpenSubtitles is unavailable.",
            Self::RateLimited => "OpenSubtitles rate limit reached; try again shortly.",
            Self::QuotaExhausted { .. } => "The OpenSubtitles download allowance is used up.",
            Self::ApiKeyRejected => "OpenSubtitles rejected the API key.",
            Self::SignInRequired => "Sign in to OpenSubtitles to download subtitles.",
            Self::SessionExpired => "The OpenSubtitles session expired; sign in again.",
            Self::LoginRejected => "OpenSubtitles rejected the username or password.",
            Self::Network => "OpenSubtitles connection failed.",
            Self::Cancelled => "Operation canceled.",
            Self::InvalidResponse => "OpenSubtitles returned an invalid response.",
        })
    }
}
impl std::error::Error for OpenSubtitlesError {}
type Result<T> = std::result::Result<T, OpenSubtitlesError>;

/// A signed-in user. The token is redacted from `Debug`.
#[derive(Clone)]
pub struct Session {
    token: Zeroizing<String>,
    /// `api.opensubtitles.com` or `vip-api.opensubtitles.com`.
    pub base_host: String,
    pub allowed_downloads: Option<u32>,
    pub remaining_downloads: Option<u32>,
}
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("base_host", &self.base_host)
            .field("allowed_downloads", &self.allowed_downloads)
            .field("remaining_downloads", &self.remaining_downloads)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserInfo {
    pub allowed_downloads: Option<u32>,
    pub remaining_downloads: Option<u32>,
    pub vip: bool,
}

pub struct DownloadedSubtitle {
    pub bytes: Vec<u8>,
    pub file_name: Option<String>,
    pub format: Option<String>,
    pub remaining: Option<u32>,
    pub reset_time_utc: Option<String>,
}
impl fmt::Debug for DownloadedSubtitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DownloadedSubtitle")
            .field("bytes", &self.bytes.len())
            .field("remaining", &self.remaining)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Endpoint {
    Login,
    Search,
    UserInfo,
    Download,
    Link,
}

pub struct OpenSubtitlesClient {
    api_key: HeaderValue,
    user_agent: HeaderValue,
    client: Client,
    api_base: Url,
    vip_base: Url,
    spacing: Duration,
    login_spacing: Duration,
    last_request: Mutex<Option<Instant>>,
    last_login: Mutex<Option<Instant>>,
    /// Fingerprint (not the password) of credentials the service already rejected.
    rejected_login: Mutex<Option<u64>>,
    #[cfg(test)]
    loopback_links: bool,
}

impl OpenSubtitlesClient {
    pub fn new(api_key: SecretString, user_agent: impl Into<String>) -> Result<Self> {
        let key = api_key.expose().trim();
        if key.is_empty() {
            return Err(OpenSubtitlesError::ApiKeyRejected);
        }
        let mut api_key =
            HeaderValue::from_str(key).map_err(|_| OpenSubtitlesError::ApiKeyRejected)?;
        api_key.set_sensitive(true);
        let user_agent = HeaderValue::from_str(&user_agent.into())
            .map_err(|_| OpenSubtitlesError::Configuration)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                tracing::warn!(
                    operation = "build_client",
                    kind = http_error_kind(&error),
                    "HTTP client initialization failed"
                );
                OpenSubtitlesError::Configuration
            })?;
        let base = |host: &str| Url::parse(&format!("https://{host}/api/v1/"));
        let (api_base, vip_base) = match (base(API_HOST), base(VIP_HOST)) {
            (Ok(a), Ok(v)) => (a, v),
            _ => return Err(OpenSubtitlesError::Configuration),
        };
        Ok(Self {
            api_key,
            user_agent,
            client,
            api_base,
            vip_base,
            spacing: REQUEST_SPACING,
            login_spacing: LOGIN_SPACING,
            last_request: Mutex::new(None),
            last_login: Mutex::new(None),
            rejected_login: Mutex::new(None),
            #[cfg(test)]
            loopback_links: false,
        })
    }

    /// Loopback fixtures only; never compiled into production builds.
    #[cfg(test)]
    fn for_test(api_base: Url, vip_base: Url) -> Result<Self> {
        let mut client = Self::new(SecretString::new("test-key".into()), "Kinema vtest")?;
        client.api_base = api_base;
        client.vip_base = vip_base;
        client.spacing = Duration::ZERO;
        client.login_spacing = Duration::ZERO;
        client.loopback_links = true;
        Ok(client)
    }

    pub fn login(
        &self,
        username: &str,
        password: &SecretString,
        cancel: &AtomicBool,
    ) -> Result<Session> {
        let _span = tracing::info_span!("opensubtitles", operation = "login").entered();
        let result = self.login_inner(username, password, cancel);
        log_outcome(
            &result,
            |s| tracing::debug!(host = %s.base_host, "OpenSubtitles login completed"),
        );
        result
    }

    fn login_inner(
        &self,
        username: &str,
        password: &SecretString,
        cancel: &AtomicBool,
    ) -> Result<Session> {
        check(cancel)?;
        let fingerprint = fingerprint(username, password.expose());
        if *lock(&self.rejected_login) == Some(fingerprint) {
            // The service said never to retry rejected credentials.
            tracing::debug!(category = "login_not_retried", "Login skipped");
            return Err(OpenSubtitlesError::LoginRejected);
        }
        space(&self.last_login, self.login_spacing, cancel)?;
        let url = self
            .api_base
            .join("login")
            .map_err(|_| OpenSubtitlesError::Configuration)?;
        let body = serde_json::json!({ "username": username, "password": password.expose() });
        let request = self
            .client
            .post(url)
            .headers_common(self, None)
            .body(json_body(&body)?);
        let response = match self.execute(request, Endpoint::Login, false, cancel) {
            Err(OpenSubtitlesError::LoginRejected) => {
                *lock(&self.rejected_login) = Some(fingerprint);
                return Err(OpenSubtitlesError::LoginRejected);
            }
            other => other?,
        };
        let value = read_json(response, cancel)?;
        let token = value
            .get("token")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .ok_or(OpenSubtitlesError::InvalidResponse)?;
        let base_host = match value.get("base_url").and_then(Value::as_str) {
            None | Some("") => API_HOST.to_owned(),
            Some(raw) => allowed_api_host(raw).ok_or_else(|| {
                tracing::warn!(category = "host_not_allowed", "Login base host rejected");
                OpenSubtitlesError::InvalidResponse
            })?,
        };
        let user = value.get("user");
        let number = |key: &str| {
            user.and_then(|u| u.get(key))
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
        };
        Ok(Session {
            token: Zeroizing::new(token.to_owned()),
            base_host,
            allowed_downloads: number("allowed_downloads"),
            remaining_downloads: number("remaining_downloads"),
        })
    }

    pub fn user_info(&self, session: &Session, cancel: &AtomicBool) -> Result<UserInfo> {
        let _span = tracing::info_span!("opensubtitles", operation = "user_info").entered();
        let result = (|| {
            check(cancel)?;
            let url = self
                .base_for(Some(session))
                .join("infos/user")
                .map_err(|_| OpenSubtitlesError::Configuration)?;
            let request = self.client.get(url).headers_common(self, Some(session));
            let response = self.execute(request, Endpoint::UserInfo, true, cancel)?;
            let value = read_json(response, cancel)?;
            let data = value
                .get("data")
                .filter(|d| d.is_object())
                .ok_or(OpenSubtitlesError::InvalidResponse)?;
            let number = |key: &str| {
                data.get(key)
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
            };
            Ok(UserInfo {
                allowed_downloads: number("allowed_downloads"),
                remaining_downloads: number("remaining_downloads"),
                vip: data.get("vip").and_then(Value::as_bool).unwrap_or(false),
            })
        })();
        log_outcome(&result, |_| {
            tracing::debug!("OpenSubtitles user info completed")
        });
        result
    }

    /// Returns ranked results. An empty `Vec` is a successful empty search.
    pub fn search(
        &self,
        query: &SubtitleQuery,
        session: Option<&Session>,
        cancel: &AtomicBool,
    ) -> Result<Vec<SubtitleResult>> {
        let _span = tracing::info_span!("opensubtitles", operation = "search").entered();
        let result = (|| {
            check(cancel)?;
            let canonical = canonical_query(query)?;
            let mut url = self
                .base_for(session)
                .join("subtitles")
                .map_err(|_| OpenSubtitlesError::Configuration)?;
            url.set_query(Some(&canonical));
            let request = self.client.get(url).headers_common(self, session);
            let response = self.execute(request, Endpoint::Search, session.is_some(), cancel)?;
            let value = read_json(response, cancel)?;
            Ok(rank_results(parse_results(&value)?, &query.languages))
        })();
        log_outcome(&result, |r| {
            tracing::debug!(results = r.len(), "OpenSubtitles search completed")
        });
        result
    }

    /// Spends one download allowance (unless the file was fetched recently), then
    /// fetches the temporary link. The link is never logged or returned.
    pub fn download(
        &self,
        file_id: u64,
        session: Option<&Session>,
        cancel: &AtomicBool,
    ) -> Result<DownloadedSubtitle> {
        let _span = tracing::info_span!("opensubtitles", operation = "download").entered();
        let result = (|| {
            check(cancel)?;
            let url = self
                .base_for(session)
                .join("download")
                .map_err(|_| OpenSubtitlesError::Configuration)?;
            let request = self
                .client
                .post(url)
                .headers_common(self, session)
                .body(json_body(&serde_json::json!({ "file_id": file_id }))?);
            let response = self.execute(request, Endpoint::Download, session.is_some(), cancel)?;
            let value = read_json(response, cancel)?;
            let link = value
                .get("link")
                .and_then(Value::as_str)
                .ok_or(OpenSubtitlesError::InvalidResponse)?;
            let link = self.validate_link(link)?;
            let file_name = value
                .get("file_name")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty() && n.len() <= 512)
                .map(str::to_owned);
            let remaining = value
                .get("remaining")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok());
            let reset_time_utc = reset_time(&value);
            // The link host receives no key or token.
            space(&self.last_request, self.spacing, cancel)?;
            let request = self
                .client
                .get(link)
                .header(USER_AGENT, self.user_agent.clone());
            let response = self.execute(request, Endpoint::Link, false, cancel)?;
            if response
                .content_length()
                .is_some_and(|n| n > MAX_SUBTITLE_BYTES)
            {
                return Err(OpenSubtitlesError::InvalidResponse);
            }
            let bytes = read_limited(response, MAX_SUBTITLE_BYTES, cancel)?;
            if bytes.is_empty() {
                return Err(OpenSubtitlesError::InvalidResponse);
            }
            let format = file_name.as_deref().and_then(extension);
            Ok(DownloadedSubtitle {
                bytes,
                file_name,
                format,
                remaining,
                reset_time_utc,
            })
        })();
        log_outcome(&result, |d| {
            tracing::debug!(
                bytes = d.bytes.len(),
                remaining = ?d.remaining,
                "OpenSubtitles download completed"
            )
        });
        result
    }

    fn base_for(&self, session: Option<&Session>) -> &Url {
        match session {
            Some(s) if s.base_host == VIP_HOST => &self.vip_base,
            _ => &self.api_base,
        }
    }

    fn validate_link(&self, link: &str) -> Result<Url> {
        let url = Url::parse(link).map_err(|_| OpenSubtitlesError::InvalidResponse)?;
        let plain = url.username().is_empty() && url.password().is_none();
        let production = url.scheme() == "https" && url.host_str() == Some(LINK_HOST);
        #[cfg(test)]
        let production = production
            || (self.loopback_links
                && url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1"));
        if !plain || !production {
            tracing::warn!(
                category = "host_not_allowed",
                host = %safe_url(&url),
                "Download link rejected"
            );
            return Err(OpenSubtitlesError::InvalidResponse);
        }
        Ok(url)
    }

    /// Sends one request, mapping transport failures and non-2xx statuses.
    fn execute(
        &self,
        request: RequestBuilder,
        endpoint: Endpoint,
        has_session: bool,
        cancel: &AtomicBool,
    ) -> Result<Response> {
        if endpoint != Endpoint::Link {
            space(&self.last_request, self.spacing, cancel)?;
        }
        let started = Instant::now();
        let response = request.send().map_err(|error| {
            tracing::warn!(
                kind = http_error_kind(&error),
                duration_ms = started.elapsed().as_millis() as u64,
                "HTTP transport failed"
            );
            OpenSubtitlesError::Network
        })?;
        check(cancel)?;
        let status = response.status();
        let host = safe_url(response.url());
        tracing::debug!(
            status = status.as_u16(),
            host,
            duration_ms = started.elapsed().as_millis() as u64,
            "HTTP headers received"
        );
        if status.is_success() {
            return Ok(response);
        }
        let error = match map_status(status, endpoint, has_session) {
            OpenSubtitlesError::QuotaExhausted { .. } => {
                let reset = read_limited(response, MAX_ERROR_BODY, cancel)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                    .and_then(|v| reset_time(&v));
                OpenSubtitlesError::QuotaExhausted {
                    reset_time_utc: reset,
                }
            }
            other => other,
        };
        tracing::warn!(
            status = status.as_u16(),
            host,
            category = ?error,
            "HTTP service rejected request"
        );
        Err(error)
    }
}

trait CommonHeaders {
    fn headers_common(self, client: &OpenSubtitlesClient, session: Option<&Session>) -> Self;
}
impl CommonHeaders for RequestBuilder {
    fn headers_common(self, client: &OpenSubtitlesClient, session: Option<&Session>) -> Self {
        let mut request = self
            .header(HeaderName::from_static("api-key"), client.api_key.clone())
            .header(USER_AGENT, client.user_agent.clone())
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json");
        if let Some(session) = session
            && let Ok(mut bearer) = HeaderValue::from_str(&format!("Bearer {}", *session.token))
        {
            bearer.set_sensitive(true);
            request = request.header(AUTHORIZATION, bearer);
        }
        request
    }
}

fn map_status(status: StatusCode, endpoint: Endpoint, has_session: bool) -> OpenSubtitlesError {
    use OpenSubtitlesError as E;
    match (status.as_u16(), endpoint) {
        (401, Endpoint::Login) => E::LoginRejected,
        (401, Endpoint::Download) if !has_session => E::SignInRequired,
        (401, _) if has_session => E::SessionExpired,
        (401 | 403, _) => E::ApiKeyRejected,
        (406, Endpoint::Download) => E::QuotaExhausted {
            reset_time_utc: None,
        },
        (429, _) => E::RateLimited,
        // 3xx (redirects are disabled), other 4xx and 5xx.
        _ => E::ServiceFailure,
    }
}

fn json_body(value: &Value) -> Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| OpenSubtitlesError::Configuration)
}

fn reset_time(value: &Value) -> Option<String> {
    value
        .get("reset_time_utc")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 64)
        .map(str::to_owned)
}

fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(OpenSubtitlesError::Cancelled)
    } else {
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Reserves the next slot at least `interval` after the previous one and waits
/// for it, observing cancellation.
fn space(last: &Mutex<Option<Instant>>, interval: Duration, cancel: &AtomicBool) -> Result<()> {
    let due = {
        let mut guard = lock(last);
        let now = Instant::now();
        let due = guard.map_or(now, |previous| (previous + interval).max(now));
        *guard = Some(due);
        due
    };
    loop {
        check(cancel)?;
        let now = Instant::now();
        if now >= due {
            return Ok(());
        }
        std::thread::sleep((due - now).min(Duration::from_millis(50)));
    }
}

fn fingerprint(username: &str, password: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    (username, password).hash(&mut hasher);
    hasher.finish()
}

/// Accepts `host`, `https://host` or `https://host/`, only for the two API hosts.
fn allowed_api_host(raw: &str) -> Option<String> {
    let host = raw
        .trim()
        .strip_prefix("https://")
        .unwrap_or(raw.trim())
        .trim_end_matches('/');
    matches!(host, API_HOST | VIP_HOST).then(|| host.to_owned())
}

fn read_limited(response: Response, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            tracing::warn!(io_kind = ?error.kind(), "HTTP body read failed");
            OpenSubtitlesError::Network
        })?;
    check(cancel)?;
    if bytes.len() as u64 > limit {
        tracing::warn!(category = "body_too_large", "HTTP body exceeds limit");
        return Err(OpenSubtitlesError::InvalidResponse);
    }
    Ok(bytes)
}

fn read_json(response: Response, cancel: &AtomicBool) -> Result<Value> {
    if response.content_length().is_some_and(|n| n > MAX_JSON) {
        tracing::warn!(category = "body_too_large", "HTTP body exceeds limit");
        return Err(OpenSubtitlesError::InvalidResponse);
    }
    let bytes = read_limited(response, MAX_JSON, cancel)?;
    serde_json::from_slice(&bytes).map_err(|_| OpenSubtitlesError::InvalidResponse)
}

fn log_outcome<T>(result: &Result<T>, ok: impl FnOnce(&T)) {
    match result {
        Ok(value) => ok(value),
        Err(OpenSubtitlesError::Cancelled) => tracing::debug!("OpenSubtitles operation canceled"),
        Err(error) => {
            let category = match error {
                OpenSubtitlesError::QuotaExhausted { .. } => "QuotaExhausted".to_owned(),
                other => format!("{other:?}"),
            };
            tracing::warn!(category, "OpenSubtitles operation failed");
        }
    }
}

// ---- Search request and response ----------------------------------------

/// Percent-encodes a query value: unreserved bytes kept, space as `+`.
fn encode_value(value: &str, out: &mut String) {
    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
}

/// Parameters sorted alphabetically, lowercase, defaults omitted, no leading
/// zeros, languages sorted and comma-joined, so the service need not redirect.
fn canonical_query(query: &SubtitleQuery) -> Result<String> {
    let mut params: Vec<(&str, String)> = Vec::new();
    match query.target {
        Some(SubtitleTarget::Movie { imdb }) => params.push(("imdb_id", imdb.to_string())),
        Some(SubtitleTarget::Episode {
            series_imdb,
            season,
            episode,
        }) => {
            params.push(("episode_number", episode.to_string()));
            params.push(("parent_imdb_id", series_imdb.to_string()));
            params.push(("season_number", season.to_string()));
        }
        None => {}
    }
    let text = query
        .text
        .as_deref()
        .map(|t| {
            t.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .filter(|t| !t.is_empty());
    if query.target.is_none() && text.is_none() && query.moviehash.is_none() {
        return Err(OpenSubtitlesError::Configuration);
    }
    if let Some(text) = text {
        params.push(("query", text));
    }
    let mut languages: Vec<String> = query
        .languages
        .iter()
        .map(|l| l.trim().to_ascii_lowercase())
        .filter(|l| {
            (2..=8).contains(&l.len()) && l.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
        })
        .collect();
    languages.sort();
    languages.dedup();
    if !languages.is_empty() {
        params.push(("languages", languages.join(",")));
    }
    if let Some(hash) = query.moviehash {
        params.push(("moviehash", format!("{hash:016x}")));
    }
    params.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::new();
    for (name, value) in params {
        if !out.is_empty() {
            out.push('&');
        }
        out.push_str(name);
        out.push('=');
        if name == "languages" {
            out.push_str(&value); // validated [a-z,-] only; commas stay literal
        } else {
            encode_value(&value, &mut out);
        }
    }
    Ok(out)
}

fn extension(file_name: &str) -> Option<String> {
    let (_, ext) = file_name.rsplit_once('.')?;
    (!ext.is_empty() && ext.len() <= 5 && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| ext.to_ascii_lowercase())
}

fn parse_results(value: &Value) -> Result<Vec<SubtitleResult>> {
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(OpenSubtitlesError::InvalidResponse)?;
    let mut results = Vec::new();
    let mut malformed = 0usize;
    for item in items.iter().take(MAX_ITEMS) {
        let before = results.len();
        parse_item(item, &mut results);
        if results.len() == before {
            malformed += 1;
        }
    }
    if malformed > 0 {
        tracing::debug!(malformed, "OpenSubtitles items skipped");
    }
    if results.is_empty() && !items.is_empty() {
        return Err(OpenSubtitlesError::InvalidResponse);
    }
    Ok(results)
}

fn parse_item(item: &Value, results: &mut Vec<SubtitleResult>) {
    let Some(attrs) = item.get("attributes").filter(|a| a.is_object()) else {
        return;
    };
    let Some(language) = attrs
        .get("language")
        .and_then(Value::as_str)
        .map(|l| l.trim().to_ascii_lowercase())
        .filter(|l| !l.is_empty() && l.len() <= 16)
    else {
        return;
    };
    let flag = |key: &str| attrs.get(key).and_then(Value::as_bool).unwrap_or(false);
    let release = attrs
        .get("release")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(512)
        .collect::<String>();
    let download_count = attrs
        .get("download_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let attr_format = attrs
        .get("format")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .filter(|f| !f.is_empty() && f.len() <= 5 && f.bytes().all(|b| b.is_ascii_alphanumeric()));
    let Some(files) = attrs.get("files").and_then(Value::as_array) else {
        return;
    };
    for file in files.iter().take(MAX_FILES_PER_ITEM) {
        let Some(file_id) = file.get("file_id").and_then(Value::as_u64) else {
            continue;
        };
        let file_name = file
            .get("file_name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty() && n.len() <= 512)
            .map(str::to_owned);
        let format = file_name
            .as_deref()
            .and_then(extension)
            .or_else(|| attr_format.clone());
        results.push(SubtitleResult {
            file_id,
            language: language.clone(),
            release: release.clone(),
            file_name,
            download_count,
            hearing_impaired: flag("hearing_impaired"),
            foreign_parts_only: flag("foreign_parts_only"),
            ai_translated: flag("ai_translated"),
            machine_translated: flag("machine_translated"),
            from_trusted: flag("from_trusted"),
            moviehash_match: flag("moviehash_match"),
            format,
        });
    }
}

#[cfg(test)]
mod tests;
