//! Rework metric: how many distinct logical sessions edit a file in the window.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Distinct-editing-session count at or above which a file earns the rework badge.
const REWORK_BADGE_THRESHOLD: usize = 5;

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

/// Compute per-file rework: the number of distinct logical sessions that made at
/// least one edit of the file within the metric window.
///
/// Edits already fold in Bash-mediated writes (`sed -i`, `tee`, `git mv`,
/// heredoc/`>` redirection) — [`crate::session_facts::build_session_facts`]
/// records them into [`SessionFacts::edits`] — so "incl. bash-detected" needs no
/// special handling here.
///
/// Coordinator sessions are *kept*: the coordinator guard only excludes them from
/// the pairwise and friction metrics, not from per-file scalars like rework.
/// A session that edits the same file many times counts once (distinct sessions).
#[must_use]
pub fn rework(facts: &[SessionFacts], opts: &RelationsOptions) -> ReworkResult {
    let window_start = opts.now - Duration::days(i64::from(opts.since_days));

    // Canonical path -> distinct logical session ids that edited it in the window.
    let mut sessions_by_path: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for session in facts {
        for edit in &session.edits {
            if edit.path.is_empty() || !in_window(edit.ts.as_deref(), window_start) {
                continue;
            }
            sessions_by_path
                .entry(edit.path.clone())
                .or_default()
                .insert(session.external_session_id.clone());
        }
    }

    let mut files: Vec<ReworkFile> = sessions_by_path
        .into_iter()
        .map(|(path, sessions)| {
            let sessions = sessions.len();
            ReworkFile {
                path,
                sessions,
                badge: sessions >= REWORK_BADGE_THRESHOLD,
            }
        })
        .collect();
    // Frozen list ordering: desc by metric (session count), then path ascending.
    files.sort_by(|left, right| {
        right
            .sessions
            .cmp(&left.sessions)
            .then_with(|| left.path.cmp(&right.path))
    });

    ReworkResult { files }
}

/// Whether an edit timestamp falls at or after the window start.
///
/// Absent or unparseable timestamps are kept: the transcript file list is already
/// mtime-pruned to the window, so an undated edit (e.g. a Bash-detected write with
/// no `started_at`) cannot be proven out of range and is not dropped.
fn in_window(ts: Option<&str>, window_start: DateTime<Utc>) -> bool {
    ts.and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map_or(true, |parsed| parsed.with_timezone(&Utc) >= window_start)
}
