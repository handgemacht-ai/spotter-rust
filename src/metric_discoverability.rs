//! Discoverability metric: search effort before the first successful read of a
//! file.

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Per-file discoverability cost.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoverFile {
    /// Canonical file path.
    pub path: String,
    /// Distinct sessions contributing to the median.
    pub sessions: usize,
    /// Median search cost (Grep/Glob calls + failed reads) before the first
    /// successful read, across sessions.
    pub median_cost: f64,
}

/// Result payload for the discoverability metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiscoverabilityResult {
    /// Per-file discoverability costs, sorted desc by metric then path.
    pub files: Vec<DiscoverFile>,
}

/// Compute per-file discoverability (Grep/Glob calls + failed reads before the
/// first successful read; per-file median across sessions; `>= 3` sessions only).
///
/// Wave-0 stub: returns an empty, contract-valid result. A later slice fills in
/// the pre-first-read search counting over [`SessionFacts::events`].
#[must_use]
pub fn discoverability(facts: &[SessionFacts], opts: &RelationsOptions) -> DiscoverabilityResult {
    let _ = (facts, opts);
    DiscoverabilityResult::default()
}
