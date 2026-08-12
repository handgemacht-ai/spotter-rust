//! Session co-change metric: file edit-pairs observed within one logical session.
//!
//! Two files "co-change in a session" when the same logical session edits both.
//! The metric is directional: for an ordered pair `a → b` the *support* is the
//! number of distinct logical sessions that edited both `a` and `b`, while the
//! *confidence* normalises that support by how often `a` is edited at all
//! (`support / sessions_editing(a)`), so a hub file that co-occurs with
//! everything does not manufacture strong links out of itself.
//!
//! Coordinator sessions (multi-rig / many-cwd) are excluded — their file sets
//! are grab-bags that would fabricate spurious pairs. The fan-out cap bounds how
//! many pairs a single big session may emit; it never touches the per-file
//! session counts, so a capped session still feeds `sessions_editing(a)`.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Minimum distinct sessions supporting a pair before it is emitted.
const MIN_SUPPORT: usize = 3;
/// Minimum directional confidence before a pair is emitted.
const MIN_CONFIDENCE: f64 = 0.4;

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

/// Round a ratio to two decimals for stable, deterministic emission.
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Compute session co-change edit-pairs.
///
/// A pair `a → b` is emitted when at least [`MIN_SUPPORT`] distinct
/// non-coordinator sessions edited both files and the directional confidence
/// `support / sessions_editing(a)` is at least [`MIN_CONFIDENCE`]. Support is
/// symmetric but confidence is not, so `a → b` and `b → a` are judged
/// independently.
///
/// Coordinator sessions are dropped before any counting. Within each remaining
/// session a file is counted once regardless of how many times it was edited.
/// The fan-out cap (`opts.fanout_cap`, `K`) bounds pair emission from a single
/// session: at most the first `K` of its edited files (sorted) form pairs, while
/// every edited file still increments the per-file `sessions_editing` scalar
/// that feeds the confidence denominator.
///
/// Output is deterministic: pairs are ordered by descending support, then
/// descending confidence, then `a`, then `b`.
#[must_use]
pub fn cochange_session(facts: &[SessionFacts], opts: &RelationsOptions) -> CochangeSessionResult {
    // Per-file scalar: distinct non-coordinator sessions that edited the file.
    // Never capped — it is the confidence denominator.
    let mut edit_sessions: BTreeMap<&str, usize> = BTreeMap::new();
    // Ordered-pair support: distinct sessions editing both `a` and `b`.
    let mut pair_support: BTreeMap<(&str, &str), usize> = BTreeMap::new();

    for session in facts
        .iter()
        .filter(|session| !session.coordination.is_coordinator())
    {
        // Distinct edited paths, deterministically ordered for the fan-out cap.
        let distinct: BTreeSet<&str> = session
            .edits
            .iter()
            .map(|event| event.path.as_str())
            .collect();
        for &path in &distinct {
            *edit_sessions.entry(path).or_insert(0) += 1;
        }
        // Fan-out cap bites here only: a big session contributes pairs from at
        // most `K` of its files, but its scalar counts above are untouched.
        let paired: Vec<&str> = distinct.into_iter().take(opts.fanout_cap).collect();
        for &a in &paired {
            for &b in &paired {
                if a != b {
                    *pair_support.entry((a, b)).or_insert(0) += 1;
                }
            }
        }
    }

    let mut pairs: Vec<SessionPair> = pair_support
        .into_iter()
        .filter(|&(_, support)| support >= MIN_SUPPORT)
        .filter_map(|((a, b), support)| {
            let edits_of_a = edit_sessions.get(a).copied().unwrap_or_default();
            if edits_of_a == 0 {
                return None;
            }
            let confidence = support as f64 / edits_of_a as f64;
            if confidence < MIN_CONFIDENCE {
                return None;
            }
            Some(SessionPair {
                a: a.to_string(),
                b: b.to_string(),
                support,
                confidence: round2(confidence),
            })
        })
        .collect();

    pairs.sort_by(|left, right| {
        right
            .support
            .cmp(&left.support)
            .then_with(|| {
                right
                    .confidence
                    .partial_cmp(&left.confidence)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.a.cmp(&right.a))
            .then_with(|| left.b.cmp(&right.b))
    });

    CochangeSessionResult { pairs }
}
