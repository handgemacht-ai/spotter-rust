//! Cost metric: relative maintenance cost per file, from per-turn token usage.

use std::collections::BTreeMap;

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

/// The number of relative cost tiers, `0..=4`.
const TIER_BUCKETS: i64 = 5;

/// Map a file's token cost onto a relative tier `0..=4`, scaled linearly against
/// the most expensive file. The costliest file lands in tier 4; a file costing
/// at most a fifth of it lands in tier 0. Integer arithmetic keeps the tier
/// byte-stable across platforms (no float rounding at a bucket boundary).
fn tier_for(tokens: i64, max_tokens: i64) -> u8 {
    if max_tokens <= 0 {
        return 0;
    }
    let scaled = (TIER_BUCKETS * tokens.max(0)) / max_tokens;
    u8::try_from(scaled.clamp(0, 4)).unwrap_or(0)
}

/// Running total for one file while folding turns.
#[derive(Default)]
struct CostAccum {
    tokens: i64,
    turns: usize,
}

/// Compute per-file relative maintenance cost.
///
/// For every assistant turn (already deduped by `message.id` and carrying
/// `output + cache_creation` in each turn's `attributed_tokens`), the tokens are
/// split evenly across the files edited in that turn (bash-mediated writes
/// included) and accumulated; each such turn also counts once toward the file.
/// Coordinator sessions keep their per-file scalar cost here. The relative tier
/// is assigned against the most expensive file in the result.
#[must_use]
pub fn cost(facts: &[SessionFacts], opts: &RelationsOptions) -> CostResult {
    let _ = opts;

    // Canonical path -> accumulated tokens and contributing turns.
    let mut per_file: BTreeMap<String, CostAccum> = BTreeMap::new();

    for session in facts {
        for turn in &session.turns {
            let file_count = turn.files.len();
            if file_count == 0 {
                continue;
            }
            // Split evenly across the files edited in this turn; the truncated
            // remainder is dropped so the total stays integer and deterministic.
            let share = turn.attributed_tokens / file_count as i64;
            for path in &turn.files {
                let accum = per_file.entry(path.clone()).or_default();
                accum.tokens += share;
                accum.turns += 1;
            }
        }
    }

    let max_tokens = per_file
        .values()
        .map(|accum| accum.tokens)
        .max()
        .unwrap_or(0);

    let mut files: Vec<CostFile> = per_file
        .into_iter()
        .map(|(path, accum)| CostFile {
            tier: tier_for(accum.tokens, max_tokens),
            path,
            tokens: accum.tokens,
            turns: accum.turns,
        })
        .collect();

    // Frozen contract: sorted desc by tokens, ties broken by path ascending.
    files.sort_by(|left, right| {
        right
            .tokens
            .cmp(&left.tokens)
            .then_with(|| left.path.cmp(&right.path))
    });

    CostResult { files }
}
