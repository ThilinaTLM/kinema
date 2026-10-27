//! Bounded blocking HTTP. Call only from an owned background worker.
use super::DebridError;
use crate::logging::{http_error_kind, safe_url};
use reqwest::{
    Url,
    blocking::{Client, Response},
};
use serde_json::Value;
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(crate) const MAX_BODY: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_FILES: usize = 4096;
pub(crate) const MAX_PATH: usize = 4096;
pub(crate) const POLLS: usize = 30;

pub(crate) fn client() -> Result<Client, DebridError> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Kinema/0.1")
        .build()
        .map_err(|error| {
            tracing::warn!(
                operation = "build_client",
                kind = http_error_kind(&error),
                "HTTP client initialization failed"
            );
            DebridError::Transport
        })
}
pub(crate) fn check(cancel: &AtomicBool) -> Result<(), DebridError> {
    if cancel.load(Ordering::Relaxed) {
        Err(DebridError::Canceled)
    } else {
        Ok(())
    }
}
pub(crate) fn pause(cancel: &AtomicBool) -> Result<(), DebridError> {
    for _ in 0..10 {
        check(cancel)?;
        std::thread::sleep(Duration::from_millis(100));
    }
    check(cancel)
}
// The enclosing provider/indexer span supplies a static operation and provider.
// Never inspect request bodies, headers, paths, or response/error text.
pub(super) fn send(
    request: reqwest::blocking::RequestBuilder,
    cancel: &AtomicBool,
) -> Result<Response, DebridError> {
    check(cancel)?;
    let start = Instant::now();
    let response = request.send().map_err(|error| {
        tracing::warn!(
            kind = http_error_kind(&error),
            duration_ms = start.elapsed().as_millis() as u64,
            "HTTP transport failed"
        );
        DebridError::Transport
    })?;
    check(cancel)?;
    let status = response.status().as_u16();
    let host = safe_url(response.url());
    tracing::debug!(
        status,
        host,
        duration_ms = start.elapsed().as_millis() as u64,
        "HTTP headers received"
    );
    let error = match status {
        401 | 403 => Some(DebridError::CredentialRejected),
        429 => Some(DebridError::RateLimited),
        200..=299 => None,
        _ => Some(DebridError::Service),
    };
    if let Some(category) = error {
        tracing::warn!(
            status,
            host,
            ?category,
            duration_ms = start.elapsed().as_millis() as u64,
            "HTTP service rejected request"
        );
        return Err(category);
    }
    Ok(response)
}
pub(crate) fn json(
    request: reqwest::blocking::RequestBuilder,
    cancel: &AtomicBool,
) -> Result<Value, DebridError> {
    let start = Instant::now();
    let result = (|| {
        let response = send(request, cancel)?;
        if response.content_length().is_some_and(|n| n > MAX_BODY) {
            return Err(DebridError::TooLarge);
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_BODY + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                tracing::warn!(io_kind = ?error.kind(), "HTTP body read failed");
                DebridError::Transport
            })?;
        check(cancel)?;
        if bytes.len() as u64 > MAX_BODY {
            return Err(DebridError::TooLarge);
        }
        let value = serde_json::from_slice(&bytes).map_err(|_| DebridError::Malformed)?;
        tracing::debug!(
            bytes = bytes.len(),
            duration_ms = start.elapsed().as_millis() as u64,
            "HTTP JSON decoded"
        );
        Ok(value)
    })();
    if let Err(category) = &result {
        if *category == DebridError::Canceled {
            tracing::debug!("HTTP operation canceled");
        } else {
            tracing::warn!(
                ?category,
                duration_ms = start.elapsed().as_millis() as u64,
                "HTTP JSON operation failed"
            );
        }
    }
    result
}
pub(crate) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, DebridError> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(DebridError::Malformed)
}
pub(crate) fn id(v: &Value) -> Result<String, DebridError> {
    match v {
        Value::String(s) if !s.is_empty() => Ok(s.clone()),
        Value::Number(n) if n.is_u64() => Ok(n.to_string()),
        _ => Err(DebridError::Malformed),
    }
}
pub(crate) fn endpoint(base: &Url, segments: &[&str]) -> Result<Url, DebridError> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| DebridError::Malformed)?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}
