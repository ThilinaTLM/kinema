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

/// Indexer-provided identity, never a ranking by file size or list position.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileHint {
    pub file_index: Option<u32>,
    pub filename: Option<String>,
}

impl From<&TorrentSource> for FileHint {
    fn from(source: &TorrentSource) -> Self {
        Self {
            file_index: source.file_index,
            filename: source.filename.clone(),
        }
    }
}

impl FileSelection {
    /// Non-unique preparations must show this error, never a file picker.
    pub fn error_message(&self) -> Option<&'static str> {
        match self {
            Self::Unique(_) => None,
            Self::Ambiguous(_) | Self::NotFound => {
                Some("Couldn't identify the file in this torrent — try another stream.")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileSelectionMethod {
    Filename,
    FileIndex,
    Heuristic,
    None,
}

impl FileSelectionMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filename => "filename",
            Self::FileIndex => "file_index",
            Self::Heuristic => "heuristic",
            Self::None => "none",
        }
    }
}

/// Select by exact basename, then reliable torrent index, then strict policy.
/// Torrentio/Peerflix indices are zero-based torrent indices. Real-Debrid keeps
/// one-based torrent indices in file IDs (not array positions). AllDebrid IDs in
/// our adapter are synthetic depth-first tree positions: never use them here.
/// Duplicate basenames need an agreeing reliable index, otherwise are ambiguous.
/// Size is deliberately not a tiebreaker: candidate sizes can describe a pack.
pub fn select_file_with_hint(
    target: &PlaybackTarget,
    files: &[RemoteFile],
    provider: DebridKind,
    hint: Option<&FileHint>,
) -> (FileSelection, FileSelectionMethod) {
    let indexed = hint.and_then(|hint| {
        let id = u64::from(hint.file_index?) + 1;
        if provider != DebridKind::RealDebrid {
            return None;
        }
        // An index alone carries no file name to cross-check, and indexers have
        // repeated one index for different episodes. Trust it only when the
        // file's own full marker names the target episode.
        let index_only = hint.filename.as_deref().is_none_or(|n| n.trim().is_empty());
        let matches: Vec<_> = files
            .iter()
            .enumerate()
            .filter(|(_, file)| {
                file.id.parse::<u64>().ok() == Some(id)
                    && if index_only {
                        eligible_path(&file.path, target)
                    } else {
                        eligible_hint_path(&file.path, target)
                    }
            })
            .map(|(i, _)| i)
            .collect();
        match matches.as_slice() {
            [index] => Some(*index),
            _ => None,
        }
    });
    if let Some(filename) = hint.and_then(|hint| hint.filename.as_deref()) {
        let basename = basename(filename).to_lowercase();
        let matches: Vec<_> = files
            .iter()
            .enumerate()
            .filter(|(_, file)| {
                basename == self::basename(&file.path).to_lowercase()
                    && eligible_hint_path(&file.path, target)
            })
            .map(|(i, _)| i)
            .collect();
        match matches.as_slice() {
            [] => {}
            [index] => return (FileSelection::Unique(*index), FileSelectionMethod::Filename),
            _ => {
                let selection = match indexed.filter(|i| matches.contains(i)) {
                    Some(index) => FileSelection::Unique(index),
                    None => FileSelection::Ambiguous(matches),
                };
                return (selection, FileSelectionMethod::Filename);
            }
        }
    }
    if let Some(index) = indexed {
        return (FileSelection::Unique(index), FileSelectionMethod::FileIndex);
    }
    let selection = select_file(target, files);
    let method = if matches!(selection, FileSelection::Unique(_)) {
        FileSelectionMethod::Heuristic
    } else {
        FileSelectionMethod::None
    };
    (selection, method)
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or("")
}

// Explicit identity permits unnumbered episode names, but not contradictory,
// malformed, or multi-episode markers. Strict fallback still requires a marker.
fn eligible_hint_path(path: &str, target: &PlaybackTarget) -> bool {
    let lower = path.to_lowercase();
    let Some(stem) = eligible_video_stem(&lower) else {
        return false;
    };
    let PlaybackTarget::Episode {
        season, episode, ..
    } = target
    else {
        return true;
    };
    if let Some(pair) = episode_marker(stem) {
        return pair == (*season, *episode);
    }
    // Detect even malformed SxxExx and NxNN tokens, rather than mistaking a
    // rejected range or marker for an unnumbered filename.
    !stem
        .split(|c: char| !c.is_alphanumeric())
        .any(looks_like_marker)
}

// A token that starts with `s<digit>`, embeds a glued full `s<n>e<n>` (as in
// `shows01e03`), or is `<1-2 digits>x...`. Resolutions (`1920x1080`) and a bare
// `s<digit>` inside a word (`fs88`) do not count.
fn looks_like_marker(token: &str) -> bool {
    if token
        .strip_prefix('s')
        .is_some_and(|rest| rest.bytes().next().is_some_and(|b| b.is_ascii_digit()))
    {
        return true;
    }
    let bytes = token.as_bytes();
    if (0..bytes.len()).any(|i| {
        bytes[i] == b's' && {
            let digits = bytes[i + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            digits > 0
                && bytes.get(i + 1 + digits) == Some(&b'e')
                && bytes.get(i + 2 + digits).is_some_and(u8::is_ascii_digit)
        }
    }) {
        return true;
    }
    token
        .split_once('x')
        .is_some_and(|(season, _)| is_number(season) && season.len() <= 2)
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
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
    let Some(stem) = eligible_video_stem(&lower) else {
        return false;
    };
    match target {
        PlaybackTarget::Movie(_) => true,
        PlaybackTarget::Episode {
            season, episode, ..
        } => episode_marker(stem) == Some((*season, *episode)),
    }
}

// Receives a lowercase provider path and returns an eligible video basename stem.
fn eligible_video_stem(lower: &str) -> Option<&str> {
    if lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| matches!(token, "sample" | "samples" | "trailer" | "trailers"))
    {
        return None;
    }
    let basename = lower.rsplit(['/', '\\']).next().unwrap_or("");
    let (stem, extension) = basename.rsplit_once('.')?;
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
        return None;
    }
    Some(stem)
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
        // `WxH` resolutions are not markers: season sides of 3+ digits and
        // episode sides of 4+ digits.
        if is_number(season) && (season.len() >= 3 || (is_number(episode) && episode.len() >= 4)) {
            continue;
        }
        // A revision suffix (`S01E05v2`) does not change the episode.
        let episode = match episode.split_once('v') {
            Some((number, revision)) if is_number(number) && is_number(revision) => number,
            _ => episode,
        };
        if !is_number(season) || !is_number(episode) {
            // A malformed episode-looking token cannot be ignored in favor of
            // another valid marker later in the name.
            if is_number(season) {
                return None;
            }
            continue;
        }
        let pair = (season.parse().ok()?, episode.parse().ok()?);
        if found.is_some() {
            return None;
        }
        // A following bare number or E-number is potentially a multi-episode
        // range; reject the file rather than guessing from release metadata.
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

    fn hinted(
        target: &PlaybackTarget,
        files: &[RemoteFile],
        provider: DebridKind,
        index: Option<u32>,
        name: Option<&str>,
    ) -> (FileSelection, FileSelectionMethod) {
        select_file_with_hint(
            target,
            files,
            provider,
            Some(&FileHint {
                file_index: index,
                filename: name.map(str::to_owned),
            }),
        )
    }

    #[test]
    fn filename_hint_wins_over_pack_ambiguity_and_index() {
        let pack = files(&["a/First.mkv", "b/Second.MKV"]);
        assert_eq!(
            hinted(
                &movie(),
                &pack,
                DebridKind::RealDebrid,
                Some(0),
                Some("C:\\release\\SECOND.mkv")
            ),
            (FileSelection::Unique(1), FileSelectionMethod::Filename)
        );
        assert_eq!(
            hinted(
                &episode(1, 2),
                &files(&["01.mkv", "02.mkv"]),
                DebridKind::AllDebrid,
                None,
                Some("02.mkv")
            )
            .0,
            FileSelection::Unique(1)
        );
    }

    #[test]
    fn duplicate_basenames_require_agreeing_reliable_index() {
        let pack = files(&["a/Movie.mkv", "b/Movie.mkv"]);
        assert_eq!(
            hinted(
                &movie(),
                &pack,
                DebridKind::RealDebrid,
                Some(0),
                Some("movie.mkv")
            )
            .0,
            FileSelection::Unique(1)
        ); // ID 1, not position 0
        for provider in [DebridKind::RealDebrid, DebridKind::AllDebrid] {
            assert_eq!(
                hinted(&movie(), &pack, provider, None, Some("movie.mkv")).0,
                FileSelection::Ambiguous(vec![0, 1])
            );
        }
        assert_eq!(
            hinted(
                &movie(),
                &pack,
                DebridKind::AllDebrid,
                Some(0),
                Some("movie.mkv")
            )
            .0,
            FileSelection::Ambiguous(vec![0, 1])
        );
    }

    fn rd_pack(paths: &[&str]) -> Vec<RemoteFile> {
        let mut pack = files(paths);
        for (i, file) in pack.iter_mut().enumerate() {
            file.id = (i + 1).to_string();
        }
        pack
    }

    fn strict(season: u32, ep: u32, path: &str) -> FileSelection {
        select_file(&episode(season, ep), &files(&[path]))
    }

    #[test]
    fn resolutions_are_not_markers() {
        for name in [
            "[Grp] Show 01 [BDRip 1448x1080 x264 FLAC].mkv",
            "Show 01 966x720.mkv",
            "Show 01 1920x1080.mkv",
        ] {
            let pack = rd_pack(&[name]);
            assert_eq!(
                hinted(
                    &episode(1, 1),
                    &pack,
                    DebridKind::RealDebrid,
                    Some(0),
                    Some(name)
                )
                .0,
                FileSelection::Unique(0),
                "{name}"
            );
        }
        // A real marker next to a resolution still reads.
        assert_eq!(
            strict(1, 5, "Show 1x05 1920x1080.mkv"),
            FileSelection::Unique(0)
        );
        assert_eq!(
            strict(1, 5, "Show.S01E05.1448x1080.mkv"),
            FileSelection::Unique(0)
        );
        // Resolution-only names are no marker for the strict rule.
        assert_eq!(strict(1, 1, "Show 1448x1080.mkv"), FileSelection::NotFound);
    }

    #[test]
    fn s_digit_inside_a_word_is_not_a_malformed_marker() {
        let name = "Show 01 Title (1080p x265 q22 FS88 Joy).mkv";
        let pack = rd_pack(&[name]);
        assert_eq!(
            hinted(
                &episode(1, 1),
                &pack,
                DebridKind::RealDebrid,
                Some(0),
                Some(name)
            )
            .0,
            FileSelection::Unique(0)
        );
        // The marker reader ignores it beside a real marker too.
        assert_eq!(
            strict(1, 1, "Show.S01E01.FS88.mkv"),
            FileSelection::Unique(0)
        );
        // A glued full marker is still seen, and still contradicts.
        let glued = rd_pack(&["ShowS01E03.mkv"]);
        assert_eq!(
            hinted(
                &episode(1, 2),
                &glued,
                DebridKind::RealDebrid,
                Some(0),
                Some("ShowS01E03.mkv")
            )
            .0,
            FileSelection::NotFound
        );
        // A leading malformed `s<digit>` token still refuses.
        let bad = rd_pack(&["Show.S01Special.mkv"]);
        assert_eq!(
            hinted(
                &episode(1, 1),
                &bad,
                DebridKind::RealDebrid,
                Some(0),
                Some("Show.S01Special.mkv")
            )
            .0,
            FileSelection::NotFound
        );
    }

    #[test]
    fn revision_suffix_is_allowed() {
        assert_eq!(
            strict(1, 5, "[Grp] Show - S01E05v2.mkv"),
            FileSelection::Unique(0)
        );
        assert_eq!(strict(1, 5, "Show 1x05v2.mkv"), FileSelection::Unique(0));
        assert_eq!(strict(1, 6, "Show 1x05v2.mkv"), FileSelection::NotFound);
        let name = "[Grp] Show - S01E01v2.mkv";
        let pack = rd_pack(&[name]);
        assert_eq!(
            hinted(
                &episode(1, 1),
                &pack,
                DebridKind::RealDebrid,
                Some(0),
                Some(name)
            )
            .0,
            FileSelection::Unique(0)
        );
    }

    #[test]
    fn combined_and_contradicting_markers_stay_refused() {
        for name in [
            "Show.S01E01E02.mkv",
            "Show.S01E01-02.mkv",
            "Show - [1x16 - 1x17] - Two Parts.mkv",
            "Show.S01E01.S01E02.mkv",
            "Show.S01E05v2E06.mkv",
            "Show.S01E05.06.mkv",
            "Show.2nd.season.ep.-04x13~'Name'.(BDRip).avi",
        ] {
            for (s, e) in [(1, 1), (1, 2), (1, 5), (1, 16), (1, 17), (2, 13)] {
                assert_eq!(
                    strict(s, e, name),
                    FileSelection::NotFound,
                    "{name} {s}x{e}"
                );
                let pack = rd_pack(&[name]);
                assert_eq!(
                    hinted(
                        &episode(s, e),
                        &pack,
                        DebridKind::RealDebrid,
                        Some(0),
                        Some(name)
                    )
                    .0,
                    FileSelection::NotFound,
                    "{name} hint {s}x{e}"
                );
            }
        }
    }

    #[test]
    fn index_only_hint_needs_the_files_own_matching_marker() {
        let pack = rd_pack(&["Show/01.mkv", "Show/02.mkv", "Show/03.mkv"]);
        // Unnumbered files cannot verify an index: no selection.
        assert_eq!(
            hinted(&episode(1, 2), &pack, DebridKind::RealDebrid, Some(1), None),
            (FileSelection::NotFound, FileSelectionMethod::None)
        );
        let marked = rd_pack(&["Show.S01E01.mkv", "Show.S01E02.mkv", "Show.S01E03.mkv"]);
        assert_eq!(
            hinted(
                &episode(1, 2),
                &marked,
                DebridKind::RealDebrid,
                Some(1),
                None
            ),
            (FileSelection::Unique(1), FileSelectionMethod::FileIndex)
        );
        // Wrong index: the marker rejects it; the strict rule finds the file.
        assert_eq!(
            hinted(
                &episode(1, 3),
                &marked,
                DebridKind::RealDebrid,
                Some(1),
                None
            )
            .0,
            FileSelection::Unique(2)
        );
        // A blank filename counts as index-only.
        let blank = select_file_with_hint(
            &episode(1, 2),
            &pack,
            DebridKind::RealDebrid,
            Some(&FileHint {
                file_index: Some(1),
                filename: Some("  ".into()),
            }),
        );
        assert_eq!(blank.0, FileSelection::NotFound);
        // AllDebrid IDs are never indices.
        assert_eq!(
            hinted(
                &episode(1, 2),
                &marked,
                DebridKind::AllDebrid,
                Some(0),
                None
            )
            .1,
            FileSelectionMethod::Heuristic
        );
    }

    #[test]
    fn named_hint_with_index_still_allows_unnumbered_files() {
        let pack = rd_pack(&["Show/01.mkv", "Show/02.mkv"]);
        assert_eq!(
            hinted(
                &episode(1, 2),
                &pack,
                DebridKind::RealDebrid,
                Some(1),
                Some("02.mkv")
            ),
            (FileSelection::Unique(1), FileSelectionMethod::Filename)
        );
    }

    #[test]
    fn hints_never_admit_extras_nonvideo_or_conflicting_episode_markers() {
        for name in [
            "sample.mkv",
            "trailers/Movie.mp4",
            "movie.txt",
            "Show.S01E03.mkv",
            "Show.S01E02E03.mkv",
            "Show.S01E02-03.mkv",
            "ShowS01E03.mkv",
        ] {
            let mut pack = files(&[name]);
            pack[0].id = "1".into();
            assert_eq!(
                hinted(
                    &episode(1, 2),
                    &pack,
                    DebridKind::RealDebrid,
                    Some(0),
                    Some(name)
                )
                .0,
                FileSelection::NotFound,
                "{name}"
            );
        }
        // Invalid hints may still fall back to a different, strictly matched file.
        assert_eq!(
            hinted(
                &episode(1, 2),
                &files(&["sample.mkv", "Show.S01E02.mkv"]),
                DebridKind::RealDebrid,
                None,
                Some("sample.mkv")
            ),
            (FileSelection::Unique(1), FileSelectionMethod::Heuristic)
        );
    }

    #[test]
    fn real_debrid_index_uses_one_based_id_not_array_position() {
        let mut pack = files(&["third.mkv", "first.mkv", "second.mkv"]);
        for (file, id) in pack.iter_mut().zip(["3", "1", "2"]) {
            file.id = id.into();
        }
        for (hint, expected) in [(0, 1), (1, 2), (2, 0)] {
            assert_eq!(
                hinted(&movie(), &pack, DebridKind::RealDebrid, Some(hint), None),
                (
                    FileSelection::Unique(expected),
                    FileSelectionMethod::FileIndex
                )
            );
        }
        for index in [3, u32::MAX] {
            assert_eq!(
                hinted(&movie(), &pack, DebridKind::RealDebrid, Some(index), None).0,
                FileSelection::Ambiguous(vec![0, 1, 2])
            );
        }
    }

    #[test]
    fn alldebrid_tree_positions_are_not_torrent_indices() {
        let pack = files(&["folder/a.mkv", "b.mkv"]);
        assert_eq!(
            hinted(&movie(), &pack, DebridKind::AllDebrid, Some(0), None),
            (
                FileSelection::Ambiguous(vec![0, 1]),
                FileSelectionMethod::None
            )
        );
        assert_eq!(
            hinted(
                &movie(),
                &pack,
                DebridKind::AllDebrid,
                Some(0),
                Some("b.mkv")
            )
            .0,
            FileSelection::Unique(1)
        );
    }

    #[test]
    fn missing_hints_preserve_strict_selection_and_errors() {
        for target in [movie(), episode(1, 2)] {
            let pack = files(&["Show.S01E02.mkv", "Show.S01E03.mkv"]);
            assert_eq!(
                select_file_with_hint(&target, &pack, DebridKind::RealDebrid, None).0,
                select_file(&target, &pack)
            );
        }
        assert_eq!(FileSelection::Unique(0).error_message(), None);
        for selection in [
            FileSelection::NotFound,
            FileSelection::Ambiguous(vec![0, 1]),
        ] {
            assert_eq!(
                selection.error_message(),
                Some("Couldn't identify the file in this torrent — try another stream.")
            );
        }
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

/// Conservative display tags parsed from an indexer release description. These
/// are hints, never evidence of playback capability or debrid availability.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReleaseTags {
    pub release_name: String,
    pub codec: Option<&'static str>,
    pub audio: Option<String>,
    pub languages: Vec<String>,
    pub tracker: Option<String>,
}

pub fn parse_release_tags(name: &str) -> ReleaseTags {
    let lower = name.to_lowercase();
    let words: Vec<_> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let has = |s: &str| words.contains(&s);
    let codec = if ["hevc", "x265", "h265"].iter().any(|s| has(s)) || lower.contains("h.265") {
        Some("HEVC")
    } else if ["avc", "x264", "h264"].iter().any(|s| has(s)) || lower.contains("h.264") {
        Some("AVC")
    } else if has("av1") {
        Some("AV1")
    } else {
        None
    };
    let audio = if has("atmos") {
        Some("Atmos")
    } else if lower.contains("dts-hd") {
        Some("DTS-HD")
    } else if has("dts") {
        Some("DTS")
    } else if has("ddp") || lower.contains("ddp5") || has("eac3") {
        Some("DDP")
    } else if has("aac") {
        Some("AAC")
    } else {
        None
    };
    let audio = audio.map(|a| {
        let channels = ["7.1", "5.1", "2.0"]
            .into_iter()
            .find(|s| lower.contains(s));
        channels.map_or_else(|| a.to_owned(), |c| format!("{a} {c}"))
    });
    let mut languages = Vec::new();
    // Regional indicators are converted to ISO country codes, then mapped to
    // language codes where the country/language spellings differ.
    let mut regional = None;
    for ch in name.chars() {
        if ('\u{1f1e6}'..='\u{1f1ff}').contains(&ch) {
            let letter = (b'A' + (ch as u32 - 0x1f1e6) as u8) as char;
            if let Some(first) = regional.take() {
                let code = format!("{first}{letter}");
                let code = match code.as_str() {
                    "GB" | "US" => "EN",
                    "JP" => "JA",
                    "CN" => "ZH",
                    "KR" => "KO",
                    "BR" => "PT",
                    other => other,
                }
                .to_owned();
                if !languages.contains(&code) {
                    languages.push(code);
                }
            } else {
                regional = Some(letter);
            }
        } else {
            regional = None;
        }
    }
    for (code, names) in [
        ("EN", &["english", "eng"][..]),
        ("IT", &["italian", "ita"]),
        ("ES", &["spanish", "spa"]),
        ("FR", &["french", "fra", "fre"]),
        ("DE", &["german", "deu", "ger"]),
        ("PT", &["portuguese", "por"]),
        ("JA", &["japanese", "jpn"]),
        ("ZH", &["chinese", "zho"]),
        ("KO", &["korean", "kor"]),
        ("RU", &["russian", "rus"]),
        ("HI", &["hindi", "hin"]),
    ] {
        if names.iter().any(|n| has(n)) && !languages.iter().any(|s| s == code) {
            languages.push(code.to_owned());
        }
    }
    let tracker = name
        .lines()
        .find_map(|line| line.split_once("🌐").map(|(_, s)| s.trim().to_owned()))
        .filter(|s| !s.is_empty());
    let release_name = name
        .lines()
        .next()
        .unwrap_or(name)
        .split(['👤', '💾', '🌐', '⚙'])
        .next()
        .unwrap_or(name)
        .trim()
        .to_owned();
    ReleaseTags {
        release_name,
        codec,
        audio,
        languages,
        tracker,
    }
}

/// Match both the original release and the normalized display tags (e.g. x265
/// is searchable as HEVC, and flag emoji are searchable by language code).
pub fn release_matches_filter(name: &str, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    if filter.is_empty() || name.to_lowercase().contains(&filter) {
        return true;
    }
    let tags = parse_release_tags(name);
    tags.codec
        .is_some_and(|s| s.to_lowercase().contains(&filter))
        || tags
            .audio
            .is_some_and(|s| s.to_lowercase().contains(&filter))
        || tags
            .languages
            .iter()
            .any(|s| s.to_lowercase().contains(&filter))
}

#[cfg(test)]
mod release_tag_tests {
    use super::*;
    #[test]
    fn release_tags_flags_names_audio_and_tracker() {
        let tags =
            parse_release_tags("Film.1080p.H.265.DDP5.1.Atmos 🇬🇧 🇮🇹 English\n🌐 TorrentGalaxy");
        assert_eq!(tags.codec, Some("HEVC"));
        assert_eq!(tags.audio.as_deref(), Some("Atmos 5.1"));
        assert_eq!(tags.languages, ["EN", "IT"]);
        assert_eq!(tags.tracker.as_deref(), Some("TorrentGalaxy"));
    }
    #[test]
    fn release_tags_do_not_guess_from_substrings() {
        assert!(release_matches_filter("Movie.x265 🇮🇹", "hevc"));
        assert!(release_matches_filter("Movie.x265 🇮🇹", "IT"));
        assert!(!release_matches_filter("Movie.x265 🇮🇹", "EN"));
        let tags = parse_release_tags("Frenchman.Englishman.2026");
        assert_eq!(tags.codec, None);
        assert!(tags.languages.is_empty());
        assert_eq!(tags.release_name, "Frenchman.Englishman.2026");
        assert_eq!(
            parse_release_tags("Movie.AV1.Japanese 🇯🇵").languages,
            ["JA"]
        );
    }
}
