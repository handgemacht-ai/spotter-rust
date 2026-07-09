//! Friction metric: active time spent in sessions that read a file versus
//! sessions that edit it, reported per cohort.

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Active-time summary for one cohort (readers or editors) of a file.
///
/// Active time sums consecutive-event gaps, each capped at 300s to kill
/// overnight-idle inflation.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FrictionStat {
    /// Distinct sessions in the cohort.
    pub sessions: usize,
    /// Median active seconds across the cohort.
    pub median_active_seconds: i64,
    /// 90th-percentile active seconds across the cohort.
    pub p90_active_seconds: i64,
}

/// Per-file friction split into read and edit cohorts.
#[derive(Debug, Clone, Serialize)]
pub struct FrictionFile {
    /// Canonical file path.
    pub path: String,
    /// Active-time stats for sessions that read the file.
    pub read: FrictionStat,
    /// Active-time stats for sessions that edit the file.
    pub edit: FrictionStat,
}

/// Result payload for the friction metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FrictionResult {
    /// Per-file friction stats, sorted desc by metric then path.
    pub files: Vec<FrictionFile>,
}

/// Compute per-file read/edit friction from active time (gap cap 300s; median
/// and p90 per cohort).
///
/// Wave-0 stub: returns an empty, contract-valid result. A later slice fills in
/// the active-time accumulation over [`SessionFacts::events`].
#[must_use]
pub fn friction(facts: &[SessionFacts], opts: &RelationsOptions) -> FrictionResult {
    let _ = (facts, opts);
    FrictionResult::default()
}
