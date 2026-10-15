//! Validated catalog values, independent of providers, storage, and presentation.

use std::{error::Error, fmt};

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

/// A catalog title with a nonempty, trimmed name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogTitle {
    id: ImdbId,
    kind: MediaKind,
    name: String,
    overview: Option<String>,
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
        })
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

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
