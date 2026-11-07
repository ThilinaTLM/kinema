//! Provider-neutral subtitle search types and pure policies.
//!
//! No network or filesystem access lives here; adapters fill these types and the
//! owner applies [`rank_results`] before display.

use crate::sources::PlaybackTarget;
use std::cmp::Reverse;

/// Bytes hashed at each end of the file by the OpenSubtitles movie hash.
pub const MOVIE_HASH_CHUNK: usize = 64 * 1024;

/// What to search for. IMDb ids are numeric, without `tt` or leading zeros.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SubtitleTarget {
    Movie {
        imdb: u32,
    },
    Episode {
        series_imdb: u32,
        season: u32,
        episode: u32,
    },
}

impl SubtitleTarget {
    /// `None` when the IMDb id does not fit the numeric form (or is zero).
    pub fn from_playback(target: &PlaybackTarget) -> Option<Self> {
        let imdb = parse_imdb(target.imdb_id().as_str())?;
        Some(match target {
            PlaybackTarget::Movie(_) => Self::Movie { imdb },
            PlaybackTarget::Episode {
                season, episode, ..
            } => Self::Episode {
                series_imdb: imdb,
                season: *season,
                episode: *episode,
            },
        })
    }
}

impl TryFrom<&PlaybackTarget> for SubtitleTarget {
    type Error = ();
    fn try_from(target: &PlaybackTarget) -> Result<Self, ()> {
        Self::from_playback(target).ok_or(())
    }
}

/// Parses `tt0133093` or `0133093` into `133093`; zero and overflow are rejected.
fn parse_imdb(value: &str) -> Option<u32> {
    let digits = value.strip_prefix("tt").unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().filter(|id| *id != 0)
}

/// A provider-neutral search request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubtitleQuery {
    pub target: Option<SubtitleTarget>,
    /// Free-text title search, used when no target is known or the user edits it.
    pub text: Option<String>,
    /// ISO 639-1 lowercase codes.
    pub languages: Vec<String>,
    pub moviehash: Option<u64>,
}

/// One downloadable subtitle file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubtitleResult {
    pub file_id: u64,
    pub language: String,
    pub release: String,
    pub file_name: Option<String>,
    pub download_count: u64,
    pub hearing_impaired: bool,
    pub foreign_parts_only: bool,
    pub ai_translated: bool,
    pub machine_translated: bool,
    pub from_trusted: bool,
    pub moviehash_match: bool,
    pub format: Option<String>,
}

/// Orders results for display and drops machine translations.
///
/// Languages follow the user's preference order (unlisted languages last, in
/// first-seen order); within a language: hash match, human before AI-translated,
/// trusted uploader, then download count descending. The sort is stable.
pub fn rank_results(results: Vec<SubtitleResult>, languages: &[String]) -> Vec<SubtitleResult> {
    let mut results: Vec<_> = results
        .into_iter()
        .filter(|result| !result.machine_translated)
        .collect();
    let mut unlisted: Vec<String> = Vec::new();
    let mut keyed: Vec<(usize, SubtitleResult)> = results
        .drain(..)
        .map(|result| {
            let rank = languages
                .iter()
                .position(|code| code.eq_ignore_ascii_case(&result.language))
                .unwrap_or_else(|| {
                    let position = unlisted
                        .iter()
                        .position(|code| code.eq_ignore_ascii_case(&result.language))
                        .unwrap_or_else(|| {
                            unlisted.push(result.language.clone());
                            unlisted.len() - 1
                        });
                    languages.len() + position
                });
            (rank, result)
        })
        .collect();
    keyed.sort_by_key(|(rank, result)| {
        (
            *rank,
            Reverse(result.moviehash_match),
            result.ai_translated,
            Reverse(result.from_trusted),
            Reverse(result.download_count),
        )
    });
    keyed.into_iter().map(|(_, result)| result).collect()
}

/// The OpenSubtitles movie hash: file size plus the wrapping sum of the
/// little-endian `u64` words of the first and last 64 KiB.
///
/// Published algorithm: <https://trac.opensubtitles.org/projects/opensubtitles/wiki/HashSourceCodes>.
/// `None` when the file is smaller than 64 KiB or either slice is not exactly
/// 64 KiB.
pub fn movie_hash(size: u64, head: &[u8], tail: &[u8]) -> Option<u64> {
    if size < MOVIE_HASH_CHUNK as u64
        || head.len() != MOVIE_HASH_CHUNK
        || tail.len() != MOVIE_HASH_CHUNK
    {
        return None;
    }
    let sum = |bytes: &[u8]| {
        bytes.chunks_exact(8).fold(0u64, |acc, word| {
            let mut buffer = [0u8; 8];
            buffer.copy_from_slice(word);
            acc.wrapping_add(u64::from_le_bytes(buffer))
        })
    };
    Some(size.wrapping_add(sum(head)).wrapping_add(sum(tail)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ImdbId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn result(id: u64, language: &str) -> SubtitleResult {
        SubtitleResult {
            file_id: id,
            language: language.into(),
            release: String::new(),
            file_name: None,
            download_count: 0,
            hearing_impaired: false,
            foreign_parts_only: false,
            ai_translated: false,
            machine_translated: false,
            from_trusted: false,
            moviehash_match: false,
            format: None,
        }
    }
    fn ids(results: &[SubtitleResult]) -> Vec<u64> {
        results.iter().map(|r| r.file_id).collect()
    }
    fn langs(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn imdb_conversion_strips_prefix_and_zeros() -> TestResult {
        let movie = PlaybackTarget::Movie(ImdbId::new("tt0133093")?);
        assert_eq!(
            SubtitleTarget::from_playback(&movie),
            Some(SubtitleTarget::Movie { imdb: 133093 })
        );
        let episode = PlaybackTarget::Episode {
            series: ImdbId::new("tt0903747")?,
            season: 2,
            episode: 5,
        };
        assert_eq!(
            SubtitleTarget::try_from(&episode),
            Ok(SubtitleTarget::Episode {
                series_imdb: 903747,
                season: 2,
                episode: 5
            })
        );
        Ok(())
    }

    #[test]
    fn imdb_conversion_rejects_zero_and_overflow() -> TestResult {
        for id in ["tt0000000", "tt99999999999"] {
            let target = PlaybackTarget::Movie(ImdbId::new(id)?);
            assert_eq!(SubtitleTarget::from_playback(&target), None, "{id}");
        }
        Ok(())
    }

    #[test]
    fn ranking_orders_language_then_quality() {
        let mut hash = result(1, "en");
        hash.moviehash_match = true;
        let mut popular = result(2, "en");
        popular.download_count = 900;
        let mut trusted = result(3, "en");
        trusted.from_trusted = true;
        let mut ai = result(4, "en");
        ai.ai_translated = true;
        ai.moviehash_match = false;
        ai.download_count = 10_000;
        let fr = result(5, "fr");
        let de = result(6, "de");
        let input = vec![de, ai, popular, fr, trusted, hash];
        let ranked = rank_results(input, &langs(&["fr", "en"]));
        assert_eq!(ids(&ranked), vec![5, 1, 3, 2, 4, 6]);
    }

    #[test]
    fn ranking_is_stable_and_drops_machine_translations() {
        let mut machine = result(9, "en");
        machine.machine_translated = true;
        let ranked = rank_results(
            vec![result(1, "en"), machine, result(2, "en")],
            &langs(&["en"]),
        );
        assert_eq!(ids(&ranked), vec![1, 2]);
        assert!(rank_results(Vec::new(), &langs(&["en"])).is_empty());
    }

    #[test]
    fn ranking_keeps_unlisted_languages_grouped_in_first_seen_order() {
        let ranked = rank_results(
            vec![result(1, "de"), result(2, "es"), result(3, "de")],
            &langs(&["en"]),
        );
        assert_eq!(ids(&ranked), vec![1, 3, 2]);
    }

    fn synthetic(seed: u8) -> Vec<u8> {
        (0..MOVIE_HASH_CHUNK)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    #[test]
    fn movie_hash_matches_reference_loop() {
        let head = synthetic(3);
        let tail = synthetic(200);
        let size = 5_000_000_123u64;
        let mut expected = size;
        for bytes in [&head, &tail] {
            for i in 0..MOVIE_HASH_CHUNK / 8 {
                let mut word = 0u64;
                for b in 0..8 {
                    word |= u64::from(bytes[i * 8 + b]) << (8 * b);
                }
                expected = expected.wrapping_add(word);
            }
        }
        assert_eq!(movie_hash(size, &head, &tail), Some(expected));
    }

    #[test]
    fn movie_hash_of_zero_data_is_the_size_and_wraps() {
        let zeros = vec![0u8; MOVIE_HASH_CHUNK];
        let size = MOVIE_HASH_CHUNK as u64;
        assert_eq!(movie_hash(size, &zeros, &zeros), Some(size));
        let ones = vec![0xFFu8; MOVIE_HASH_CHUNK];
        // 2 * 8192 words of u64::MAX wrap to -16384; plus size 65536.
        assert_eq!(
            movie_hash(size, &ones, &ones),
            Some(size.wrapping_sub(2 * (MOVIE_HASH_CHUNK as u64 / 8)))
        );
    }

    #[test]
    fn movie_hash_rejects_short_files_and_wrong_slices() {
        let chunk = vec![1u8; MOVIE_HASH_CHUNK];
        assert_eq!(movie_hash(65_535, &chunk, &chunk), None);
        assert_eq!(movie_hash(1 << 20, &chunk[..100], &chunk), None);
        assert_eq!(movie_hash(1 << 20, &chunk, &[]), None);
    }
}
