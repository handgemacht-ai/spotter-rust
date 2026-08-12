//! A parse-once RFC3339 timestamp newtype.
//!
//! Several structs (`FileEvent`, `SessionEvent`, `SessionRecord`,
//! `ToolCallRun`, `LeanMessage`) carried their timestamps as `Option<String>`
//! RFC3339 fields. Every sort or compare site re-parsed the string with
//! `DateTime::parse_from_rfc3339`, and every persistence boundary
//! re-stringified it with `to_rfc3339`. Worse, the sorts that *did not*
//! re-parse — comparing `Option<String>` directly — ordered lexicographically,
//! which is wrong for mixed-offset RFC3339 (`Z` vs `+00:00` vs `+05:30`) even
//! though it is incidentally correct for the single-offset test fixtures.
//!
//! [`Timestamp`] carries the parsed `DateTime<Utc>` so `Option<Timestamp>`
//! ordering is real chronological [`Ord`], not lexicographic. The DB boundary
//! still stores `TEXT` (see [`Timestamp::to_rfc3339`]); the newtype only changes
//! the in-memory representation and the compare sites.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A parsed RFC3339 timestamp in UTC.
///
/// Construct via [`Timestamp::parse`] (from a stored string) or
/// [`Timestamp::from`] (from a `DateTime<Utc>` the parser already produced).
/// Compare via the derived [`Ord`], which delegates to the inner `DateTime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    /// Parse an RFC3339 string, returning `None` on failure — mirroring the
    /// prior `DateTime::parse_from_rfc3339(raw).ok()` sites.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        DateTime::parse_from_rfc3339(raw)
            .map(|dt| Self(dt.with_timezone(&Utc)))
            .ok()
    }

    /// The underlying UTC datetime.
    #[must_use]
    pub const fn as_inner(&self) -> &DateTime<Utc> {
        &self.0
    }

    /// Re-stringify to RFC3339 for the DB `TEXT` boundary.
    #[must_use]
    pub fn to_rfc3339(&self) -> String {
        self.0.to_rfc3339()
    }
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
