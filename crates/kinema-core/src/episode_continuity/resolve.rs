//! Release similarity and the neighbour-stream resolution flow.

use super::traits::{ReleaseTraits, Resolution, Source};
use crate::sources::{
    DebridKind, FileHint, FileSelection, PlaybackTarget, RemoteFile, SourceCandidate, select_file,
    select_file_with_hint,
};

/// The release the neighbour is compared against: the current torrent, or the
/// remembered one for the season. Stores RAW strings, exactly as persisted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReferenceRelease {
    pub name: String,
    pub file_name: Option<String>,
    pub size: Option<u64>,
}

impl ReferenceRelease {
    /// Traits re-derived from the raw strings.
    pub fn traits(&self) -> ReleaseTraits {
        ReleaseTraits::parse(&self.name, self.file_name.as_deref())
    }
}

/// Traits of an indexer candidate (its name and hinted file name).
pub fn candidate_traits(candidate: &SourceCandidate) -> ReleaseTraits {
    ReleaseTraits::parse(&candidate.name, candidate.torrent.filename.as_deref())
}

/// How alike a candidate is to the reference. Fields are in priority order and
/// the derived `Ord` compares them in that order, so a greater value is a
/// closer match. A trait missing on either side never matches.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Similarity {
    pub group: bool,
    pub resolution: bool,
    pub source: bool,
    pub codec: bool,
    /// Both sides list HDR/DV flags and they are equal.
    pub hdr: bool,
    pub audio: bool,
    /// `min / max` of the sizes in permille (0 when either is unknown).
    pub size_permille: u16,
    /// Unverified hint; the last tiebreak. `None` ranks below any count.
    pub seeders: Option<u32>,
}

impl Similarity {
    /// A strong match: release group AND resolution both present and equal.
    pub fn strong(self) -> bool {
        self.group && self.resolution
    }
}

/// Similarity of `candidate` to `reference`.
pub fn similarity(reference: &ReferenceRelease, candidate: &SourceCandidate) -> Similarity {
    similarity_of(&reference.traits(), reference.size, candidate)
}

fn similarity_of(
    reference: &ReleaseTraits,
    reference_size: Option<u64>,
    candidate: &SourceCandidate,
) -> Similarity {
    let c = candidate_traits(candidate);
    fn same<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
        matches!((a, b), (Some(a), Some(b)) if a == b)
    }
    let size_permille = match (reference_size, candidate.size) {
        (Some(a), Some(b)) if a > 0 && b > 0 => {
            let (lo, hi) = (u128::from(a.min(b)), u128::from(a.max(b)));
            u16::try_from(lo * 1000 / hi).unwrap_or(1000)
        }
        _ => 0,
    };
    Similarity {
        group: same(&reference.group, &c.group),
        resolution: same(&reference.resolution, &c.resolution),
        source: same(&reference.source, &c.source),
        codec: same(&reference.codec, &c.codec),
        hdr: !reference.hdr.is_empty() && reference.hdr == c.hdr,
        audio: same(&reference.audio, &c.audio),
        size_permille,
        seeders: candidate.seeders,
    }
}

/// A candidate with its score.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RankedCandidate {
    pub candidate: SourceCandidate,
    pub similarity: Similarity,
}

/// Candidates best-first. The order is total and independent of input order:
/// similarity (descending), then name, then info hash, then input position.
pub fn rank_candidates(
    reference: &ReferenceRelease,
    candidates: &[SourceCandidate],
) -> Vec<RankedCandidate> {
    let traits = reference.traits();
    let mut ranked: Vec<(usize, RankedCandidate)> = candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let similarity = similarity_of(&traits, reference.size, candidate);
            (
                i,
                RankedCandidate {
                    candidate: candidate.clone(),
                    similarity,
                },
            )
        })
        .collect();
    ranked.sort_by(|(ia, a), (ib, b)| {
        b.similarity
            .cmp(&a.similarity)
            .then_with(|| a.candidate.name.cmp(&b.candidate.name))
            .then_with(|| {
                a.candidate
                    .torrent
                    .hash
                    .as_str()
                    .cmp(b.candidate.torrent.hash.as_str())
            })
            .then_with(|| ia.cmp(ib))
    });
    ranked.into_iter().map(|(_, r)| r).collect()
}

/// The indexer query for the neighbour failed (network, rate limit, parse).
/// Distinct from a successful empty result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexerFailed;

/// The kept debrid preparation: every file the torrent lists.
#[derive(Clone, Copy, Debug)]
pub struct KeptFiles<'a> {
    pub provider: DebridKind,
    pub files: &'a [RemoteFile],
}

/// Inputs of [`choose_neighbour_stream`].
#[derive(Clone, Copy, Debug)]
pub struct NeighbourQuery<'a> {
    /// The neighbour episode (a [`PlaybackTarget::Episode`]).
    pub target: &'a PlaybackTarget,
    /// Info hash of the torrent now playing.
    pub current_hash: &'a crate::sources::InfoHash,
    /// The kept preparation of the current torrent, when the window holds one.
    pub kept: Option<KeptFiles<'a>>,
    /// The indexer result for the neighbour.
    pub indexer: Result<&'a [SourceCandidate], IndexerFailed>,
    /// The current release, or the season's remembered one (the previous
    /// season's when crossing into a season with none).
    pub reference: &'a ReferenceRelease,
}

/// Why a choice was made, for the UI. See [`ChoiceReason::label`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChoiceReason {
    /// The indexer lists the playing torrent for the neighbour.
    SamePack,
    /// A different torrent with the same release group and resolution.
    StrongMatch {
        resolution: Resolution,
        source: Option<Source>,
    },
    /// The strict episode-marker rule found the file in the kept pack.
    KeptFileByMarker,
    /// Candidates exist but none shares both release group and resolution.
    NoStrongMatch,
    /// The playing torrent lists the episode, but its file is not unique.
    PackFileNotUnique,
    /// The indexer failed and the kept pack has no unique file for it.
    IndexerFailed,
    /// The indexer answered with no streams.
    NoStreams,
}

impl ChoiceReason {
    /// Short UI text, e.g. "Same pack" or "Same release group · 1080p WEB-DL".
    pub fn label(self) -> String {
        match self {
            Self::SamePack => "Same pack".into(),
            Self::StrongMatch { resolution, source } => {
                let mut label = format!("Same release group · {}", resolution.label());
                if let Some(source) = source {
                    label.push(' ');
                    label.push_str(source.label());
                }
                label
            }
            Self::KeptFileByMarker => "Same pack · by episode number".into(),
            Self::NoStrongMatch => "No close match — pick a stream".into(),
            Self::PackFileNotUnique => {
                "Couldn't identify the episode in this pack — pick a stream".into()
            }
            Self::IndexerFailed => "Couldn't find a stream for this episode".into(),
            Self::NoStreams => "No streams found for this episode".into(),
        }
    }
}

/// What to do for the neighbour episode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NeighbourChoice {
    /// Play `file_index` (index into the kept file list) of the playing torrent.
    SamePack {
        candidate: SourceCandidate,
        file_index: usize,
    },
    /// Prepare `candidate` and play without asking. The file inside it is
    /// chosen at its own preparation by the existing rules; if that is not
    /// unique the caller must fall back to the picker, never guess.
    /// `reason` is [`ChoiceReason::StrongMatch`] with the shared traits, or
    /// [`ChoiceReason::SamePack`] when the indexer lists the playing torrent
    /// but no kept file list is available.
    StrongMatch {
        candidate: SourceCandidate,
        reason: ChoiceReason,
    },
    /// Open the stream picker; the first entry is preselected.
    Picker {
        ranked: Vec<RankedCandidate>,
        reason: ChoiceReason,
    },
    /// The indexer failed; the strict marker rule found one file in the kept pack.
    KeptFile { file_index: usize },
    /// Nothing to play or pick. Show a notice with the reason.
    Unavailable { reason: ChoiceReason },
}

impl NeighbourChoice {
    /// The reason in UI terms; `.label()` gives the text.
    pub fn reason(&self) -> ChoiceReason {
        match self {
            Self::SamePack { .. } => ChoiceReason::SamePack,
            Self::KeptFile { .. } => ChoiceReason::KeptFileByMarker,
            Self::StrongMatch { reason, .. }
            | Self::Picker { reason, .. }
            | Self::Unavailable { reason } => *reason,
        }
    }
}

/// Resolves the stream for a neighbour episode. Pure; the caller runs the
/// indexer query and holds the kept preparation.
///
/// 1. Indexer failed: the strict marker rule on the kept files if it yields one
///    file ([`NeighbourChoice::KeptFile`]), else `Unavailable(IndexerFailed)`.
/// 2. Indexer returned nothing: `Unavailable(NoStreams)`. This is deliberately
///    not the failure fallback.
/// 3. A row lists the playing torrent: with kept files, its hint is applied via
///    [`select_file_with_hint`] (including the index-only rule); one unique file
///    gives [`NeighbourChoice::SamePack`], otherwise the picker opens, without
///    the playing torrent since its file cannot be identified. Without kept
///    files the row is a `StrongMatch` (file chosen when prepared).
/// 4. Otherwise rank by [`Similarity`]; the best is a `StrongMatch` when
///    [`Similarity::strong`], else the picker opens with the ranking.
pub fn choose_neighbour_stream(query: &NeighbourQuery<'_>) -> NeighbourChoice {
    let candidates = match query.indexer {
        Err(IndexerFailed) => return indexer_failed(query),
        Ok(candidates) => candidates,
    };
    if candidates.is_empty() {
        return NeighbourChoice::Unavailable {
            reason: ChoiceReason::NoStreams,
        };
    }
    let same_hash: Vec<&SourceCandidate> = candidates
        .iter()
        .filter(|c| &c.torrent.hash == query.current_hash)
        .collect();
    if let Some(first) = same_hash.first() {
        let Some(kept) = query.kept else {
            return NeighbourChoice::StrongMatch {
                candidate: (*first).clone(),
                reason: ChoiceReason::SamePack,
            };
        };
        let mut unique: Option<(usize, &SourceCandidate)> = None;
        let mut conflict = false;
        for row in &same_hash {
            let hint = FileHint::from(&row.torrent);
            let (selection, _) =
                select_file_with_hint(query.target, kept.files, kept.provider, Some(&hint));
            if let FileSelection::Unique(index) = selection {
                match unique {
                    Some((other, _)) if other != index => conflict = true,
                    Some(_) => {}
                    None => unique = Some((index, row)),
                }
            }
        }
        return match unique {
            Some((file_index, row)) if !conflict => NeighbourChoice::SamePack {
                candidate: row.clone(),
                file_index,
            },
            _ => {
                let others: Vec<SourceCandidate> = candidates
                    .iter()
                    .filter(|c| &c.torrent.hash != query.current_hash)
                    .cloned()
                    .collect();
                if others.is_empty() {
                    return NeighbourChoice::Unavailable {
                        reason: ChoiceReason::PackFileNotUnique,
                    };
                }
                NeighbourChoice::Picker {
                    ranked: rank_candidates(query.reference, &others),
                    reason: ChoiceReason::PackFileNotUnique,
                }
            }
        };
    }
    let ranked = rank_candidates(query.reference, candidates);
    match ranked.first() {
        Some(best) if best.similarity.strong() => {
            let traits = candidate_traits(&best.candidate);
            // `strong` guarantees a resolution on the candidate.
            let reason = match traits.resolution {
                Some(resolution) => ChoiceReason::StrongMatch {
                    resolution,
                    source: traits.source,
                },
                None => ChoiceReason::NoStrongMatch,
            };
            NeighbourChoice::StrongMatch {
                candidate: best.candidate.clone(),
                reason,
            }
        }
        _ => NeighbourChoice::Picker {
            ranked,
            reason: ChoiceReason::NoStrongMatch,
        },
    }
}

fn indexer_failed(query: &NeighbourQuery<'_>) -> NeighbourChoice {
    if let Some(kept) = query.kept
        && let FileSelection::Unique(file_index) = select_file(query.target, kept.files)
    {
        return NeighbourChoice::KeptFile { file_index };
    }
    NeighbourChoice::Unavailable {
        reason: ChoiceReason::IndexerFailed,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{
        catalog::ImdbId,
        sources::{InfoHash, TorrentSource},
    };

    fn hash(n: u32) -> InfoHash {
        InfoHash::new(format!("{n:040x}")).expect("hash")
    }
    fn cand(
        n: u32,
        name: &str,
        file: Option<(Option<u32>, Option<&str>)>,
        size: Option<u64>,
        seeders: Option<u32>,
    ) -> SourceCandidate {
        let (file_index, filename) = file.unwrap_or((None, None));
        SourceCandidate {
            torrent: TorrentSource {
                hash: hash(n),
                file_index,
                filename: filename.map(str::to_owned),
            },
            name: name.into(),
            quality: None,
            size,
            seeders,
        }
    }
    fn target(season: u32, episode: u32) -> PlaybackTarget {
        PlaybackTarget::Episode {
            series: ImdbId::new("tt0000001").expect("id"),
            season,
            episode,
        }
    }
    fn reference(name: &str) -> ReferenceRelease {
        ReferenceRelease {
            name: name.into(),
            file_name: None,
            size: Some(1_000),
        }
    }
    fn pack() -> Vec<RemoteFile> {
        ["Show/01.mkv", "Show/02.mkv", "Show/03.mkv"]
            .iter()
            .enumerate()
            .map(|(i, p)| RemoteFile {
                id: (i + 1).to_string(),
                path: (*p).into(),
                size: 1,
            })
            .collect()
    }
    fn marked_pack() -> Vec<RemoteFile> {
        ["Show.S01E01.mkv", "Show.S01E02.mkv", "Show.S01E03.mkv"]
            .iter()
            .enumerate()
            .map(|(i, p)| RemoteFile {
                id: (i + 1).to_string(),
                path: (*p).into(),
                size: 1,
            })
            .collect()
    }
    const CURRENT: &str = "Show.S01.1080p.WEB-DL.x265-GRP";

    fn query<'a>(
        target: &'a PlaybackTarget,
        current: &'a InfoHash,
        kept: Option<KeptFiles<'a>>,
        indexer: Result<&'a [SourceCandidate], IndexerFailed>,
        reference: &'a ReferenceRelease,
    ) -> NeighbourQuery<'a> {
        NeighbourQuery {
            target,
            current_hash: current,
            kept,
            indexer,
            reference,
        }
    }

    #[test]
    fn same_pack_uses_the_row_hint() {
        let files = pack();
        let t = target(1, 3);
        let cur = hash(1);
        let rows = [
            cand(1, CURRENT, Some((Some(2), Some("03.mkv"))), None, Some(5)),
            cand(2, "Other.1080p-GRP", None, None, None),
        ];
        let r = reference(CURRENT);
        let kept = KeptFiles {
            provider: DebridKind::RealDebrid,
            files: &files,
        };
        let got = choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r));
        assert_eq!(
            got,
            NeighbourChoice::SamePack {
                candidate: rows[0].clone(),
                file_index: 2
            }
        );
        assert_eq!(got.reason().label(), "Same pack");
    }

    #[test]
    fn same_pack_works_for_all_debrid_by_filename() {
        let files = pack();
        let t = target(1, 2);
        let cur = hash(1);
        let rows = [cand(
            1,
            CURRENT,
            Some((Some(1), Some("02.mkv"))),
            None,
            None,
        )];
        let r = reference(CURRENT);
        let kept = KeptFiles {
            provider: DebridKind::AllDebrid,
            files: &files,
        };
        assert!(matches!(
            choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r)),
            NeighbourChoice::SamePack { file_index: 1, .. }
        ));
    }

    #[test]
    fn same_pack_index_only_hint_needs_a_matching_marker() {
        let t = target(1, 2);
        let cur = hash(1);
        let r = reference(CURRENT);
        let rows = [cand(1, CURRENT, Some((Some(1), None)), None, None)];
        let numbered = marked_pack();
        let kept = KeptFiles {
            provider: DebridKind::RealDebrid,
            files: &numbered,
        };
        assert!(matches!(
            choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r)),
            NeighbourChoice::SamePack { file_index: 1, .. }
        ));
        // Unnumbered files: the index cannot be verified, so the picker opens
        // (here with the only other stream).
        let unnumbered = pack();
        let kept = KeptFiles {
            provider: DebridKind::RealDebrid,
            files: &unnumbered,
        };
        let rows = [
            cand(1, CURRENT, Some((Some(1), None)), None, None),
            cand(2, "Show.S01E02.720p.WEB-DL-OTHER", None, None, None),
        ];
        match choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r)) {
            NeighbourChoice::Picker { ranked, reason } => {
                assert_eq!(reason, ChoiceReason::PackFileNotUnique);
                assert_eq!(ranked.len(), 1);
                assert_eq!(ranked[0].candidate.torrent.hash, hash(2));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn contradicting_hint_goes_to_picker_or_strict_file() {
        let t = target(1, 2);
        let cur = hash(1);
        let r = reference(CURRENT);
        // The hint names a file whose marker contradicts: the strict rule on
        // the numbered pack still finds the right one.
        let numbered = marked_pack();
        let kept = KeptFiles {
            provider: DebridKind::RealDebrid,
            files: &numbered,
        };
        let rows = [cand(
            1,
            CURRENT,
            Some((Some(2), Some("Show.S01E03.mkv"))),
            None,
            None,
        )];
        assert!(matches!(
            choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r)),
            NeighbourChoice::SamePack { file_index: 1, .. }
        ));
        // Unnumbered pack, hint names a non-existent file: nothing unique.
        let plain = pack();
        let kept = KeptFiles {
            provider: DebridKind::AllDebrid,
            files: &plain,
        };
        let rows = [cand(1, CURRENT, Some((None, Some("zz.mkv"))), None, None)];
        assert_eq!(
            choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r)),
            NeighbourChoice::Unavailable {
                reason: ChoiceReason::PackFileNotUnique
            }
        );
    }

    #[test]
    fn ambiguous_hint_is_not_a_same_pack_play() {
        let t = target(1, 2);
        let cur = hash(1);
        let r = reference(CURRENT);
        let files: Vec<RemoteFile> = ["A/02.mkv", "B/02.mkv"]
            .iter()
            .enumerate()
            .map(|(i, p)| RemoteFile {
                id: (i + 1).to_string(),
                path: (*p).into(),
                size: 1,
            })
            .collect();
        let kept = KeptFiles {
            provider: DebridKind::AllDebrid,
            files: &files,
        };
        let rows = [
            cand(1, CURRENT, Some((None, Some("02.mkv"))), None, None),
            cand(3, "Show.S01E02.1080p.WEB-DL.x265-GRP", None, None, None),
        ];
        match choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r)) {
            NeighbourChoice::Picker { reason, ranked } => {
                assert_eq!(reason, ChoiceReason::PackFileNotUnique);
                assert_eq!(ranked.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn conflicting_rows_for_the_same_hash_are_not_unique() {
        let t = target(1, 2);
        let cur = hash(1);
        let r = reference(CURRENT);
        let files = pack();
        let kept = KeptFiles {
            provider: DebridKind::AllDebrid,
            files: &files,
        };
        let rows = [
            cand(1, CURRENT, Some((None, Some("01.mkv"))), None, None),
            cand(1, CURRENT, Some((None, Some("02.mkv"))), None, None),
        ];
        assert!(matches!(
            choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&rows), &r)),
            NeighbourChoice::Unavailable { .. }
        ));
    }

    #[test]
    fn same_hash_without_kept_files_defers_the_file() {
        let t = target(2, 1); // crossing a season inside a complete-series pack
        let cur = hash(1);
        let r = reference(CURRENT);
        let rows = [cand(1, CURRENT, Some((Some(9), Some("x.mkv"))), None, None)];
        let got = choose_neighbour_stream(&query(&t, &cur, None, Ok(&rows), &r));
        assert!(matches!(
            got,
            NeighbourChoice::StrongMatch {
                reason: ChoiceReason::SamePack,
                ..
            }
        ));
    }

    #[test]
    fn strong_match_needs_group_and_resolution() {
        let t = target(1, 3);
        let cur = hash(1);
        let r = reference(CURRENT);
        let rows = [
            cand(
                2,
                "Show.S01E03.1080p.WEB-DL.x264-OTHER",
                None,
                Some(900),
                Some(500),
            ),
            cand(
                3,
                "Show.S01E03.720p.WEB-DL.x265-GRP",
                None,
                Some(1000),
                Some(500),
            ),
            cand(
                4,
                "Show.S01E03.1080p.BluRay.x264-GRP",
                None,
                Some(2000),
                Some(1),
            ),
        ];
        let got = choose_neighbour_stream(&query(&t, &cur, None, Ok(&rows), &r));
        match &got {
            NeighbourChoice::StrongMatch { candidate, reason } => {
                assert_eq!(candidate.torrent.hash, hash(4));
                assert_eq!(reason.label(), "Same release group · 1080p BluRay");
            }
            other => panic!("{other:?}"),
        }
        let weak = [rows[0].clone(), rows[1].clone()];
        match choose_neighbour_stream(&query(&t, &cur, None, Ok(&weak), &r)) {
            NeighbourChoice::Picker { ranked, reason } => {
                assert_eq!(reason, ChoiceReason::NoStrongMatch);
                assert_eq!(ranked.len(), 2);
                assert!(ranked.iter().all(|c| !c.similarity.strong()));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_reference_traits_never_make_a_strong_match() {
        let t = target(1, 3);
        let cur = hash(1);
        let r = reference("Show Season 1");
        let rows = [cand(2, "Show.S01E03", None, None, None)];
        assert!(matches!(
            choose_neighbour_stream(&query(&t, &cur, None, Ok(&rows), &r)),
            NeighbourChoice::Picker { .. }
        ));
    }

    #[test]
    fn indexer_failure_is_not_an_empty_result() {
        let t = target(1, 2);
        let cur = hash(1);
        let r = reference(CURRENT);
        let numbered = marked_pack();
        let kept = KeptFiles {
            provider: DebridKind::AllDebrid,
            files: &numbered,
        };
        let failed = choose_neighbour_stream(&query(&t, &cur, Some(kept), Err(IndexerFailed), &r));
        assert_eq!(failed, NeighbourChoice::KeptFile { file_index: 1 });
        let empty = choose_neighbour_stream(&query(&t, &cur, Some(kept), Ok(&[]), &r));
        assert_eq!(
            empty,
            NeighbourChoice::Unavailable {
                reason: ChoiceReason::NoStreams
            }
        );
    }

    #[test]
    fn indexer_failure_without_a_unique_kept_file_is_a_notice() {
        let t = target(1, 2);
        let cur = hash(1);
        let r = reference(CURRENT);
        let unnumbered = pack();
        let kept = KeptFiles {
            provider: DebridKind::RealDebrid,
            files: &unnumbered,
        };
        let expect = NeighbourChoice::Unavailable {
            reason: ChoiceReason::IndexerFailed,
        };
        assert_eq!(
            choose_neighbour_stream(&query(&t, &cur, Some(kept), Err(IndexerFailed), &r)),
            expect
        );
        assert_eq!(
            choose_neighbour_stream(&query(&t, &cur, None, Err(IndexerFailed), &r)),
            expect
        );
        let dup: Vec<RemoteFile> = ["a/Show.S01E02.mkv", "b/Show.S01E02.mkv"]
            .iter()
            .enumerate()
            .map(|(i, p)| RemoteFile {
                id: i.to_string(),
                path: (*p).into(),
                size: 1,
            })
            .collect();
        let kept = KeptFiles {
            provider: DebridKind::RealDebrid,
            files: &dup,
        };
        assert_eq!(
            choose_neighbour_stream(&query(&t, &cur, Some(kept), Err(IndexerFailed), &r)),
            expect
        );
    }

    #[test]
    fn labels() {
        assert_eq!(ChoiceReason::SamePack.label(), "Same pack");
        assert_eq!(
            ChoiceReason::StrongMatch {
                resolution: Resolution::P1080,
                source: Some(Source::WebDl)
            }
            .label(),
            "Same release group · 1080p WEB-DL"
        );
        assert_eq!(
            ChoiceReason::StrongMatch {
                resolution: Resolution::P720,
                source: None
            }
            .label(),
            "Same release group · 720p"
        );
        for reason in [
            ChoiceReason::KeptFileByMarker,
            ChoiceReason::NoStrongMatch,
            ChoiceReason::PackFileNotUnique,
            ChoiceReason::IndexerFailed,
            ChoiceReason::NoStreams,
        ] {
            assert!(!reason.label().is_empty());
        }
    }

    #[test]
    fn similarity_priority_and_missing_traits() {
        let r = reference(CURRENT);
        let group_only = similarity(&r, &cand(2, "Show.720p.BluRay.x264-GRP", None, None, None));
        let all_but_group = similarity(
            &r,
            &cand(3, "Show.1080p.WEB-DL.x265.AAC-ZZZ", None, None, None),
        );
        assert!(
            group_only > all_but_group,
            "group outranks every other trait"
        );
        let res_only = similarity(&r, &cand(4, "Show.1080p.BluRay.x264", None, None, None));
        let source_only = similarity(&r, &cand(5, "Show.720p.WEB-DL.x264", None, None, None));
        assert!(res_only > source_only);
        // Missing never matches, even two missing.
        let none = similarity(&reference("Show"), &cand(6, "Show", None, None, None));
        assert!(
            !none.group
                && !none.resolution
                && !none.source
                && !none.codec
                && !none.hdr
                && !none.audio
        );
        assert!(!none.strong());
        // HDR: only equal non-empty flags match.
        let hdr = reference("Show.2160p.HDR.x265-GRP");
        assert!(similarity(&hdr, &cand(7, "Show.2160p.HDR.x265-GRP", None, None, None)).hdr);
        assert!(!similarity(&hdr, &cand(7, "Show.2160p.DV.x265-GRP", None, None, None)).hdr);
        assert!(!similarity(&r, &cand(7, "Show.1080p.HDR", None, None, None)).hdr);
    }

    #[test]
    fn size_ratio_then_seeders_break_ties() {
        let r = reference(CURRENT); // size 1000
        let near = cand(2, "Show.1080p.WEB-DL.x265-GRP", None, Some(900), Some(1));
        let far = cand(
            3,
            "Show.1080p.WEB-DL.x265-GRP",
            None,
            Some(100),
            Some(9_999),
        );
        let unknown = cand(4, "Show.1080p.WEB-DL.x265-GRP", None, None, Some(9_999));
        let order = |rows: &[SourceCandidate]| -> Vec<InfoHash> {
            rank_candidates(&r, rows)
                .into_iter()
                .map(|c| c.candidate.torrent.hash)
                .collect()
        };
        assert_eq!(
            order(&[unknown.clone(), far.clone(), near.clone()]),
            [hash(2), hash(3), hash(4)]
        );
        assert_eq!(similarity(&r, &near).size_permille, 900);
        assert_eq!(similarity(&r, &unknown).size_permille, 0);
        // Same size: seeders decide; None ranks last.
        let a = cand(5, "Show.1080p.WEB-DL.x265-GRP", None, Some(1000), Some(3));
        let b = cand(6, "Show.1080p.WEB-DL.x265-GRP", None, Some(1000), Some(30));
        let c = cand(7, "Show.1080p.WEB-DL.x265-GRP", None, Some(1000), None);
        assert_eq!(order(&[c, a, b]), [hash(6), hash(5), hash(7)]);
    }

    #[test]
    fn ranking_is_total_and_input_order_independent() {
        let r = reference(CURRENT);
        let rows = [
            cand(2, "B.1080p", None, None, None),
            cand(3, "A.1080p", None, None, None),
            cand(4, "A.1080p", None, None, None),
            cand(5, "A.1080p", None, None, None),
        ];
        let mut reversed = rows.to_vec();
        reversed.reverse();
        let a: Vec<_> = rank_candidates(&r, &rows)
            .into_iter()
            .map(|c| c.candidate)
            .collect();
        let b: Vec<_> = rank_candidates(&r, &reversed)
            .into_iter()
            .map(|c| c.candidate)
            .collect();
        assert_eq!(a, b);
        assert_eq!(a[0].torrent.hash, hash(3));
        assert_eq!(a[3].name, "B.1080p");
    }

    #[test]
    fn reference_traits_come_from_raw_strings() {
        let r = ReferenceRelease {
            name: "Show.S01.BluRay".into(),
            file_name: Some("Show/Show.S01E02.1080p-GRP.mkv".into()),
            size: None,
        };
        let t = r.traits();
        assert_eq!(t.group.as_deref(), Some("grp"));
        assert_eq!(t.resolution, Some(Resolution::P1080));
        assert_eq!(t.source, Some(Source::BluRay));
    }
}
