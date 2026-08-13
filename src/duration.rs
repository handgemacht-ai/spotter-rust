//! `Seconds` / `Millis` duration newtypes.
//!
//! Several fields and folds carried durations as bare `i64`: the friction
//! fold's active-time gap cap and per-session active seconds (`metric_friction`)
//! sit next to `ToolCallRun.duration_ms` (milliseconds) in [`crate::db`]. With
//! both expressed as `i64` the compiler cannot catch a seconds-vs-millis mix-up
//! — exactly the class of mistake a newtype exists to prevent.
//!
//! [`Seconds`] and [`Millis`] name the unit at the type level. Public struct
//! fields and the JSON/DB wire format stay `i64` (see
//! [`crate::timestamp::Timestamp`] for the same boundary-vs-in-memory split);
//! the newtypes only type the internal pipelines that *produce* and *combine*
//! these durations, so the cap, the gap sum, and the millisecond computation
//! are no longer interchangeable with each other or with raw counters.

use serde::{Deserialize, Serialize};

/// A duration in seconds.
///
/// Construct via [`Seconds::from_inner`] and read via [`Seconds::as_inner`].
/// Compare via the derived [`Ord`]. Serializes transparently as a bare integer
/// so the wire format is unchanged.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Seconds(i64);

impl Seconds {
    /// Wrap a raw seconds count.
    #[must_use]
    pub const fn from_inner(seconds: i64) -> Self {
        Self(seconds)
    }

    /// The underlying seconds count.
    #[must_use]
    pub const fn as_inner(self) -> i64 {
        self.0
    }
}

/// A duration in milliseconds.
///
/// Construct via [`Millis::from_inner`] and read via [`Millis::as_inner`].
/// Serializes transparently as a bare integer so the wire format is unchanged.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Millis(i64);

impl Millis {
    /// Wrap a raw milliseconds count.
    #[must_use]
    pub const fn from_inner(millis: i64) -> Self {
        Self(millis)
    }

    /// The underlying milliseconds count.
    #[must_use]
    pub const fn as_inner(self) -> i64 {
        self.0
    }
}
