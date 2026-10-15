//! AllDebrid v4 upload/files/unlock and v4.1 status; no deprecated instant lookup.
use super::{DebridClient, DebridError, PreparedTorrent, ResolvedMedia, http, resolved};
use kinema_core::sources::{RemoteFile, TorrentSource};
use serde_json::Value;
use std::sync::atomic::AtomicBool;

fn call(
    client: &DebridClient,
    endpoint: &[&str],
    fields: &[(&str, &str)],
    cancel: &AtomicBool,
) -> Result<Value, DebridError> {
    let value = http::json(client.form(endpoint, fields)?, cancel)?;
    envelope(value)
}
fn envelope(value: Value) -> Result<Value, DebridError> {
    match value.get("status").and_then(Value::as_str) {
        Some("success") => value.get("data").cloned().ok_or(DebridError::Malformed),
        Some("error") => {
            let code = value
                .pointer("/error/code")
                .and_then(Value::as_str)
                .ok_or(DebridError::Malformed)?;
            if code.starts_with("AUTH_") {
                Err(DebridError::CredentialRejected)
            } else {
                Err(DebridError::Failed)
            }
        }
        _ => Err(DebridError::Malformed),
    }
}
pub(super) fn check_account(client: &DebridClient, cancel: &AtomicBool) -> Result<(), DebridError> {
    let data = envelope(http::json(
        client.request(reqwest::Method::GET, &["v4", "user"])?,
        cancel,
    )?)?;
    let account = data.get("user").ok_or(DebridError::Malformed)?;
    http::text(account, "username")?;
    if account.get("isPremium").and_then(Value::as_bool).is_none() {
        return Err(DebridError::Malformed);
    }
    http::check(cancel)
}

fn magnet<'a>(data: &'a Value, job: &str) -> Result<&'a Value, DebridError> {
    let value = data.get("magnets").ok_or(DebridError::Malformed)?;
    // Some API revisions return a single object when an id is supplied.
    if value.is_object()
        && value
            .get("id")
            .is_some_and(|v| http::id(v).is_ok_and(|id| id == job))
    {
        return Ok(value);
    }
    let rows = value.as_array().ok_or(DebridError::Malformed)?;
    if rows.len() > http::MAX_FILES {
        return Err(DebridError::TooLarge);
    }
    rows.iter()
        .find(|r| {
            r.get("id")
                .is_some_and(|v| http::id(v).is_ok_and(|id| id == job))
        })
        .ok_or(DebridError::Malformed)
}
fn wait(client: &DebridClient, job: &str, cancel: &AtomicBool) -> Result<(), DebridError> {
    for attempt in 0..client.polls {
        let data = call(
            client,
            &["v4.1", "magnet", "status"],
            &[("id", job)],
            cancel,
        )?;
        let row = magnet(&data, job)?;
        match row
            .get("statusCode")
            .and_then(Value::as_u64)
            .ok_or(DebridError::Malformed)?
        {
            4 => return Ok(()),
            0..=3 => {}
            5..=15 => return Err(DebridError::Failed),
            _ => return Err(DebridError::Malformed),
        }
        if attempt + 1 < client.polls {
            http::pause(cancel)?;
        }
    }
    Err(DebridError::PendingTimeout)
}
// Download URLs stay only in this private, transient response representation.
fn tree(
    rows: &[Value],
    prefix: &str,
    depth: usize,
    out: &mut Vec<(RemoteFile, String)>,
) -> Result<(), DebridError> {
    if depth > 32 {
        return Err(DebridError::TooLarge);
    }
    for row in rows {
        let name = http::text(row, "n")?;
        if name.len().saturating_add(prefix.len()).saturating_add(1) > http::MAX_PATH {
            return Err(DebridError::TooLarge);
        }
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        if let Some(children) = row.get("e") {
            tree(
                children.as_array().ok_or(DebridError::Malformed)?,
                &path,
                depth + 1,
                out,
            )?;
        } else {
            if out.len() >= http::MAX_FILES {
                return Err(DebridError::TooLarge);
            }
            let file = RemoteFile {
                id: out.len().to_string(),
                path,
                size: row
                    .get("s")
                    .and_then(Value::as_u64)
                    .ok_or(DebridError::UnknownSize)?,
            };
            if out.iter().any(|(f, _)| f.path == file.path) {
                return Err(DebridError::Malformed);
            }
            out.push((file, http::text(row, "l")?.to_owned()));
        }
    }
    Ok(())
}
fn files(
    client: &DebridClient,
    job: &str,
    cancel: &AtomicBool,
) -> Result<Vec<(RemoteFile, String)>, DebridError> {
    let data = call(client, &["v4", "magnet", "files"], &[("id[]", job)], cancel)?;
    let row = magnet(&data, job)?;
    let rows = row
        .get("files")
        .and_then(Value::as_array)
        .ok_or(DebridError::Malformed)?;
    let mut out = Vec::new();
    tree(rows, "", 0, &mut out)?;
    if out.is_empty() {
        return Err(DebridError::Malformed);
    }
    Ok(out)
}
pub(super) fn prepare(
    client: &DebridClient,
    source: &TorrentSource,
    cancel: &AtomicBool,
) -> Result<PreparedTorrent, DebridError> {
    let magnet_url = format!("magnet:?xt=urn:btih:{}", source.hash.as_str());
    let data = call(
        client,
        &["v4", "magnet", "upload"],
        &[("magnets[]", &magnet_url)],
        cancel,
    )?;
    let rows = data
        .get("magnets")
        .and_then(Value::as_array)
        .ok_or(DebridError::Malformed)?;
    if rows.len() != 1 {
        return Err(DebridError::Malformed);
    }
    if rows[0].get("error").is_some() {
        return Err(DebridError::Failed);
    }
    let job = http::id(rows[0].get("id").ok_or(DebridError::Malformed)?)?;
    wait(client, &job, cancel)?;
    let files = files(client, &job, cancel)?
        .into_iter()
        .map(|(file, _)| file)
        .collect();
    Ok(PreparedTorrent {
        provider: client.kind,
        source: source.clone(),
        job_id: job,
        files,
    })
}
pub(super) fn resolve(
    client: &DebridClient,
    prepared: &PreparedTorrent,
    file: &RemoteFile,
    cancel: &AtomicBool,
) -> Result<ResolvedMedia, DebridError> {
    let rows = files(client, &prepared.job_id, cancel)?;
    let (_, link) = rows
        .iter()
        .find(|(f, _)| f.path == file.path && f.size == file.size)
        .ok_or(DebridError::FileNotFound)?;
    let data = call(client, &["v4", "link", "unlock"], &[("link", link)], cancel)?;
    if data
        .get("filesize")
        .and_then(Value::as_u64)
        .is_some_and(|size| size != file.size)
    {
        return Err(DebridError::Malformed);
    }
    if let Some(link) = data.get("link").and_then(Value::as_str) {
        return resolved(prepared, file, link);
    }
    let delayed = http::id(data.get("delayed").ok_or(DebridError::Malformed)?)?;
    for _ in 0..client.polls {
        // The official delayed-link API requires >=5 seconds between polls.
        for _ in 0..5 {
            http::pause(cancel)?;
        }
        let data = call(
            client,
            &["v4", "link", "delayed"],
            &[("id", &delayed)],
            cancel,
        )?;
        match data.get("status").and_then(Value::as_u64) {
            Some(2) => return resolved(prepared, file, http::text(&data, "link")?),
            Some(1) => {}
            Some(3) => return Err(DebridError::Failed),
            _ => return Err(DebridError::Malformed),
        }
    }
    Err(DebridError::PendingTimeout)
}
