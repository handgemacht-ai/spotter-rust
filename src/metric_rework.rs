//! Rework metric: how many distinct logical sessions edit a file in the window.

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Per-file rework count.
#[derive(Debug, Clone, Serialize)]
pub struct ReworkFile {
    /// Canonical file path.
    pub path: String,
    /// Distinct logical sessions that edited the file in the window.
    pub sessions: usize,
    /// Whether the file crosses the rework badge threshold (`sessions >= 5`).
    pub badge: bool,
}

/// Result payload for the rework metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReworkResult {
    /// Per-file rework counts, sorted desc by sessions then path.
    pub files: Vec<ReworkFile>,
}

/// Compute per-file rework (distinct editing sessions in 30d; badge at `>= 5`).
/// Coordinator sessions keep their per-file scalar edits here.
///
/// Wave-0 stub: returns an empty, contract-valid result. A later slice fills in
/// the per-file session counting over [`SessionFacts::edits`].
#[must_use]
pub fn rework(facts: &[SessionFacts], opts: &RelationsOptions) -> ReworkResult {
    let _ = (facts, opts);
    ReworkResult::default()
}
