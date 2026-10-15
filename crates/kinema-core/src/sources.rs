//! Provider-independent source identities and conservative remote-file selection.

use std::{error::Error, fmt};

use crate::{MediaKind, catalog::ImdbId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IndexerKind {
    Torrentio,
    Peerflix,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DebridKind {
    RealDebrid,
    AllDebrid,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PlaybackTarget {
    Movie(ImdbId),
    Episode {
        series: ImdbId,
        season: u32,
        episode: u32,
    },
}

impl PlaybackTarget {
    pub fn imdb_id(&self) -> &ImdbId {
        match self {
            Self::Movie(id) | Self::Episode { series: id, .. } => id,
        }
    }

    pub fn kind(&self) -> MediaKind {
        match self {
            Self::Movie(_) => MediaKind::Movie,
            Self::Episode { .. } => MediaKind::Series,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceValidationError {
    InvalidInfoHash,
}

impl fmt::Display for SourceValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Torrent hashes must contain exactly 40 ASCII hexadecimal characters.")
    }
}

impl Error for SourceValidationError {}

/// A canonical lowercase BitTorrent v1 info hash, not a magnet or URL.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct InfoHash(String);

impl InfoHash {
    pub fn new(value: impl Into<String>) -> Result<Self, SourceValidationError> {
        let value = value.into();
        if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SourceValidationError::InvalidInfoHash);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TorrentSource {
    pub hash: InfoHash,
    pub file_index: Option<u32>,
    pub filename: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCandidate {
    pub torrent: TorrentSource,
    pub name: String,
    pub quality: Option<String>,
    pub size: Option<u64>,
    pub seeders: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteFile {
    pub id: String,
    pub path: String,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaIdentity {
    pub torrent: InfoHash,
    pub file_path: String,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileSelection {
    Unique(usize),
    Ambiguous(Vec<usize>),
    NotFound,
}

/// Returns original input indices, never ranks by size, ID, or input order.
/// Paths are provider paths, not host filesystem paths. Both slash styles work.
/// Movies admit any non-extra video; episodes require an exact, standalone
/// SxxExx or season x episode marker in the basename. Multiple markers/ranges
/// are conservatively rejected rather than guessing a position within a pack.
pub fn select_file(target: &PlaybackTarget, files: &[RemoteFile]) -> FileSelection {
    let matches: Vec<_> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| eligible_path(&file.path, target))
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [] => FileSelection::NotFound,
        [index] => FileSelection::Unique(*index),
        _ => FileSelection::Ambiguous(matches),
    }
}

fn eligible_path(path: &str, target: &PlaybackTarget) -> bool {
    let lower = path.to_lowercase();
    if lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| matches!(token, "sample" | "samples" | "trailer" | "trailers"))
    {
        return false;
    }
    let basename = lower.rsplit(['/', '\\']).next().unwrap_or("");
    let Some((stem, extension)) = basename.rsplit_once('.') else {
        return false;
    };
    if !matches!(
        extension,
        "mkv"
            | "mp4"
            | "avi"
            | "m4v"
            | "mov"
            | "webm"
            | "ts"
            | "m2ts"
            | "mpg"
            | "mpeg"
            | "wmv"
            | "flv"
            | "ogv"
    ) {
        return false;
    }
    match target {
        PlaybackTarget::Movie(_) => true,
        PlaybackTarget::Episode {
            season, episode, ..
        } => episode_marker(stem) == Some((*season, *episode)),
    }
}

// Tokenization is intentionally strict: concatenated/multi-episode markers such
// as S01E01E02, S01E01-02, or 1x01-02 must not silently select a combined file.
fn episode_marker(stem: &str) -> Option<(u32, u32)> {
    let tokens: Vec<_> = stem
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let mut found = None;
    for (index, token) in tokens.iter().enumerate() {
        let pair = if let Some(rest) = token.strip_prefix('s') {
            rest.split_once('e')
        } else {
            token.split_once('x')
        };
        let Some((season, episode)) = pair else {
            continue;
        };
        if season.is_empty()
            || episode.is_empty()
            || !season.bytes().all(|b| b.is_ascii_digit())
            || !episode.bytes().all(|b| b.is_ascii_digit())
        {
            // A malformed episode-looking token cannot be ignored in favor of
            // another valid marker later in the name.
            if !season.is_empty() && season.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            continue;
        }
        let pair = (season.parse().ok()?, episode.parse().ok()?);
        if found.is_some() {
            return None;
        }
        // A following bare number or E-number is potentially a multi-episode
        // range; prefer an explicit picker over guessing from release metadata.
        if let Some(next) = tokens.get(index + 1) {
            let number = next.strip_prefix('e').unwrap_or(next);
            if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
        found = Some(pair);
    }
    found
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn movie() -> PlaybackTarget {
        PlaybackTarget::Movie(ImdbId::new("tt1234567").expect("valid ID"))
    }
    fn episode(season: u32, episode: u32) -> PlaybackTarget {
        PlaybackTarget::Episode {
            series: movie().imdb_id().clone(),
            season,
            episode,
        }
    }
    fn files(paths: &[&str]) -> Vec<RemoteFile> {
        paths
            .iter()
            .enumerate()
            .map(|(i, path)| RemoteFile {
                id: i.to_string(),
                path: (*path).into(),
                size: (i as u64 + 1) * 100,
            })
            .collect()
    }

    #[test]
    fn hashes_validate_exactly_and_canonicalize() {
        let lower = "0123456789abcdef0123456789abcdef01234567";
        let hash = InfoHash::new(lower.to_uppercase()).expect("valid hash");
        assert_eq!(hash.as_str(), lower);
        assert_eq!(
            HashSet::from([hash.clone(), InfoHash::new(lower).expect("valid")]).len(),
            1
        );
        for bad in [
            "".into(),
            "a".repeat(39),
            "a".repeat(41),
            "g".repeat(40),
            "é".repeat(20),
            format!(" {lower}"),
            format!("{lower}\n"),
        ] {
            assert_eq!(
                InfoHash::new(bad),
                Err(SourceValidationError::InvalidInfoHash)
            );
        }
    }

    #[test]
    fn identities_distinguish_kind_season_episode_and_specials() {
        assert_eq!(movie().kind(), MediaKind::Movie);
        assert_eq!(episode(0, 1).kind(), MediaKind::Series);
        assert_eq!(movie().imdb_id(), episode(1, 1).imdb_id());
        assert_eq!(
            HashSet::from([movie(), episode(0, 1), episode(1, 1), episode(1, 2)]).len(),
            4
        );
    }

    #[test]
    fn movies_exclude_nonvideo_extras_and_never_guess_largest() {
        let pack = files(&[
            "readme.txt",
            "Samples/movie.MKV",
            "movie-trailer.mp4",
            "Movie.MKV",
        ]);
        assert_eq!(select_file(&movie(), &pack), FileSelection::Unique(3));
        assert_eq!(
            select_file(&movie(), &files(&["a.mkv", "b.mp4"])),
            FileSelection::Ambiguous(vec![0, 1])
        );
        assert_eq!(
            select_file(
                &movie(),
                &files(&[
                    "a.mp4.exe",
                    "sample.mkv",
                    "folder.mkv/noextension",
                    "trailer\\a.mkv"
                ])
            ),
            FileSelection::NotFound
        );
        assert_eq!(select_file(&movie(), &[]), FileSelection::NotFound);
    }

    #[test]
    fn episode_matching_is_exact_and_supports_specials_and_both_notations() {
        let pack = files(&[
            "Show.S01E02.mkv",
            "Show.S01E020.mkv",
            "Show.1x02.mp4",
            "Show.S02E02.mkv",
            "Show.S00E02.mkv",
        ]);
        assert_eq!(
            select_file(&episode(1, 2), &pack),
            FileSelection::Ambiguous(vec![0, 2])
        );
        assert_eq!(select_file(&episode(0, 2), &pack), FileSelection::Unique(4));
        assert_eq!(
            select_file(&episode(1, 20), &pack),
            FileSelection::Unique(1)
        );
        assert_eq!(select_file(&episode(1, 3), &pack), FileSelection::NotFound);
        assert_eq!(
            select_file(&episode(1, 2), &files(&["Show.s1e2.1080p.MKV"])),
            FileSelection::Unique(0)
        );
    }

    #[test]
    fn episode_matching_rejects_guesses_ranges_and_partial_tokens() {
        for name in [
            "02.mkv",
            "Season 1/02.mkv",
            "ShowS01E02.mkv",
            "Show.S01E02E03.mkv",
            "Show.S01E02-03.mkv",
            "Show.1x02-03.mkv",
            "Show.S01E02-E03.mkv",
            "Show.S01E02.S01E03.mkv",
            "Show.S01E02sample.mkv",
            "Show.S4294967296E02.mkv",
            "Show.S01E4294967296.mkv",
            "Show.S01E02.trailer.mkv",
        ] {
            assert_eq!(
                select_file(&episode(1, 2), &files(&[name])),
                FileSelection::NotFound,
                "{name}"
            );
        }
        assert_eq!(
            select_file(&episode(1, 2), &files(&["Show/Show.1x02.720p.x264.mkv"])),
            FileSelection::Unique(0)
        );
    }
}
