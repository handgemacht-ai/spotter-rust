//! Cost metric: relative maintenance cost per file, from per-turn token usage.

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Per-file relative maintenance cost.
///
/// Tokens are `output + cache-creation` (cache-read excluded so the figure
/// tracks work, not context size), split evenly across the files edited in each
/// turn and summed. Reported as a relative tier, never dollars.
#[derive(Debug, Clone, Serialize)]
pub struct CostFile {
    /// Canonical file path.
    pub path: String,
    /// Attributed tokens for the file.
    pub tokens: i64,
    /// Number of assistant turns contributing to the file.
    pub turns: usize,
    /// Relative cost tier, `0..=4`.
    pub tier: u8,
}

/// Result payload for the cost metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CostResult {
    /// Per-file costs, sorted desc by tokens then path.
    pub files: Vec<CostFile>,
}

/// Compute per-file relative maintenance cost (per turn, deduped by
/// `message.id`, `output + cache_creation` split evenly across edited files).
///
/// Wave-0 stub: returns an empty, contract-valid result. A later slice fills in
/// the token attribution over [`SessionFacts::turns`].
#[must_use]
pub fn cost(facts: &[SessionFacts], opts: &RelationsOptions) -> CostResult {
    let _ = (facts, opts);
    CostResult::default()
}
