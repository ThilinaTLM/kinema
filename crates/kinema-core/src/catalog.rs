//! Validated catalog values, independent of providers, storage, and presentation.

use std::{error::Error, fmt, sync::Arc};

use crate::MediaKind;

/// A validation failure for a catalog value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogValidationError {
    EmptyQuery,
    QueryTooLong,
    InvalidImdbId,
    EmptyTitleName,
}

impl fmt::Display for CatalogValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyQuery => "Enter a search query.",
            Self::QueryTooLong => "Search queries must contain at most 200 characters.",
            Self::InvalidImdbId => {
                "IMDb IDs must start with tt followed by at least seven ASCII digits."
            }
            Self::EmptyTitleName => "Catalog titles must have a nonempty name.",
        })
    }
}

impl Error for CatalogValidationError {}

/// A search identity: media kind and trimmed, otherwise unchanged query text.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SearchQuery {
    kind: MediaKind,
    text: String,
}

impl SearchQuery {
    /// Trims surrounding whitespace and accepts 1–200 Unicode scalar values.
    /// Case, internal whitespace, and Unicode normalization are not changed.
    pub fn new(kind: MediaKind, text: impl Into<String>) -> Result<Self, CatalogValidationError> {
        let text = text.into();
        let text = text.trim();
        if text.is_empty() {
            return Err(CatalogValidationError::EmptyQuery);
        }
        if text.chars().count() > 200 {
            return Err(CatalogValidationError::QueryTooLong);
        }
        Ok(Self {
            kind,
            text: text.to_owned(),
        })
    }

    pub fn kind(&self) -> MediaKind {
        self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// An IMDb title identifier: lowercase `tt` and at least seven ASCII digits.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImdbId(String);

impl ImdbId {
    /// Validates exactly as supplied, without trimming or case conversion.
    pub fn new(value: impl Into<String>) -> Result<Self, CatalogValidationError> {
        let value = value.into();
        let valid = value
            .strip_prefix("tt")
            .is_some_and(|digits| digits.len() >= 7 && digits.bytes().all(|b| b.is_ascii_digit()));
        if !valid {
            return Err(CatalogValidationError::InvalidImdbId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A genuine IMDb score, rounded to display tenths. Zero is not missing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImdbRating(u8);
impl ImdbRating {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && (0.0..=10.0).contains(&value))
            .then(|| Self((value * 10.0).round() as u8))
    }
    pub fn value(self) -> f32 {
        f32::from(self.0) / 10.0
    }
}

/// A credited performer; character is unknown when the provider supplies only names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CastMember {
    pub name: String,
    pub character: Option<String>,
}

/// Optional detail facts. Rating values use the validated tenths score container;
/// `tmdb_rating` is never an IMDb score despite sharing its value representation.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct TitleFacts {
    pub tmdb_rating: Option<ImdbRating>,
    pub tmdb_vote_count: Option<u64>,
    pub imdb_rating: Option<ImdbRating>,
    pub runtime_minutes: Option<u32>,
    pub status: Option<String>,
    pub release_date: Option<String>,
    pub first_air_date: Option<String>,
    pub last_air_date: Option<String>,
    pub certification: Option<String>,
    pub original_language: Option<String>,
    pub directors: Vec<String>,
    pub creators: Vec<String>,
    pub cast: Vec<CastMember>,
    pub studios: Vec<String>,
    pub networks: Vec<String>,
    pub tagline: Option<String>,
    pub writers: Vec<String>,
    pub countries: Vec<String>,
    pub release_info: Option<String>,
}
impl TitleFacts {
    /// Pure bounded validation, also applied when reconstructing cache payloads.
    pub fn is_valid(&self) -> bool {
        fn text(s: &str, max: usize) -> bool {
            !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
        }
        let strings = [
            &self.status,
            &self.certification,
            &self.original_language,
            &self.release_info,
        ];
        strings
            .into_iter()
            .all(|s| s.as_deref().is_none_or(|s| text(s, 200)))
            && self.tagline.as_deref().is_none_or(|s| text(s, 1000))
            && [
                &self.release_date,
                &self.first_air_date,
                &self.last_air_date,
            ]
            .into_iter()
            .all(|s| {
                s.as_deref()
                    .is_none_or(crate::catalog_normalized::valid_date)
            })
            && self
                .runtime_minutes
                .is_none_or(|n| (1..=10000).contains(&n))
            && self.tmdb_vote_count.is_none_or(|n| n <= i64::MAX as u64)
            && [
                &self.directors,
                &self.creators,
                &self.studios,
                &self.networks,
                &self.writers,
                &self.countries,
            ]
            .into_iter()
            .all(|v| v.len() <= 32 && v.iter().all(|s| text(s, 200)))
            && self.cast.len() <= 10
            && self
                .cast
                .iter()
                .all(|c| text(&c.name, 200) && c.character.as_deref().is_none_or(|s| text(s, 200)))
    }
}

/// Optional provider-supplied display facts; no inferred ratings or enrichment.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogDisplay {
    pub year: Option<u16>,
    pub poster_url: Option<String>,
    pub imdb_rating: Option<ImdbRating>,
    pub runtime_minutes: Option<u16>,
    pub genres: Vec<String>,
    pub season_count: Option<u32>,
}

/// A catalog title with a nonempty, trimmed name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogTitle {
    id: ImdbId,
    kind: MediaKind,
    name: String,
    overview: Option<String>,
    display: Arc<CatalogDisplay>,
}

impl CatalogTitle {
    /// Trims the name; preserves the optional overview exactly as supplied.
    pub fn new(
        id: ImdbId,
        kind: MediaKind,
        name: impl Into<String>,
        overview: Option<String>,
    ) -> Result<Self, CatalogValidationError> {
        let name = name.into();
        let name = name.trim();
        if name.is_empty() {
            return Err(CatalogValidationError::EmptyTitleName);
        }
        Ok(Self {
            id,
            kind,
            name: name.to_owned(),
            overview,
            display: Arc::default(),
        })
    }

    pub fn with_display(mut self, display: CatalogDisplay) -> Self {
        self.display = Arc::new(display);
        self
    }
    pub fn year(&self) -> Option<u16> {
        self.display.year
    }
    pub fn poster_url(&self) -> Option<&str> {
        self.display.poster_url.as_deref()
    }
    pub fn imdb_rating(&self) -> Option<f32> {
        self.display.imdb_rating.map(ImdbRating::value)
    }
    pub fn runtime_minutes(&self) -> Option<u16> {
        self.display.runtime_minutes
    }
    pub fn genres(&self) -> &[String] {
        &self.display.genres
    }
    pub fn season_count(&self) -> Option<u32> {
        self.display.season_count
    }

    pub fn id(&self) -> &ImdbId {
        &self.id
    }

    pub fn kind(&self) -> MediaKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn overview(&self) -> Option<&str> {
        self.overview.as_deref()
    }
}

/// One provider-supplied episode; season zero represents specials.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogEpisode {
    /// Provider-supplied episode still; never inferred from positional file data.
    pub still_url: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub season: u32,
    pub number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub released: Option<String>,
}

/// Display details without provider DTOs or transport concerns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TitleDetails {
    pub facts: TitleFacts,
    pub title: CatalogTitle,
    pub poster_url: Option<String>,
    pub background_url: Option<String>,
    pub release_info: Option<String>,
    pub genres: Vec<String>,
    pub episodes: Vec<CatalogEpisode>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn title_facts_validate_optional_bounded_values() {
        let mut facts = TitleFacts::default();
        assert!(facts.is_valid());
        facts.runtime_minutes = Some(0);
        assert!(!facts.is_valid());
        facts.runtime_minutes = Some(45);
        facts.release_date = Some("2024-02-30".into());
        assert!(!facts.is_valid());
        facts.release_date = Some("2024-02-29".into());
        assert!(facts.is_valid());
        facts.cast = (0..11)
            .map(|_| CastMember {
                name: "Actor".into(),
                character: None,
            })
            .collect();
        assert!(!facts.is_valid());
    }
    #[test]
    fn imdb_rating_preserves_zero_and_rejects_non_scores() {
        assert_eq!(ImdbRating::new(0.0).map(ImdbRating::value), Some(0.0));
        assert_eq!(ImdbRating::new(8.46).map(ImdbRating::value), Some(8.5));
        assert_eq!(ImdbRating::new(10.0).map(ImdbRating::value), Some(10.0));
        for value in [f64::NAN, f64::INFINITY, -0.1, 10.1] {
            assert_eq!(ImdbRating::new(value), None);
        }
    }
    #[test]
    fn query_trims_only_surrounding_whitespace() {
        let query = SearchQuery::new(MediaKind::Movie, "\u{2003} The  MATRIX \n")
            .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"));
        assert_eq!(query.text(), "The  MATRIX");
        assert_eq!(query.kind(), MediaKind::Movie);
        assert_eq!(
            SearchQuery::new(MediaKind::Movie, " \t\n\u{2003}"),
            Err(CatalogValidationError::EmptyQuery)
        );
        assert_eq!(
            SearchQuery::new(MediaKind::Movie, ""),
            Err(CatalogValidationError::EmptyQuery)
        );
    }

    #[test]
    fn query_length_counts_unicode_scalars_not_bytes() {
        let boundary = "映".repeat(200);
        assert_eq!(
            SearchQuery::new(MediaKind::Series, format!(" {boundary} "))
                .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"))
                .text(),
            boundary
        );
        for text in ["映".repeat(201), "a".repeat(201)] {
            assert_eq!(
                SearchQuery::new(MediaKind::Movie, text),
                Err(CatalogValidationError::QueryTooLong)
            );
        }
        // Combining marks count separately; no Unicode normalization is applied.
        assert!(SearchQuery::new(MediaKind::Movie, "e\u{301}".repeat(100)).is_ok());
        assert!(SearchQuery::new(MediaKind::Movie, "e\u{301}".repeat(101)).is_err());
    }

    #[test]
    fn query_identity_includes_kind_and_preserves_text() {
        let movie = SearchQuery::new(MediaKind::Movie, "Dark")
            .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"));
        let series = SearchQuery::new(MediaKind::Series, "Dark")
            .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"));
        assert_ne!(movie, series);
        let mut queries = HashSet::from([movie.clone(), series]);
        assert!(
            !queries.insert(
                SearchQuery::new(MediaKind::Movie, " Dark ")
                    .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"))
            )
        );
        assert!(
            queries.insert(
                SearchQuery::new(MediaKind::Movie, "dark")
                    .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"))
            )
        );
        assert_eq!(queries.len(), 3);
        assert_eq!(movie.clone(), movie);
    }

    #[test]
    fn imdb_ids_require_exact_prefix_and_ascii_digits() {
        for value in ["tt0000000", "tt1234567", "tt123456789012"] {
            let id = ImdbId::new(value)
                .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"));
            assert_eq!(id.as_str(), value);
            assert_eq!(HashSet::from([id.clone(), id]).len(), 1);
        }
        for value in [
            "",
            "tt",
            "tt123456",
            "1234567",
            "TT1234567",
            "nm1234567",
            " tt1234567",
            "tt1234567 ",
            "tt123456a",
            "tt1234567\n",
            "tt１２３４５６７",
            "tt١٢٣٤٥٦٧",
            "tt+1234567",
        ] {
            assert_eq!(
                ImdbId::new(value),
                Err(CatalogValidationError::InvalidImdbId),
                "unexpected acceptance of {value:?}"
            );
        }
    }

    #[test]
    fn title_validates_name_and_exposes_values() {
        let id = ImdbId::new("tt1234567")
            .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"));
        for name in ["", " \t\n\u{2003}"] {
            assert_eq!(
                CatalogTitle::new(id.clone(), MediaKind::Movie, name, None),
                Err(CatalogValidationError::EmptyTitleName)
            );
        }
        let title = CatalogTitle::new(
            id.clone(),
            MediaKind::Series,
            "  映画  ",
            Some(" Overview \n".to_owned()),
        )
        .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"));
        assert_eq!(title.id(), &id);
        assert_eq!(title.kind(), MediaKind::Series);
        assert_eq!(title.name(), "映画");
        assert_eq!(title.overview(), Some(" Overview \n"));
        assert_eq!(title.clone(), title);
        for overview in [None, Some(String::new())] {
            let title = CatalogTitle::new(id.clone(), MediaKind::Movie, "Name", overview.clone())
                .unwrap_or_else(|error| panic!("unexpected validation failure: {error}"));
            assert_eq!(title.overview(), overview.as_deref());
        }
    }
}
