//! Pure byte availability and single HTTP byte-range resolution.

use std::{error::Error, fmt};

/// Nonempty half-open byte interval `[start, end)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    pub const fn new(start: u64, end: u64) -> Option<Self> {
        if start < end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    pub const fn start(self) -> u64 {
        self.start
    }
    pub const fn end(self) -> u64 {
        self.end
    }
    pub const fn len(self) -> u64 {
        self.end - self.start
    }
    pub const fn is_empty(self) -> bool {
        false
    }
}

/// Sorted, nonoverlapping, nonadjacent intervals of available bytes.
/// No file size is assumed; the owner clips insertions to its media identity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RangeSet {
    ranges: Vec<ByteRange>,
}

impl RangeSet {
    pub const fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    pub fn ranges(&self) -> &[ByteRange] {
        &self.ranges
    }

    /// Union with an interval; adjacency merges without endpoint arithmetic.
    pub fn insert(&mut self, mut range: ByteRange) {
        let first = self.ranges.partition_point(|item| item.end < range.start);
        let mut last = first;
        while let Some(item) = self.ranges.get(last) {
            if item.start > range.end {
                break;
            }
            range.start = range.start.min(item.start);
            range.end = range.end.max(item.end);
            last += 1;
        }
        self.ranges.splice(first..last, [range]);
    }

    pub fn contains(&self, range: ByteRange) -> bool {
        let index = self.ranges.partition_point(|item| item.end <= range.start);
        self.ranges
            .get(index)
            .is_some_and(|item| item.start <= range.start && item.end >= range.end)
    }

    /// Missing pieces in ascending order, clipped to the requested interval.
    pub fn missing(&self, range: ByteRange) -> Vec<ByteRange> {
        let mut missing = Vec::new();
        let mut cursor = range.start;
        for item in &self.ranges {
            if item.end <= cursor {
                continue;
            }
            if item.start >= range.end {
                break;
            }
            if cursor < item.start {
                missing.push(ByteRange {
                    start: cursor,
                    end: item.start,
                });
            }
            cursor = cursor.max(item.end).min(range.end);
            if cursor == range.end {
                break;
            }
        }
        if cursor < range.end {
            missing.push(ByteRange {
                start: cursor,
                end: range.end,
            });
        }
        missing
    }

    /// Available pieces clipped to a requested interval.
    pub fn intersection(&self, range: ByteRange) -> Self {
        Self {
            ranges: self
                .ranges
                .iter()
                .filter_map(|item| {
                    ByteRange::new(item.start.max(range.start), item.end.min(range.end))
                })
                .collect(),
        }
    }

    /// The normalized union lies within `[0, u64::MAX)`, so its sum cannot overflow.
    pub fn total_bytes(&self) -> u64 {
        self.ranges.iter().map(|range| range.len()).sum()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeError {
    Invalid,
    Unsatisfiable,
    MultipleRangesUnsupported,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "The byte range is invalid.",
            Self::Unsatisfiable => "The byte range is not satisfiable for this file.",
            Self::MultipleRangesUnsupported => "Multiple byte ranges are not supported.",
        })
    }
}

impl Error for RangeError {}

/// Resolves one explicit HTTP Range header to a nonempty half-open interval.
/// Supports `bytes=start-end`, `bytes=start-`, and `bytes=-length`; inclusive
/// ends and suffix lengths are clamped to file size. Empty files and zero
/// suffixes are unsatisfiable. Decimal overflow, reversed bounds, unknown units,
/// signs, and interior whitespace are invalid. Comma-separated ranges are
/// deliberately unsupported. An absent Range header is handled by the caller.
pub fn parse_http_range(header: &str, size: u64) -> Result<ByteRange, RangeError> {
    let (unit, spec) = header
        .trim_matches([' ', '\t'])
        .split_once('=')
        .ok_or(RangeError::Invalid)?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return Err(RangeError::Invalid);
    }
    if spec.contains(',') {
        return Err(RangeError::MultipleRangesUnsupported);
    }
    let (start, end) = spec.split_once('-').ok_or(RangeError::Invalid)?;
    if start.is_empty() {
        let length = decimal(end)?;
        if size == 0 || length == 0 {
            return Err(RangeError::Unsatisfiable);
        }
        return Ok(ByteRange {
            start: size.saturating_sub(length),
            end: size,
        });
    }
    let start = decimal(start)?;
    let end = if end.is_empty() {
        None
    } else {
        Some(decimal(end)?)
    };
    if end.is_some_and(|end| end < start) {
        return Err(RangeError::Invalid);
    }
    if start >= size {
        return Err(RangeError::Unsatisfiable);
    }
    // Clamp the inclusive endpoint before adding one, including u64::MAX.
    let end = end.map_or(size, |end| end.min(size - 1) + 1);
    Ok(ByteRange { start, end })
}

fn decimal(text: &str) -> Result<u64, RangeError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RangeError::Invalid);
    }
    text.parse().map_err(|_| RangeError::Invalid)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn r(start: u64, end: u64) -> ByteRange {
        ByteRange::new(start, end).expect("nonempty range")
    }

    #[test]
    fn ranges_are_nonempty_half_open_and_allow_max_endpoint() {
        assert_eq!(ByteRange::new(0, 0), None);
        assert_eq!(ByteRange::new(3, 2), None);
        assert_eq!(ByteRange::new(u64::MAX, u64::MAX), None);
        let all = r(0, u64::MAX);
        assert_eq!(
            (all.start(), all.end(), all.len(), all.is_empty()),
            (0, u64::MAX, u64::MAX, false)
        );
    }

    #[test]
    fn union_merges_adjacent_overlapping_contained_and_unsorted_insertions() {
        let mut set = RangeSet::new();
        for (a, b) in [(10, 20), (30, 40), (15, 35), (12, 14), (0, 10), (40, 50)] {
            set.insert(r(a, b));
        }
        assert_eq!(set.ranges(), &[r(0, 50)]);
        assert_eq!(set.total_bytes(), 50);
        assert!(set.contains(r(0, 50)));
        assert!(set.contains(r(2, 3)));
        assert!(!set.contains(r(50, 51)));
        assert!(!set.contains(r(49, 51)));
    }

    #[test]
    fn missing_and_intersection_clip_boundaries_without_empty_intervals() {
        let mut set = RangeSet::new();
        assert_eq!(set.missing(r(0, 30)), vec![r(0, 30)]);
        assert!(set.intersection(r(0, 30)).ranges().is_empty());
        set.insert(r(5, 10));
        set.insert(r(15, 20));
        set.insert(r(25, 35));
        assert_eq!(set.missing(r(0, 30)), vec![r(0, 5), r(10, 15), r(20, 25)]);
        assert_eq!(
            set.intersection(r(8, 27)).ranges(),
            &[r(8, 10), r(15, 20), r(25, 27)]
        );
        assert_eq!(set.missing(r(10, 15)), vec![r(10, 15)]);
        assert!(set.intersection(r(10, 15)).ranges().is_empty());
        assert!(set.missing(r(5, 10)).is_empty());
        assert!(!set.contains(r(5, 20)));
    }

    #[test]
    fn union_and_gaps_do_not_overflow() {
        let mut set = RangeSet::new();
        set.insert(r(u64::MAX - 1, u64::MAX));
        set.insert(r(0, 1));
        assert_eq!(set.total_bytes(), 2);
        assert_eq!(set.missing(r(0, u64::MAX)), vec![r(1, u64::MAX - 1)]);
        set.insert(r(1, u64::MAX - 1));
        assert_eq!(set.total_bytes(), u64::MAX);
        assert_eq!(set.ranges(), &[r(0, u64::MAX)]);
    }

    #[test]
    fn exhaustive_small_unions_match_byte_availability() {
        for a in 0..8 {
            for b in a + 1..9 {
                for c in 0..8 {
                    for d in c + 1..9 {
                        let mut set = RangeSet::new();
                        set.insert(r(a, b));
                        set.insert(r(c, d));
                        let available: Vec<_> = (0..9)
                            .map(|i| (a..b).contains(&i) || (c..d).contains(&i))
                            .collect();
                        assert_eq!(
                            set.total_bytes(),
                            available.iter().filter(|yes| **yes).count() as u64
                        );
                        for start in 0..8 {
                            for end in start + 1..9 {
                                let request = r(start, end);
                                assert_eq!(
                                    set.contains(request),
                                    (start..end).all(|i| available[i as usize])
                                );
                                let missing = set.missing(request);
                                let intersection = set.intersection(request);
                                for i in 0..9 {
                                    assert_eq!(
                                        missing.iter().any(|part| part.start <= i && i < part.end),
                                        (start..end).contains(&i) && !available[i as usize]
                                    );
                                    assert_eq!(
                                        intersection.contains(r(i, i + 1)),
                                        (start..end).contains(&i) && available[i as usize]
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn closed_open_and_suffix_ranges_resolve_and_clamp() {
        for (header, expected) in [
            ("bytes=0-0", r(0, 1)),
            ("bytes=1-4", r(1, 5)),
            ("bytes=5-99", r(5, 10)),
            ("bytes=5-", r(5, 10)),
            ("bytes=-3", r(7, 10)),
            ("bytes=-10", r(0, 10)),
            ("bytes=-999", r(0, 10)),
            (" Bytes=000-001 \t", r(0, 2)),
        ] {
            assert_eq!(parse_http_range(header, 10), Ok(expected), "{header}");
        }
    }

    #[test]
    fn invalid_unsupported_and_unsatisfiable_are_distinct() {
        for header in [
            "",
            "bytes",
            "items=0-1",
            "bytes=",
            "bytes=-",
            "bytes=1",
            "bytes=2-1",
            "bytes=+1-2",
            "bytes=1--2",
            "bytes= 1-2",
            "bytes=1 -2",
            "bytes=١-٢",
            "bytes=0-18446744073709551616",
            "bytes=-18446744073709551616",
            "\nbytes=0-1",
            "bytes=0-1\r\n",
            "\u{2003}bytes=0-1",
        ] {
            assert_eq!(
                parse_http_range(header, 10),
                Err(RangeError::Invalid),
                "{header}"
            );
        }
        for header in ["bytes=0-1,3-4", "bytes=0-1,", "bytes=0-1, 3-4"] {
            assert_eq!(
                parse_http_range(header, 10),
                Err(RangeError::MultipleRangesUnsupported)
            );
        }
        for header in ["bytes=10-", "bytes=10-12", "bytes=-0"] {
            assert_eq!(parse_http_range(header, 10), Err(RangeError::Unsatisfiable));
        }
        for header in ["bytes=0-0", "bytes=0-", "bytes=-1"] {
            assert_eq!(parse_http_range(header, 0), Err(RangeError::Unsatisfiable));
        }
    }

    #[test]
    fn http_endpoint_arithmetic_never_overflows() {
        assert_eq!(
            parse_http_range("bytes=0-18446744073709551615", u64::MAX),
            Ok(r(0, u64::MAX))
        );
        assert_eq!(
            parse_http_range("bytes=18446744073709551614-18446744073709551615", u64::MAX),
            Ok(r(u64::MAX - 1, u64::MAX))
        );
        assert_eq!(
            parse_http_range("bytes=18446744073709551615-", u64::MAX),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            parse_http_range("bytes=-18446744073709551615", u64::MAX),
            Ok(r(0, u64::MAX))
        );
    }
}
