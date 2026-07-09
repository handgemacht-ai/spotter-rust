//! Discoverability metric: search effort before the first successful read of a
//! file.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionEventKind, SessionFacts};

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

/// Minimum distinct logical sessions required before a file is reported.
const MIN_SESSIONS: usize = 3;

/// Compute per-file discoverability (Grep/Glob calls + failed reads before the
/// first successful read; per-file median across sessions; `>= 3` sessions only).
///
/// Within one logical session a running counter accumulates search effort —
/// each `Grep`/`Glob` call and each *failed* `Read` — in event order. The first
/// *successful* `Read` of a file freezes that file's per-session cost at the
/// counter's current value; later reads of the same file in that session do not
/// change it. A file's `median_cost` is the median of its per-session costs, and
/// only files seen in at least [`MIN_SESSIONS`] sessions are emitted, sorted
/// descending by cost then ascending by path.
///
/// Coordinator sessions keep their per-file scalar contributions here (the
/// coordinator guard only drops pairwise metrics and friction), matching rework
/// and cost.
#[must_use]
pub fn discoverability(facts: &[SessionFacts], opts: &RelationsOptions) -> DiscoverabilityResult {
    let _ = opts;

    // Canonical path -> per-session search costs (one entry per logical session
    // that successfully read the file).
    let mut costs: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    for session in facts {
        // Running search effort (Grep/Glob calls + failed reads) seen so far in
        // this session, in event order.
        let mut search_so_far: usize = 0;
        // Cost frozen at each file's first successful read within this session.
        let mut first_read_cost: BTreeMap<String, usize> = BTreeMap::new();

        for event in &session.events {
            match event.kind {
                SessionEventKind::Read => {
                    if event.success {
                        if let Some(path) = &event.path {
                            first_read_cost.entry(path.clone()).or_insert(search_so_far);
                        }
                    } else {
                        // A failed read is search effort toward finding files
                        // not yet successfully read.
                        search_so_far += 1;
                    }
                }
                SessionEventKind::Grep | SessionEventKind::Glob => search_so_far += 1,
                SessionEventKind::Edit | SessionEventKind::Other => {}
            }
        }

        for (path, cost) in first_read_cost {
            costs.entry(path).or_default().push(cost);
        }
    }

    let mut files: Vec<DiscoverFile> = costs
        .into_iter()
        .filter(|(_, values)| values.len() >= MIN_SESSIONS)
        .map(|(path, values)| DiscoverFile {
            sessions: values.len(),
            median_cost: median(values),
            path,
        })
        .collect();

    // Desc by metric, then path (frozen list ordering).
    files.sort_by(|left, right| {
        right
            .median_cost
            .partial_cmp(&left.median_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.path.cmp(&right.path))
    });

    DiscoverabilityResult { files }
}

/// Median of a non-empty list of integer costs, rounded to two decimals.
///
/// Averaging two integer middles yields an exact integer or half-integer, so the
/// rounding never loses information; it keeps the `round2` convention shared with
/// the other float-valued metrics.
fn median(mut values: Vec<usize>) -> f64 {
    values.sort_unstable();
    let mid = values.len() / 2;
    let raw = if values.len() % 2 == 1 {
        values[mid] as f64
    } else {
        (values[mid - 1] + values[mid]) as f64 / 2.0
    };
    round2(raw)
}

/// Round to two decimal places (matches the shared `round2` convention).
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
