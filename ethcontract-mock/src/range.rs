//! Helpers for working with rust's ranges.
//!
//! Note: contents of this module are meant to be used via the [`Into`] trait.
//! They are not a part of public API.

use std::ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};

/// A type that represents a rust's range, i.e., a struct produced
/// by range syntax like `..`, `a..`, `..b`, `..=c`, `d..e`, or `f..=g`.
///
/// Each of the above range types is represented by a distinct `std` struct.
/// Standard library does not export a single struct to represent all of them,
/// so we have to implement it ourselves.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct TimesRange(usize, usize);

impl TimesRange {
    /// Checks if expectation can be called if it was already called
    /// this number of times.
    pub fn can_call(&self, x: usize) -> bool {
        x + 1 < self.1
    }

    /// Checks if the given element is contained by this range.
    pub fn contains(&self, x: usize) -> bool {
        self.0 <= x && x < self.1
    }

    /// Checks if this range contains exactly one element.
    pub fn is_exact(&self) -> bool {
        (self.1 - self.0) == 1
    }

    /// Returns lower bound on this range.
    pub fn lower_bound(&self) -> usize {
        self.0
    }

    /// Returns upper bound on this range.
    pub fn upper_bound(&self) -> usize {
        self.1
    }
}

impl Default for TimesRange {
    fn default() -> TimesRange {
        TimesRange(0, usize::MAX)
    }
}

impl From<usize> for TimesRange {
    fn from(n: usize) -> TimesRange {
        TimesRange(n, n + 1)
    }
}

impl From<Range<usize>> for TimesRange {
    fn from(r: Range<usize>) -> TimesRange {
        assert!(r.end > r.start, "backwards range");
        TimesRange(r.start, r.end)
    }
}

impl From<RangeFrom<usize>> for TimesRange {
    fn from(r: RangeFrom<usize>) -> TimesRange {
        TimesRange(r.start, usize::MAX)
    }
}

impl From<RangeFull> for TimesRange {
    fn from(_: RangeFull) -> TimesRange {
        TimesRange(0, usize::MAX)
    }
}

impl From<RangeInclusive<usize>> for TimesRange {
    fn from(r: RangeInclusive<usize>) -> TimesRange {
        assert!(r.end() >= r.start(), "backwards range");
        TimesRange(*r.start(), *r.end() + 1)
    }
}

impl From<RangeTo<usize>> for TimesRange {
    fn from(r: RangeTo<usize>) -> TimesRange {
        TimesRange(0, r.end)
    }
}

impl From<RangeToInclusive<usize>> for TimesRange {
    fn from(r: RangeToInclusive<usize>) -> TimesRange {
        TimesRange(0, r.end + 1)
    }
}
