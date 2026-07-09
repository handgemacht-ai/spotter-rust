//! Session co-change metric: file edit-pairs observed within one logical session.

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// One asymmetric edit-pair `a → b`.
///
/// `confidence` is the directional strength `support / sessions_editing(a)`.
#[derive(Debug, Clone, Serialize)]
pub struct SessionPair {
    /// Source file of the directional pair.
    pub a: String,
    /// File frequently edited in the same session as `a`.
    pub b: String,
    /// Distinct logical sessions supporting the pair.
    pub support: usize,
    /// Directional confidence, `support / sessions_editing(a)`.
    pub confidence: f64,
}

/// Result payload for the session co-change metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CochangeSessionResult {
    /// Asymmetric edit-pairs meeting the thresholds, sorted desc by metric then path.
    pub pairs: Vec<SessionPair>,
}

/// Compute session co-change edit-pairs (coordinators excluded, `support >= 3`,
/// `confidence >= 0.4`).
///
/// Wave-0 stub: returns an empty, contract-valid result. A later slice fills in
/// the pairwise counting over [`SessionFacts::edits`].
#[must_use]
pub fn cochange_session(facts: &[SessionFacts], opts: &RelationsOptions) -> CochangeSessionResult {
    let _ = (facts, opts);
    CochangeSessionResult::default()
}
