//! Real-Debrid REST 1.0: addMagnet → info → selectFiles → info → unrestrict.
use super::{DebridClient, DebridError, PreparedTorrent, ResolvedMedia, http, resolved};
use kinema_core::sources::{RemoteFile, TorrentSource};
use reqwest::Method;
use serde_json::Value;
use std::sync::atomic::AtomicBool;

pub(super) fn check_account(client: &DebridClient, cancel: &AtomicBool) -> Result<(), DebridError> {
    let account = http::json(client.request(Method::GET, &["user"])?, cancel)?;
    if account
        .get("id")
        .and_then(Value::as_u64)
        .is_none_or(|id| id == 0)
    {
        return Err(DebridError::Malformed);
    }
    http::text(&account, "username")?;
    http::check(cancel)
}

fn info(client: &DebridClient, job: &str, cancel: &AtomicBool) -> Result<Value, DebridError> {
    http::json(
        client.request(Method::GET, &["torrents", "info", job])?,
        cancel,
    )
}
fn files(value: &Value) -> Result<Vec<RemoteFile>, DebridError> {
    let rows = value
        .get("files")
        .and_then(Value::as_array)
        .ok_or(DebridError::Malformed)?;
    if rows.len() > http::MAX_FILES {
        return Err(DebridError::TooLarge);
    }
    let mut out = Vec::new();
    for row in rows {
        let file = RemoteFile {
            id: http::id(row.get("id").ok_or(DebridError::Malformed)?)?,
            path: http::text(row, "path")?.to_owned(),
            size: row
                .get("bytes")
                .and_then(Value::as_u64)
                .ok_or(DebridError::UnknownSize)?,
        };
        // Only numeric provider IDs are valid; never let a malformed ID become "all".
        if file.id.parse::<u64>().ok().is_none_or(|id| id == 0) {
            return Err(DebridError::Malformed);
        }
        if file.path.len() > http::MAX_PATH {
            return Err(DebridError::TooLarge);
        }
        if out.iter().any(|f: &RemoteFile| f.id == file.id) {
            return Err(DebridError::Malformed);
        }
        out.push(file);
    }
    if out.is_empty() {
        return Err(DebridError::Malformed);
    }
    Ok(out)
}
fn status(value: &Value) -> Result<&str, DebridError> {
    let status = http::text(value, "status")?;
    match status {
        "magnet_error" | "error" | "virus" | "dead" => Err(DebridError::Failed),
        "magnet_conversion"
        | "waiting_files_selection"
        | "queued"
        | "downloading"
        | "downloaded"
        | "compressing"
        | "uploading" => Ok(status),
        _ => Err(DebridError::Malformed),
    }
}
pub(super) fn prepare(
    client: &DebridClient,
    source: &TorrentSource,
    cancel: &AtomicBool,
) -> Result<PreparedTorrent, DebridError> {
    let magnet = format!("magnet:?xt=urn:btih:{}", source.hash.as_str());
    let added = http::json(
        client.form(&["torrents", "addMagnet"], &[("magnet", &magnet)])?,
        cancel,
    )?;
    let job = http::id(added.get("id").ok_or(DebridError::Malformed)?)?;
    for attempt in 0..client.polls {
        let value = info(client, &job, cancel)?;
        let state = status(&value)?;
        if state != "magnet_conversion" {
            let files = files(&value)?;
            return Ok(PreparedTorrent {
                provider: client.kind,
                source: source.clone(),
                job_id: job,
                files,
            });
        }
        if attempt + 1 < client.polls {
            http::pause(cancel)?;
        }
    }
    Err(DebridError::PendingTimeout)
}
fn select(
    request: reqwest::blocking::RequestBuilder,
    cancel: &AtomicBool,
) -> Result<(), DebridError> {
    http::send(request, cancel).map(|_| ())
}
fn selected_link<'a>(value: &'a Value, file: &RemoteFile) -> Result<&'a str, DebridError> {
    let rows = value
        .get("files")
        .and_then(Value::as_array)
        .ok_or(DebridError::Malformed)?;
    let links = value
        .get("links")
        .and_then(Value::as_array)
        .ok_or(DebridError::Malformed)?;
    if rows.len() > http::MAX_FILES {
        return Err(DebridError::TooLarge);
    }
    let selected: Vec<_> = rows
        .iter()
        .filter(|r| r.get("selected").and_then(Value::as_u64) == Some(1))
        .collect();
    if selected.len() != links.len() {
        return Err(DebridError::Malformed);
    }
    for (row, link) in selected.into_iter().zip(links) {
        if http::id(row.get("id").ok_or(DebridError::Malformed)?)? == file.id {
            if http::text(row, "path")? != file.path
                || row.get("bytes").and_then(Value::as_u64) != Some(file.size)
            {
                return Err(DebridError::FileNotFound);
            }
            return link
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or(DebridError::Malformed);
        }
    }
    Err(DebridError::FileNotFound)
}
pub(super) fn resolve(
    client: &DebridClient,
    prepared: &PreparedTorrent,
    file: &RemoteFile,
    cancel: &AtomicBool,
) -> Result<ResolvedMedia, DebridError> {
    let mut selection_sent = false;
    for attempt in 0..client.polls {
        let value = info(client, &prepared.job_id, cancel)?;
        let state = status(&value)?;
        if state == "waiting_files_selection" && !selection_sent {
            if !files(&value)?.iter().any(|current| current == file) {
                return Err(DebridError::FileNotFound);
            }
            // Acquisition starts only after the caller has chosen this exact provider file.
            // This endpoint returns HTTP 204 with no JSON body.
            select(
                client.form(
                    &["torrents", "selectFiles", &prepared.job_id],
                    &[("files", &file.id)],
                )?,
                cancel,
            )?;
            selection_sent = true;
        }
        if state == "downloaded" {
            let link = selected_link(&value, file)?;
            let unlocked = http::json(
                client.form(&["unrestrict", "link"], &[("link", link)])?,
                cancel,
            )?;
            if unlocked
                .get("filesize")
                .and_then(Value::as_u64)
                .is_some_and(|size| size != file.size)
            {
                return Err(DebridError::Malformed);
            }
            return resolved(prepared, file, http::text(&unlocked, "download")?);
        }
        if attempt + 1 < client.polls {
            http::pause(cancel)?;
        }
    }
    Err(DebridError::PendingTimeout)
}
