//! Public, noncredentialed Stremio indexers. Invoke only on a background worker.
use crate::providers::http;
use kinema_core::{
    MediaKind,
    sources::{IndexerKind, InfoHash, PlaybackTarget, SourceCandidate, TorrentSource},
};
use reqwest::{Url, blocking::Client};
use serde_json::Value;
use std::{collections::HashSet, fmt, sync::atomic::AtomicBool};

const MAX_RESULTS: usize = 1000;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceError {
    Transport,
    Service,
    RateLimited,
    Malformed,
    TooLarge,
    Canceled,
}
impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Transport => "Indexer connection failed.",
            Self::Service => "Indexer service failed.",
            Self::RateLimited => "Indexer rate limit reached.",
            Self::Malformed => "Invalid indexer response.",
            Self::TooLarge => "Indexer response exceeds limits.",
            Self::Canceled => "Operation canceled.",
        })
    }
}
impl std::error::Error for SourceError {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceResults {
    pub candidates: Vec<SourceCandidate>,
    pub omitted: usize,
}
pub struct IndexerClient {
    client: Client,
    torrentio: Url,
    peerflix: Url,
}
impl IndexerClient {
    pub fn new() -> Result<Self, SourceError> {
        Ok(Self {
            client: http::client().map_err(|_| SourceError::Transport)?,
            torrentio: Url::parse("https://torrentio.strem.fun/")
                .map_err(|_| SourceError::Malformed)?,
            peerflix: Url::parse("https://peerflix.mov/").map_err(|_| SourceError::Malformed)?,
        })
    }
    pub fn fetch(
        &self,
        indexer: IndexerKind,
        target: &PlaybackTarget,
        cancel: &AtomicBool,
    ) -> Result<SourceResults, SourceError> {
        check_cancel(cancel)?;
        let mut url = match indexer {
            IndexerKind::Torrentio => self.torrentio.clone(),
            IndexerKind::Peerflix => self.peerflix.clone(),
        };
        let kind = match target.kind() {
            MediaKind::Movie => "movie",
            MediaKind::Series => "series",
        };
        let id = match target {
            PlaybackTarget::Movie(id) => id.as_str().to_owned(),
            PlaybackTarget::Episode {
                series,
                season,
                episode,
            } => format!("{}:{season}:{episode}", series.as_str()),
        };
        url.path_segments_mut()
            .map_err(|_| SourceError::Malformed)?
            .pop_if_empty()
            .extend(["stream", kind, &format!("{id}.json")]);
        let response = http::json(self.client.get(url), cancel);
        check_cancel(cancel)?;
        let value = response.map_err(|error| match error {
            crate::providers::DebridError::Transport => SourceError::Transport,
            crate::providers::DebridError::Malformed => SourceError::Malformed,
            crate::providers::DebridError::TooLarge => SourceError::TooLarge,
            crate::providers::DebridError::RateLimited => SourceError::RateLimited,
            crate::providers::DebridError::Canceled => SourceError::Canceled,
            _ => SourceError::Service,
        })?;
        check_cancel(cancel)?;
        let result = parse(&value);
        check_cancel(cancel)?;
        result
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), SourceError> {
    http::check(cancel).map_err(|_| SourceError::Canceled)
}
fn magnet_hash(value: &str) -> Result<InfoHash, SourceError> {
    let url = Url::parse(value).map_err(|_| SourceError::Malformed)?;
    if url.scheme() != "magnet" {
        return Err(SourceError::Malformed);
    }
    let mut found = None;
    for (key, value) in url.query_pairs() {
        if key == "xt" && value.starts_with("urn:btih:") {
            let hash = InfoHash::new(value.trim_start_matches("urn:btih:"))
                .map_err(|_| SourceError::Malformed)?;
            if found.as_ref().is_some_and(|old| old != &hash) {
                return Err(SourceError::Malformed);
            }
            found = Some(hash);
        }
    }
    found.ok_or(SourceError::Malformed)
}
fn label(text: &str) -> String {
    text.split_whitespace()
        .take(80)
        .map(|word| {
            if word.contains("://") || word.starts_with("magnet:") {
                "[redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(512)
        .collect()
}
fn optional_u64(row: &Value, hints: &Value, keys: &[&str]) -> Result<Option<u64>, SourceError> {
    for key in keys {
        if let Some(value) = row.get(key).or_else(|| hints.get(key)) {
            if value.is_null() {
                continue;
            }
            return value.as_u64().map(Some).ok_or(SourceError::Malformed);
        }
    }
    Ok(None)
}
// These fallback estimates are unverified UI hints, never provider file metadata.
// Inspect only 2,048 characters; duplicate markers or noncanonical numeric fields are ignored.
fn label_hints(text: &str) -> (Option<u64>, Option<u32>) {
    let bounded: String = text.chars().take(2048).collect();
    let field = |marker| {
        bounded
            .split_once(marker)
            .map(|(_, tail)| tail)
            .filter(|tail| !tail.contains(marker))
    };
    let seeders = field('👤')
        .and_then(|tail| tail.split_whitespace().next())
        .filter(|word| word.len() <= 10 && word.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|word| word.parse::<u32>().ok());
    let size = field('💾').and_then(|tail| {
        let mut words = tail.split_whitespace();
        let number = words.next()?;
        let unit = words.next()?;
        let multiplier: u64 = match unit {
            "B" => 1,
            "KB" => 1_000,
            "MB" => 1_000_000,
            "GB" => 1_000_000_000,
            "TB" => 1_000_000_000_000,
            "KiB" => 1 << 10,
            "MiB" => 1 << 20,
            "GiB" => 1 << 30,
            "TiB" => 1 << 40,
            _ => return None,
        };
        if number.len() > 20 {
            return None;
        }
        let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > 3
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || (number.contains('.') && fraction.is_empty())
        {
            return None;
        }
        let whole = whole.parse::<u64>().ok()?.checked_mul(multiplier)?;
        let fraction = if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u64>().ok()?.checked_mul(multiplier)?
                / 10_u64.pow(fraction.len() as u32)
        };
        whole.checked_add(fraction).filter(|size| *size > 0)
    });
    (size, seeders)
}
fn parse(value: &Value) -> Result<SourceResults, SourceError> {
    let rows = value
        .get("streams")
        .and_then(Value::as_array)
        .ok_or(SourceError::Malformed)?;
    if rows.len() > MAX_RESULTS {
        return Err(SourceError::TooLarge);
    }
    let mut result = SourceResults {
        candidates: Vec::new(),
        omitted: 0,
    };
    let mut seen = HashSet::new();
    for row in rows {
        if !row.is_object() {
            return Err(SourceError::Malformed);
        }
        let hash = if let Some(hash) = row.get("infoHash").filter(|v| !v.is_null()) {
            InfoHash::new(hash.as_str().ok_or(SourceError::Malformed)?)
                .map_err(|_| SourceError::Malformed)?
        } else if let Some(url) = row
            .get("url")
            .or_else(|| row.get("externalUrl"))
            .and_then(Value::as_str)
        {
            if url.starts_with("magnet:") {
                magnet_hash(url)?
            } else {
                result.omitted += 1;
                continue;
            }
        } else {
            return Err(SourceError::Malformed);
        };
        let hints = row.get("behaviorHints").unwrap_or(&Value::Null);
        let file_index = optional_u64(row, hints, &["fileIdx"])?
            .map(|n| u32::try_from(n).map_err(|_| SourceError::Malformed))
            .transpose()?;
        if !seen.insert((hash.clone(), file_index)) {
            result.omitted += 1;
            continue;
        }
        let release = ["description", "title", "name"]
            .into_iter()
            .find_map(|key| {
                row.get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or("Torrent");
        let name = label(release);
        let (label_size, label_seeders) = label_hints(release);
        let addon = label(row.get("name").and_then(Value::as_str).unwrap_or_default());
        let structured_quality = label(
            row.get("quality")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        let quality_text = format!("{structured_quality} {addon} {name}").to_ascii_lowercase();
        let is_4k = quality_text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| word == "4k");
        let quality = if is_4k {
            Some("2160p".to_owned())
        } else {
            ["2160p", "1080p", "720p", "480p"]
                .into_iter()
                .find(|q| quality_text.contains(q))
                .map(str::to_owned)
        };
        let filename = hints
            .get("filename")
            .or_else(|| row.get("filename"))
            .and_then(Value::as_str)
            .map(label);
        let size = optional_u64(
            row,
            hints,
            &["sizebytes", "size", "videoSize", "torrentSize"],
        )?;
        let size = size.or(label_size);
        let seeders = optional_u64(row, hints, &["seeders", "seed"])?
            .map(|n| u32::try_from(n).map_err(|_| SourceError::Malformed))
            .transpose()?;
        result.candidates.push(SourceCandidate {
            torrent: TorrentSource {
                hash,
                file_index,
                filename,
            },
            name,
            quality,
            size,
            seeders: seeders.or(label_seeders),
        });
    }
    Ok(result)
}

#[cfg(test)]
#[path = "providers/source_tests.rs"]
mod tests;
