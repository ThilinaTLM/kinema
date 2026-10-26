//! Small, platform-independent domain types and rules.
//!
//! Keep GUI, provider protocols, persistence, and filesystem operations outside
//! this crate.

pub mod catalog;
pub mod catalog_normalized;
pub mod discover;
pub mod library;
pub mod media;
pub mod provider;
pub mod sources;

use std::num::NonZeroU64;

/// The catalog distinguishes movies from series, not releases or media files.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MediaKind {
    Movie,
    Series,
}

/// A nonzero byte limit for managed media, separate from other cache budgets.
///
/// This is only an admission calculation: callers must account for allocated and
/// reserved bytes and separately check physical free space and active file use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheBudget(NonZeroU64);

impl CacheBudget {
    /// Returns `None` for zero; an unlimited cache is not represented by zero.
    pub const fn new(bytes: u64) -> Option<Self> {
        match NonZeroU64::new(bytes) {
            Some(bytes) => Some(Self(bytes)),
            None => None,
        }
    }

    pub const fn bytes(self) -> u64 {
        self.0.get()
    }

    /// Whether additional bytes fit, given bytes already allocated or reserved.
    ///
    /// Exact capacity is allowed; overflow or existing overuse is rejected.
    /// This does not reserve space or make concurrent admissions atomic.
    pub const fn admits(self, accounted_bytes: u64, additional_bytes: u64) -> bool {
        match accounted_bytes.checked_add(additional_bytes) {
            Some(total) => total <= self.bytes(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CacheBudget;

    #[test]
    fn zero_budget_is_invalid() {
        assert_eq!(CacheBudget::new(0), None);
    }

    #[test]
    fn smallest_budget_accepts_exactly_one_byte() {
        let Some(budget) = CacheBudget::new(1) else {
            panic!("a positive budget must be valid");
        };
        assert_eq!(budget.bytes(), 1);
        assert!(budget.admits(0, 1));
        assert!(!budget.admits(0, 2));
    }

    #[test]
    fn capacity_boundary_includes_existing_usage() {
        let Some(budget) = CacheBudget::new(100) else {
            panic!("a positive budget must be valid");
        };
        assert!(budget.admits(40, 59));
        assert!(budget.admits(40, 60));
        assert!(!budget.admits(40, 61));
        assert!(!budget.admits(0, 101));
        assert!(budget.admits(100, 0));
        assert!(!budget.admits(101, 0));
    }

    #[test]
    fn arithmetic_overflow_is_not_available_capacity() {
        let Some(budget) = CacheBudget::new(u64::MAX) else {
            panic!("a positive budget must be valid");
        };
        assert!(budget.admits(u64::MAX - 1, 1));
        assert!(!budget.admits(u64::MAX, 1));
        assert!(!budget.admits(1, u64::MAX));
    }
}
