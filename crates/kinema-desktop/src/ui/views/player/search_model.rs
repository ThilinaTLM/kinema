//! Online subtitle search sheet: pure state, copy and query rules.
//!
//! No GPUI, network or filesystem access lives here (see `search.rs` for the
//! window glue). Every outcome of a search or download maps to one explicit
//! state with short copy, so a failure is never shown as "no results".
use crate::providers::opensubtitles::OpenSubtitlesError;
use kinema_core::{
    sources::PlaybackTarget,
    subtitle_search::{SubtitleQuery, SubtitleResult, SubtitleTarget},
    tracks::language_name,
};

/// Source name shown in the dimmed track column of downloaded subtitles.
pub(super) const ONLINE_SOURCE: &str = "OpenSubtitles";
/// Results kept for display; the service pages beyond this are never fetched.
pub(super) const MAX_DISPLAYED: usize = 50;
/// Longest typed query sent to the service, in characters.
pub(super) const MAX_QUERY_CHARS: usize = 120;
/// mpv's limit for a `sub-add` title, in bytes.
const MAX_TITLE_BYTES: usize = 256;
/// mpv's limit for a `sub-add` language tag, in bytes.
const MAX_LANG_BYTES: usize = 16;

/// Why a search or download did not produce a subtitle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Problem {
    /// No API key is saved.
    NoApiKey,
    /// Secure storage could not be read.
    KeyStorage,
    Service,
    RateLimited,
    QuotaExhausted {
        reset_time_utc: Option<String>,
    },
    ApiKeyRejected,
    SignInRequired,
    LoginRejected,
    Offline,
    /// The downloaded file could not be stored or loaded.
    Unusable,
}

/// What the problem panel offers besides the copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProblemAction {
    Retry,
    OpenSettings,
    None,
}

impl Problem {
    /// `None` for a cancelled request: the owner dropped it on purpose.
    pub(super) fn from_error(error: &OpenSubtitlesError) -> Option<Self> {
        Some(match error {
            OpenSubtitlesError::Cancelled => return None,
            OpenSubtitlesError::Network => Self::Offline,
            OpenSubtitlesError::RateLimited => Self::RateLimited,
            OpenSubtitlesError::QuotaExhausted { reset_time_utc } => Self::QuotaExhausted {
                reset_time_utc: reset_time_utc.clone(),
            },
            OpenSubtitlesError::ApiKeyRejected => Self::ApiKeyRejected,
            OpenSubtitlesError::SignInRequired => Self::SignInRequired,
            OpenSubtitlesError::LoginRejected => Self::LoginRejected,
            // An expired session is retried once by the worker; reaching here
            // means it expired again, which the user cannot act on.
            OpenSubtitlesError::SessionExpired
            | OpenSubtitlesError::Configuration
            | OpenSubtitlesError::ServiceFailure
            | OpenSubtitlesError::InvalidResponse => Self::Service,
        })
    }

    /// Short heading.
    pub(super) fn title(&self) -> &'static str {
        match self {
            Self::NoApiKey => "API key needed",
            Self::KeyStorage => "Can't read saved key",
            Self::Service => "Search unavailable",
            Self::RateLimited => "Too many requests",
            Self::QuotaExhausted { .. } => "Download limit reached",
            Self::ApiKeyRejected => "API key rejected",
            Self::SignInRequired => "Sign in required",
            Self::LoginRejected => "Sign-in rejected",
            Self::Offline => "You're offline",
            Self::Unusable => "Subtitle failed",
        }
    }

    /// One or two plain sentences.
    pub(super) fn detail(&self) -> String {
        match self {
            Self::NoApiKey => "Add an OpenSubtitles API key in Settings → Accounts.".into(),
            Self::KeyStorage => {
                "Secure storage is unavailable. Restore access, then try again.".into()
            }
            Self::Service => "OpenSubtitles had a problem. Try again in a moment.".into(),
            Self::RateLimited => "Wait a few seconds, then try again.".into(),
            Self::QuotaExhausted { reset_time_utc } => {
                let reset = reset_time_utc.as_deref().and_then(reset_label);
                match reset {
                    Some(reset) => format!(
                        "No downloads left today. {reset}. Signing in under Settings → Accounts raises the limit."
                    ),
                    None => "No downloads left today. Signing in under Settings → Accounts raises the limit."
                        .into(),
                }
            }
            Self::ApiKeyRejected => "Check the API key in Settings → Accounts.".into(),
            Self::SignInRequired => {
                "Add your OpenSubtitles login in Settings → Accounts, or allow anonymous downloads for your key."
                    .into()
            }
            Self::LoginRejected => {
                "Check your OpenSubtitles username and password in Settings → Accounts.".into()
            }
            Self::Offline => "Check your connection, then try again.".into(),
            Self::Unusable => "The file could not be loaded. Try another one.".into(),
        }
    }

    pub(super) fn action(&self) -> ProblemAction {
        match self {
            Self::NoApiKey | Self::ApiKeyRejected | Self::SignInRequired | Self::LoginRejected => {
                ProblemAction::OpenSettings
            }
            Self::KeyStorage
            | Self::Service
            | Self::RateLimited
            | Self::Offline
            | Self::Unusable => ProblemAction::Retry,
            Self::QuotaExhausted { .. } => ProblemAction::None,
        }
    }

    /// Logging category; never carries a query, title, key or link.
    pub(super) fn category(&self) -> &'static str {
        match self {
            Self::NoApiKey => "no_api_key",
            Self::KeyStorage => "key_storage",
            Self::Service => "service",
            Self::RateLimited => "rate_limited",
            Self::QuotaExhausted { .. } => "quota_exhausted",
            Self::ApiKeyRejected => "api_key_rejected",
            Self::SignInRequired => "sign_in_required",
            Self::LoginRejected => "login_rejected",
            Self::Offline => "offline",
            Self::Unusable => "unusable",
        }
    }
}

/// "Resets at 00:00 UTC" from an ISO-8601 stamp such as `2026-11-08T00:00:00.000Z`.
/// `None` when the stamp has no readable time.
pub(super) fn reset_label(stamp: &str) -> Option<String> {
    let (_, time) = stamp.split_once(['T', ' '])?;
    let mut parts = time.split(':');
    let hour: u8 = parts.next()?.parse().ok()?;
    let minute: u8 = parts
        .next()?
        .get(..2)
        .and_then(|m| m.parse().ok())
        .filter(|m| *m < 60)?;
    (hour < 24).then(|| format!("Resets at {hour:02}:{minute:02} UTC"))
}

/// Downloads left, as last reported by the service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Allowance {
    pub remaining: u32,
    pub reset_time_utc: Option<String>,
}

impl Allowance {
    pub(super) fn label(&self) -> String {
        let count = match self.remaining {
            0 => "No downloads left".to_owned(),
            1 => "1 download left".to_owned(),
            n => format!("{n} downloads left"),
        };
        match self.reset_time_utc.as_deref().and_then(reset_label) {
            Some(reset) if self.remaining == 0 => format!("{count} · {reset}"),
            _ => count,
        }
    }
}

/// What the results area shows.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Phase {
    /// Waiting for a typed query (media without a catalog identity).
    Idle,
    Loading,
    /// Never empty; an empty success is [`Phase::Empty`].
    Results(Vec<SubtitleResult>),
    Empty,
    Failed(Problem),
}

/// Result of a finished search, before it reaches the model.
pub(super) type SearchOutcome = Result<Vec<SubtitleResult>, Problem>;

/// Sheet state with generation-checked transitions.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SearchModel {
    generation: u64,
    pub phase: Phase,
    /// Languages offered by the filter, in the user's order.
    pub languages: Vec<String>,
    /// `None` searches every language in [`Self::languages`].
    pub selected: Option<String>,
    /// File id being downloaded and loaded; one at a time.
    pub adding: Option<u64>,
    /// Why the last choose failed; the results stay available.
    pub add_problem: Option<Problem>,
}

impl SearchModel {
    pub(super) fn new(languages: Vec<String>, searchable: bool) -> Self {
        Self {
            generation: 0,
            phase: if searchable {
                Phase::Loading
            } else {
                Phase::Idle
            },
            languages,
            selected: None,
            adding: None,
            add_problem: None,
        }
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// Languages a search sends right now.
    pub(super) fn query_languages(&self) -> Vec<String> {
        match &self.selected {
            Some(code) => vec![code.clone()],
            None => self.languages.clone(),
        }
    }

    /// True when a search may start: not already loading and not adding.
    pub(super) fn can_search(&self) -> bool {
        self.phase != Phase::Loading && self.adding.is_none()
    }

    /// Starts a search and returns its generation; `None` while one is in flight
    /// or a download is being added.
    pub(super) fn begin_search(&mut self) -> Option<u64> {
        if !self.can_search() {
            return None;
        }
        self.generation += 1;
        self.phase = Phase::Loading;
        self.add_problem = None;
        Some(self.generation)
    }

    /// Applies a finished search. A stale generation is dropped (`false`).
    pub(super) fn finish_search(&mut self, generation: u64, outcome: SearchOutcome) -> bool {
        if generation != self.generation || self.phase != Phase::Loading {
            return false;
        }
        self.phase = match outcome {
            Ok(mut results) => {
                results.truncate(MAX_DISPLAYED);
                if results.is_empty() {
                    Phase::Empty
                } else {
                    Phase::Results(results)
                }
            }
            Err(problem) => Phase::Failed(problem),
        };
        true
    }

    /// Picks the language filter (`None` = all) and reports whether it changed.
    pub(super) fn select_language(&mut self, code: Option<String>) -> bool {
        let code = code.filter(|c| self.languages.contains(c));
        if code == self.selected {
            return false;
        }
        self.selected = code;
        true
    }

    /// Starts adding a result and returns the generation to complete with.
    /// Rejected while another add is running or the id is not listed.
    pub(super) fn begin_add(&mut self, file_id: u64) -> Option<u64> {
        let Phase::Results(results) = &self.phase else {
            return None;
        };
        if self.adding.is_some() || !results.iter().any(|r| r.file_id == file_id) {
            return None;
        }
        self.generation += 1;
        self.adding = Some(file_id);
        self.add_problem = None;
        Some(self.generation)
    }

    /// Records a failed download or load. Stale generations are dropped.
    pub(super) fn fail_add(&mut self, generation: u64, problem: Problem) -> bool {
        if generation != self.generation || self.adding.is_none() {
            return false;
        }
        self.adding = None;
        self.add_problem = Some(problem);
        true
    }

    /// The player loaded or refused the file. Returns whether the sheet closes.
    pub(super) fn finish_add(&mut self, generation: u64, added: bool) -> Option<bool> {
        if generation != self.generation || self.adding.is_none() {
            return None;
        }
        self.adding = None;
        if !added {
            self.add_problem = Some(Problem::Unusable);
        }
        Some(added)
    }

    /// The media reloaded under the sheet: an in-flight add can no longer land.
    pub(super) fn abandon_add(&mut self) {
        if self.adding.take().is_some() {
            self.generation += 1;
        }
    }
}

/// Filter segment labels: names while few, codes when the row would overflow.
pub(super) fn filter_labels(languages: &[String]) -> Vec<(Option<String>, String)> {
    let names = languages.len() <= 3;
    let mut labels = vec![(None, "All".to_owned())];
    labels.extend(languages.iter().map(|code| {
        let label = if names {
            language_name(code).unwrap_or_else(|| code.to_ascii_uppercase())
        } else {
            code.to_ascii_uppercase()
        };
        (Some(code.clone()), label)
    }));
    labels
}

/// Whether the catalog identity is usable for a search by id.
pub(super) fn searchable_by_id(target: Option<&PlaybackTarget>) -> bool {
    target.and_then(SubtitleTarget::from_playback).is_some()
}

/// Builds the request. Identity comes only from the catalog target; typed text
/// is used when there is none. `None` when there is nothing to search for.
pub(super) fn build_query(
    target: Option<&PlaybackTarget>,
    text: &str,
    languages: Vec<String>,
    moviehash: Option<u64>,
) -> Option<SubtitleQuery> {
    let target = target.and_then(SubtitleTarget::from_playback);
    let text = if target.is_none() {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text: String = text.chars().take(MAX_QUERY_CHARS).collect();
        if text.is_empty() {
            return None;
        }
        Some(text)
    } else {
        None
    };
    Some(SubtitleQuery {
        target,
        text,
        languages,
        moviehash,
    })
}

/// mpv `sub-add` title: the release name without control characters, bounded.
pub(super) fn subtitle_title(result: &SubtitleResult) -> String {
    let clean = |text: &str| {
        let mut out = String::new();
        for c in text.chars() {
            let c = if c.is_control() { ' ' } else { c };
            if out.len() + c.len_utf8() > MAX_TITLE_BYTES {
                break;
            }
            out.push(c);
        }
        out.trim().to_owned()
    };
    [Some(result.release.as_str()), result.file_name.as_deref()]
        .into_iter()
        .flatten()
        .map(clean)
        .find(|t| !t.is_empty())
        .unwrap_or_else(|| "Subtitle".to_owned())
}

/// mpv `sub-add` language tag, or `None` when the service sent something odd.
pub(super) fn subtitle_lang(result: &SubtitleResult) -> Option<&str> {
    let lang = result.language.trim();
    (!lang.is_empty()
        && lang.len() <= MAX_LANG_BYTES
        && lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    .then_some(lang)
}

/// "English · 12,345 downloads" for the row's second line.
pub(super) fn result_detail(result: &SubtitleResult) -> String {
    let language = language_name(&result.language).unwrap_or_else(|| "Unknown language".into());
    match result.download_count {
        0 => language,
        1 => format!("{language} · 1 download"),
        n => format!("{language} · {} downloads", thousands(n)),
    }
}

/// Short badge labels in display order.
pub(super) fn result_badges(result: &SubtitleResult) -> Vec<&'static str> {
    let mut badges = Vec::new();
    if result.moviehash_match {
        badges.push("Hash match");
    }
    if result.hearing_impaired {
        badges.push("HI/SDH");
    }
    if result.foreign_parts_only {
        badges.push("Forced");
    }
    if result.ai_translated {
        badges.push("AI translated");
    }
    badges
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinema_core::catalog::ImdbId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn result(id: u64, language: &str) -> SubtitleResult {
        SubtitleResult {
            file_id: id,
            language: language.into(),
            release: format!("Release {id}"),
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
    fn langs(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|c| (*c).to_owned()).collect()
    }

    #[test]
    fn every_service_outcome_maps_to_one_state_with_copy() {
        use OpenSubtitlesError as E;
        let cases = [
            (E::Network, Problem::Offline, ProblemAction::Retry),
            (E::RateLimited, Problem::RateLimited, ProblemAction::Retry),
            (E::ServiceFailure, Problem::Service, ProblemAction::Retry),
            (E::InvalidResponse, Problem::Service, ProblemAction::Retry),
            (E::Configuration, Problem::Service, ProblemAction::Retry),
            (E::SessionExpired, Problem::Service, ProblemAction::Retry),
            (
                E::ApiKeyRejected,
                Problem::ApiKeyRejected,
                ProblemAction::OpenSettings,
            ),
            (
                E::SignInRequired,
                Problem::SignInRequired,
                ProblemAction::OpenSettings,
            ),
            (
                E::LoginRejected,
                Problem::LoginRejected,
                ProblemAction::OpenSettings,
            ),
            (
                E::QuotaExhausted {
                    reset_time_utc: Some("2026-11-08T00:00:00.000Z".into()),
                },
                Problem::QuotaExhausted {
                    reset_time_utc: Some("2026-11-08T00:00:00.000Z".into()),
                },
                ProblemAction::None,
            ),
        ];
        let mut titles = Vec::new();
        for (error, problem, action) in cases {
            assert_eq!(Problem::from_error(&error), Some(problem.clone()));
            assert_eq!(problem.action(), action);
            assert!(!problem.title().is_empty() && !problem.title().ends_with('…'));
            assert!(!problem.detail().is_empty());
            titles.push(problem.title());
        }
        assert_eq!(Problem::from_error(&E::Cancelled), None);
        // Distinct failures read differently (Service/Offline/RateLimited/…).
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), 7, "{titles:?}");
        for problem in [Problem::NoApiKey, Problem::KeyStorage, Problem::Unusable] {
            assert!(!problem.title().is_empty() && !problem.detail().is_empty());
        }
        assert_eq!(Problem::NoApiKey.action(), ProblemAction::OpenSettings);
        assert!(Problem::NoApiKey.detail().contains("Settings → Accounts"));
    }

    #[test]
    fn quota_copy_names_the_reset_time_only_when_readable() {
        let quota = |stamp: Option<&str>| Problem::QuotaExhausted {
            reset_time_utc: stamp.map(Into::into),
        };
        assert!(
            quota(Some("2026-11-08T00:00:00.000Z"))
                .detail()
                .contains("Resets at 00:00 UTC")
        );
        assert!(!quota(Some("garbage")).detail().contains("Resets"));
        assert!(!quota(None).detail().contains("Resets"));
        assert_eq!(
            reset_label("2026-11-08 13:05:00"),
            Some("Resets at 13:05 UTC".into())
        );
        assert_eq!(reset_label("2026-11-08T25:00:00Z"), None);
        assert_eq!(reset_label("2026-11-08T12:99:00Z"), None);
        assert_eq!(reset_label(""), None);
    }

    #[test]
    fn allowance_label_counts_and_shows_reset_when_empty() {
        let allowance = |remaining, stamp: Option<&str>| Allowance {
            remaining,
            reset_time_utc: stamp.map(Into::into),
        };
        assert_eq!(allowance(7, None).label(), "7 downloads left");
        assert_eq!(allowance(1, None).label(), "1 download left");
        assert_eq!(
            allowance(0, Some("2026-11-08T00:00:00Z")).label(),
            "No downloads left · Resets at 00:00 UTC"
        );
        assert_eq!(
            allowance(5, Some("2026-11-08T00:00:00Z")).label(),
            "5 downloads left"
        );
    }

    #[test]
    fn search_transitions_and_stale_results_are_rejected() {
        let mut model = SearchModel::new(langs(&["en", "fr"]), true);
        assert_eq!(model.phase, Phase::Loading);
        // A search is already running: a second request is refused.
        assert_eq!(model.begin_search(), None);
        assert!(model.finish_search(0, Ok(vec![result(1, "en")])));
        assert!(matches!(model.phase, Phase::Results(_)));
        let first = model.begin_search().unwrap_or_default();
        assert_eq!(model.phase, Phase::Loading);
        // Result for an older generation after the sheet moved on.
        assert!(!model.finish_search(first - 1, Ok(vec![])));
        assert_eq!(model.phase, Phase::Loading);
        assert!(model.finish_search(first, Ok(vec![])));
        assert_eq!(model.phase, Phase::Empty);
        // A late duplicate of an applied result is stale too.
        assert!(!model.finish_search(first, Ok(vec![result(2, "en")])));
        assert_eq!(model.phase, Phase::Empty);
        let second = model.begin_search().unwrap_or_default();
        assert!(second > first);
        assert!(model.finish_search(second, Err(Problem::Offline)));
        assert_eq!(model.phase, Phase::Failed(Problem::Offline));
    }

    #[test]
    fn empty_success_differs_from_failure_and_results_are_capped() {
        let mut model = SearchModel::new(langs(&["en"]), true);
        let many: Vec<_> = (0..(MAX_DISPLAYED as u64 + 20))
            .map(|id| result(id, "en"))
            .collect();
        assert!(model.finish_search(0, Ok(many)));
        assert!(matches!(&model.phase, Phase::Results(r) if r.len() == MAX_DISPLAYED));
        let generation = model.begin_search().unwrap_or_default();
        assert!(model.finish_search(generation, Ok(vec![])));
        assert_eq!(model.phase, Phase::Empty);
        assert_ne!(model.phase, Phase::Failed(Problem::Service));
    }

    #[test]
    fn text_mode_waits_for_a_query() {
        let mut model = SearchModel::new(langs(&["en"]), false);
        assert_eq!(model.phase, Phase::Idle);
        assert_eq!(model.begin_search(), Some(1));
        assert_eq!(model.phase, Phase::Loading);
    }

    #[test]
    fn language_filter_limits_the_query_and_ignores_unknown_codes() {
        let mut model = SearchModel::new(langs(&["en", "fr"]), true);
        assert_eq!(model.query_languages(), langs(&["en", "fr"]));
        assert!(model.select_language(Some("fr".into())));
        assert_eq!(model.query_languages(), langs(&["fr"]));
        assert!(!model.select_language(Some("fr".into())));
        assert!(model.select_language(Some("de".into())));
        assert_eq!(model.selected, None);
        assert_eq!(model.query_languages(), langs(&["en", "fr"]));
        let labels = filter_labels(&langs(&["en", "fr"]));
        assert_eq!(
            labels,
            vec![
                (None, "All".into()),
                (Some("en".into()), "English".into()),
                (Some("fr".into()), "French".into())
            ]
        );
        let many = filter_labels(&langs(&["en", "fr", "de", "es"]));
        assert_eq!(many[1].1, "EN");
    }

    #[test]
    fn add_is_exclusive_stale_safe_and_distinguishes_failure() {
        let mut model = SearchModel::new(langs(&["en"]), true);
        assert!(model.finish_search(0, Ok(vec![result(1, "en"), result(2, "en")])));
        assert_eq!(model.begin_add(99), None, "unlisted result");
        let add = model.begin_add(1).unwrap_or_default();
        assert_eq!(model.adding, Some(1));
        assert_eq!(model.begin_add(2), None, "one add at a time");
        assert_eq!(model.begin_search(), None, "no search while adding");
        assert!(!model.fail_add(add - 1, Problem::Offline));
        assert_eq!(model.adding, Some(1));
        assert!(model.fail_add(add, Problem::RateLimited));
        assert_eq!(model.adding, None);
        assert_eq!(model.add_problem, Some(Problem::RateLimited));
        assert!(matches!(model.phase, Phase::Results(_)), "results stay");
        // Retry and succeed.
        let add = model.begin_add(1).unwrap_or_default();
        assert_eq!(model.add_problem, None);
        assert_eq!(model.finish_add(add + 5, true), None);
        assert_eq!(model.finish_add(add, true), Some(true));
        assert_eq!(model.adding, None);
        // Player refused the file.
        let add = model.begin_add(2).unwrap_or_default();
        assert_eq!(model.finish_add(add, false), Some(false));
        assert_eq!(model.add_problem, Some(Problem::Unusable));
        // A reload under an in-flight add makes its completion stale.
        let add = model.begin_add(2).unwrap_or_default();
        model.abandon_add();
        assert_eq!(model.adding, None);
        assert_eq!(model.finish_add(add, true), None);
    }

    #[test]
    fn query_uses_catalog_identity_text_languages_and_hash() -> TestResult {
        let movie = PlaybackTarget::Movie(ImdbId::new("tt0133093")?);
        let episode = PlaybackTarget::Episode {
            series: ImdbId::new("tt0903747")?,
            season: 2,
            episode: 5,
        };
        let query = build_query(Some(&movie), "ignored", langs(&["en", "fr"]), Some(7));
        assert_eq!(
            query,
            Some(SubtitleQuery {
                target: Some(SubtitleTarget::Movie { imdb: 133093 }),
                text: None,
                languages: langs(&["en", "fr"]),
                moviehash: Some(7),
            })
        );
        let query = build_query(Some(&episode), "", langs(&["en"]), None);
        assert_eq!(
            query.and_then(|q| q.target),
            Some(SubtitleTarget::Episode {
                series_imdb: 903747,
                season: 2,
                episode: 5
            })
        );
        // No identity: typed text only, whitespace collapsed and bounded.
        let query = build_query(None, "  Blade   Runner\t2049 ", langs(&["en"]), None);
        assert_eq!(
            query,
            Some(SubtitleQuery {
                target: None,
                text: Some("Blade Runner 2049".into()),
                languages: langs(&["en"]),
                moviehash: None,
            })
        );
        assert_eq!(build_query(None, "   ", langs(&["en"]), None), None);
        let long = "x".repeat(MAX_QUERY_CHARS * 2);
        let text = build_query(None, &long, langs(&["en"]), None).and_then(|q| q.text);
        assert_eq!(text.map(|t| t.chars().count()), Some(MAX_QUERY_CHARS));
        assert!(searchable_by_id(Some(&movie)));
        assert!(!searchable_by_id(None));
        Ok(())
    }

    #[test]
    fn mpv_title_and_language_are_sanitized() {
        let mut r = result(1, "pt-BR");
        r.release = "  Movie.2020.1080p\n.WEB-DL  ".into();
        assert_eq!(subtitle_title(&r), "Movie.2020.1080p .WEB-DL");
        assert_eq!(subtitle_lang(&r), Some("pt-BR"));
        r.release = "  ".into();
        r.file_name = Some("movie.srt".into());
        assert_eq!(subtitle_title(&r), "movie.srt");
        r.file_name = None;
        assert_eq!(subtitle_title(&r), "Subtitle");
        r.release = "é".repeat(400);
        assert!(subtitle_title(&r).len() <= MAX_TITLE_BYTES);
        r.language = "en us".into();
        assert_eq!(subtitle_lang(&r), None);
        r.language = "x".repeat(40);
        assert_eq!(subtitle_lang(&r), None);
        r.language = String::new();
        assert_eq!(subtitle_lang(&r), None);
    }

    #[test]
    fn rows_show_language_downloads_and_badges() {
        let mut r = result(1, "en");
        assert_eq!(result_detail(&r), "English");
        r.download_count = 1;
        assert_eq!(result_detail(&r), "English · 1 download");
        r.download_count = 1_234_567;
        assert_eq!(result_detail(&r), "English · 1,234,567 downloads");
        assert!(result_badges(&r).is_empty());
        r.moviehash_match = true;
        r.hearing_impaired = true;
        r.foreign_parts_only = true;
        r.ai_translated = true;
        assert_eq!(
            result_badges(&r),
            ["Hash match", "HI/SDH", "Forced", "AI translated"]
        );
        r.language = "zz".into();
        assert_eq!(result_detail(&r), "ZZ · 1,234,567 downloads");
    }
}
