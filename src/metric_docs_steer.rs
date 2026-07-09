//! Docs-that-steer metric: docs whose read precedes a later code edit in the
//! same session.

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// A doc that appears to steer subsequent code edits.
#[derive(Debug, Clone, Serialize)]
pub struct SteerDoc {
    /// Canonical doc path.
    pub doc: String,
    /// Code files edited after the doc was read, sorted.
    pub targets: Vec<String>,
    /// Distinct sessions where the doc read preceded a code edit.
    pub sessions: usize,
}

/// Result payload for the docs-that-steer metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DocsSteerResult {
    /// Steering docs, sorted desc by sessions then doc path.
    pub docs: Vec<SteerDoc>,
}

/// Compute docs-that-steer (distinct sessions where a doc `Read` precedes a
/// later code edit; session-start auto-reads of `CLAUDE.md`/`AGENTS.md`/`README`
/// excluded).
///
/// Wave-0 stub: returns an empty, contract-valid result. A later slice fills in
/// the read-before-edit ordering over [`SessionFacts::events`].
#[must_use]
pub fn docs_steer(facts: &[SessionFacts], opts: &RelationsOptions) -> DocsSteerResult {
    let _ = (facts, opts);
    DocsSteerResult::default()
}
