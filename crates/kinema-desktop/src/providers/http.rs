//! Bounded blocking HTTP. Call only from an owned background worker.
use super::DebridError;
use reqwest::{Url, blocking::Client};
use serde_json::Value;
use std::{
    io::Read,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
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
        .map_err(|_| DebridError::Transport)
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
pub(crate) fn json(
    request: reqwest::blocking::RequestBuilder,
    cancel: &AtomicBool,
) -> Result<Value, DebridError> {
    check(cancel)?;
    let response = request.send().map_err(|_| DebridError::Transport)?;
    check(cancel)?;
    match response.status().as_u16() {
        401 | 403 => return Err(DebridError::CredentialRejected),
        429 => return Err(DebridError::RateLimited),
        200..=299 => {}
        _ => return Err(DebridError::Service),
    }
    if response.content_length().is_some_and(|n| n > MAX_BODY) {
        return Err(DebridError::TooLarge);
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_BODY + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| DebridError::Transport)?;
    check(cancel)?;
    if bytes.len() as u64 > MAX_BODY {
        return Err(DebridError::TooLarge);
    }
    serde_json::from_slice(&bytes).map_err(|_| DebridError::Malformed)
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
