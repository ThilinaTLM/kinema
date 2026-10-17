//! Provider-independent current-value catalog rules. No transport or storage types.
use crate::MediaKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Quality {
    Summary,
    Detail,
}
impl Quality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Detail => "detail",
        }
    }
}

/// Missing values never clear retained metadata; equal quality accepts only a
/// non-older observation. Generation rejection remains the worker's responsibility.
pub fn accepts_field(current: Option<(Quality, i64)>, incoming: Quality, observed_at: i64) -> bool {
    current.is_none_or(|(quality, time)| {
        (incoming == Quality::Detail && quality == Quality::Summary)
            || (incoming == quality && observed_at >= time)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalIdentifier {
    pub namespace: String,
    pub external_id: String,
}
impl ExternalIdentifier {
    pub fn valid(&self) -> bool {
        match self.namespace.as_str() {
            "imdb" => super::catalog::ImdbId::new(self.external_id.clone()).is_ok(),
            "tmdb" => self
                .external_id
                .parse::<u64>()
                .is_ok_and(|id| id > 0 && id.to_string() == self.external_id),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RatingObservation {
    pub source: String,
    pub metric: String,
    pub supplied_by: String,
    pub value: Option<f64>,
    pub vote_count: Option<u64>,
    pub source_updated_at: Option<i64>,
}
impl RatingObservation {
    pub fn valid(&self) -> bool {
        let max = match (self.source.as_str(), self.metric.as_str()) {
            ("imdb" | "tmdb", "score") => 10.0,
            ("rotten_tomatoes", "critic_percent" | "audience_percent") => 100.0,
            _ => return false,
        };
        matches!(self.supplied_by.as_str(), "cinemeta" | "tmdb")
            && (self.value.is_some() || self.vote_count.is_some())
            && self
                .value
                .is_none_or(|value| value.is_finite() && (0.0..=max).contains(&value))
            && self
                .vote_count
                .is_none_or(|votes| i64::try_from(votes).is_ok())
            && self.source_updated_at.is_none_or(|time| time >= 0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldValue {
    Text(String),
    Integer(i64),
    Labels(Vec<String>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct TitleObservation {
    pub kind: MediaKind,
    pub identifiers: Vec<ExternalIdentifier>,
    pub fields: Vec<(String, FieldValue)>,
    pub ratings: Vec<RatingObservation>,
}

/// Current origin of a retained non-NULL descriptive value, not history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldOrigin {
    pub field_name: String,
    pub supplied_by: String,
    pub quality: Quality,
    pub fetch_state_id: i64,
    pub observed_at: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedRating {
    pub observation: RatingObservation,
    pub fetch_state_id: i64,
    pub observed_at: i64,
    pub updated_at: i64,
}
/// Rich canonical title read model. Missing fields remain absent; origins and
/// supplier-scoped ratings are exposed independently from provider success state.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalTitle {
    pub id: i64,
    pub kind: MediaKind,
    pub identifiers: Vec<ExternalIdentifier>,
    pub fields: std::collections::BTreeMap<String, FieldValue>,
    pub provenance: Vec<FieldOrigin>,
    pub ratings: Vec<RetainedRating>,
    pub updated_at: i64,
}
impl CanonicalTitle {
    pub fn text(&self, field: &str) -> Option<&str> {
        match self.fields.get(field) {
            Some(FieldValue::Text(value)) => Some(value),
            _ => None,
        }
    }
    pub fn integer(&self, field: &str) -> Option<i64> {
        match self.fields.get(field) {
            Some(FieldValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildKind {
    Season,
    Episode,
}
/// Rich child metadata is separate from a list response's completeness. A child
/// may have real coordinates and identifiers while optional details are unknown.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalChild {
    pub id: i64,
    pub parent_id: i64,
    pub number: u32,
    pub kind: ChildKind,
    pub fields: std::collections::BTreeMap<String, FieldValue>,
    pub identifiers: Vec<ExternalIdentifier>,
    pub provenance: Vec<FieldOrigin>,
    pub ratings: Vec<RetainedRating>,
    pub updated_at: i64,
}
impl CanonicalChild {
    pub fn text(&self, field: &str) -> Option<&str> {
        match self.fields.get(field) {
            Some(FieldValue::Text(value)) => Some(value),
            _ => None,
        }
    }
    pub fn integer(&self, field: &str) -> Option<i64> {
        match self.fields.get(field) {
            Some(FieldValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }
}

/// Strict ISO calendar day. A bare year remains a year, never January 1.
pub fn valid_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = text[..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = text[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = text[8..].parse::<u32>() else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=max).contains(&day)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merge_quality_and_time() {
        assert!(!accepts_field(
            Some((Quality::Detail, 1)),
            Quality::Summary,
            100
        ));
        assert!(accepts_field(
            Some((Quality::Summary, 100)),
            Quality::Detail,
            101
        ));
        assert!(!accepts_field(
            Some((Quality::Detail, 100)),
            Quality::Detail,
            99
        ));
    }
    #[test]
    fn rating_scales_and_unknown_votes() {
        let mut rating = RatingObservation {
            source: "imdb".into(),
            metric: "score".into(),
            supplied_by: "cinemeta".into(),
            value: Some(0.0),
            vote_count: None,
            source_updated_at: None,
        };
        assert!(rating.valid());
        rating.value = Some(11.0);
        assert!(!rating.valid());
        rating.value = Some(f64::NAN);
        assert!(!rating.valid());
        rating.value = None;
        assert!(!rating.valid());
        rating.vote_count = Some(0);
        assert!(rating.valid());
    }
    #[test]
    fn real_dates_only() {
        assert!(valid_date("2024-02-29"));
        assert!(!valid_date("2023-02-29"));
        assert!(!valid_date("2024"));
        assert!(!valid_date("0000-01-01"));
    }
}
