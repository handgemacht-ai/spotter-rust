//! Friction metric: active time spent in sessions that read a file versus
//! sessions that edit it, reported per cohort.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Active-time gap cap in seconds: any pause longer than this between two
/// consecutive events is clamped, so overnight-idle stretches do not inflate a
/// session's active time.
const GAP_CAP_SECONDS: i64 = 300;

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
/// Coordinator sessions are excluded (a coordinator's long, multi-rig active
/// time would swamp the cohorts). For each remaining logical session the active
/// time is the sum of consecutive-event gaps, each capped at
/// [`GAP_CAP_SECONDS`]. Every file that any surviving session read or edited is
/// reported; the read and edit cohorts are counted independently and each yields
/// a median and p90 of its sessions' active time.
#[must_use]
pub fn friction(facts: &[SessionFacts], opts: &RelationsOptions) -> FrictionResult {
    let _ = opts;

    // Per-session active seconds, keyed by logical session id, coordinators out.
    let mut active: BTreeMap<&str, i64> = BTreeMap::new();
    // Canonical path -> the logical sessions that read / edited it.
    let mut readers: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut editors: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

    for session in facts
        .iter()
        .filter(|session| !session.coordination.is_coordinator())
    {
        let sid = session.external_session_id.as_str();
        active.insert(sid, active_seconds(session));
        for event in &session.reads {
            readers.entry(event.path.as_str()).or_default().insert(sid);
        }
        for event in &session.edits {
            editors.entry(event.path.as_str()).or_default().insert(sid);
        }
    }

    // Union of every path that was read or edited by a surviving session.
    let mut paths: BTreeSet<&str> = BTreeSet::new();
    paths.extend(readers.keys().copied());
    paths.extend(editors.keys().copied());

    let mut files: Vec<FrictionFile> = paths
        .into_iter()
        .map(|path| FrictionFile {
            path: path.to_string(),
            read: cohort_stat(&active, readers.get(path)),
            edit: cohort_stat(&active, editors.get(path)),
        })
        .collect();

    // Desc by the friction "metric" (the higher of the two cohort medians),
    // then path asc for a deterministic, unique ordering.
    let rank = |file: &FrictionFile| -> i64 {
        file.edit
            .median_active_seconds
            .max(file.read.median_active_seconds)
    };
    files.sort_by(|left, right| {
        rank(right)
            .cmp(&rank(left))
            .then_with(|| left.path.cmp(&right.path))
    });

    FrictionResult { files }
}

/// Sum the consecutive-event gaps of a session, each capped at
/// [`GAP_CAP_SECONDS`]. Events without a parseable timestamp are ignored; the
/// remaining timestamps are ordered so a gap is never negative.
fn active_seconds(session: &SessionFacts) -> i64 {
    let mut seconds: Vec<i64> = session
        .events
        .iter()
        .filter_map(|event| event.ts)
        .map(|ts| ts.as_inner().timestamp())
        .collect();
    seconds.sort_unstable();
    seconds
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).clamp(0, GAP_CAP_SECONDS))
        .sum()
}

/// Reduce a cohort of sessions to a [`FrictionStat`] over their active times.
///
/// A missing cohort (no session read or edited the file) yields the zero stat.
fn cohort_stat(active: &BTreeMap<&str, i64>, cohort: Option<&BTreeSet<&str>>) -> FrictionStat {
    let Some(sessions) = cohort else {
        return FrictionStat::default();
    };
    let mut values: Vec<i64> = sessions
        .iter()
        .map(|sid| active.get(*sid).copied().unwrap_or(0))
        .collect();
    values.sort_unstable();
    FrictionStat {
        sessions: values.len(),
        median_active_seconds: percentile(&values, 50),
        p90_active_seconds: percentile(&values, 90),
    }
}

/// Nearest-rank percentile over an ascending-sorted slice, matching the
/// convention used elsewhere in `spotter` (see `analytics::percentile`). Returns
/// `0` for an empty cohort.
fn percentile(sorted: &[i64], pct: usize) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = (sorted.len() * pct).div_ceil(100).saturating_sub(1);
    sorted.get(index).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{TimeZone, Utc};

    use super::{friction, FrictionFile, FrictionResult};
    use crate::session_facts::{
        CoordinationClass, FileEvent, RelationsOptions, SessionEvent, SessionEventKind,
        SessionFacts,
    };
    use crate::timestamp::Timestamp;

    fn opts() -> RelationsOptions {
        RelationsOptions {
            now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
            since_days: 30,
            fanout_cap: 100,
        }
    }

    fn event(ts: &str) -> SessionEvent {
        SessionEvent {
            ts: Timestamp::parse(ts),
            kind: SessionEventKind::Other,
            path: None,
            success: true,
            message_id: None,
        }
    }

    fn touch(path: &str) -> FileEvent {
        FileEvent {
            path: path.to_string(),
            ts: None,
            message_id: None,
        }
    }

    fn session(
        id: &str,
        coordinator: bool,
        reads: &[&str],
        edits: &[&str],
        events: Vec<SessionEvent>,
    ) -> SessionFacts {
        SessionFacts {
            external_session_id: id.to_string(),
            coordination: if coordinator {
                CoordinationClass::MultiRig
            } else {
                CoordinationClass::Single
            },
            rigs: BTreeSet::new(),
            edits: edits.iter().map(|path| touch(path)).collect(),
            reads: reads.iter().map(|path| touch(path)).collect(),
            turns: Vec::new(),
            events,
        }
    }

    fn find<'a>(result: &'a FrictionResult, path: &str) -> &'a FrictionFile {
        result
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("missing friction file {path}"))
    }

    #[test]
    fn active_time_caps_each_gap_at_300s() {
        // 12:00 -> 12:10 is 600s (capped to 300); 12:10 -> 12:15 is 300s.
        let events = vec![
            event("2026-07-01T12:00:00+00:00"),
            event("2026-07-01T12:10:00+00:00"),
            event("2026-07-01T12:15:00+00:00"),
        ];
        let facts = vec![session("s1", false, &["a.rs"], &[], events)];
        let result = friction(&facts, &opts());
        let file = find(&result, "a.rs");
        assert_eq!(file.read.sessions, 1);
        assert_eq!(file.read.median_active_seconds, 600);
        assert_eq!(file.read.p90_active_seconds, 600);
        assert_eq!(file.edit.sessions, 0);
        assert_eq!(file.edit.median_active_seconds, 0);
    }

    #[test]
    fn small_gaps_are_summed_uncapped() {
        let events = vec![
            event("2026-07-01T12:00:00+00:00"),
            event("2026-07-01T12:00:30+00:00"),
            event("2026-07-01T12:01:10+00:00"),
        ];
        let facts = vec![session("s1", false, &[], &["b.rs"], events)];
        let result = friction(&facts, &opts());
        let file = find(&result, "b.rs");
        // 30s + 40s of active gaps, neither hitting the cap.
        assert_eq!(file.edit.median_active_seconds, 70);
        assert_eq!(file.edit.sessions, 1);
        assert_eq!(file.read.sessions, 0);
    }

    #[test]
    fn read_and_edit_cohorts_are_independent() {
        let reader = session(
            "reader",
            false,
            &["shared.rs"],
            &[],
            vec![
                event("2026-07-01T12:00:00+00:00"),
                event("2026-07-01T12:00:40+00:00"),
            ],
        );
        let editor = session(
            "editor",
            false,
            &[],
            &["shared.rs"],
            vec![
                event("2026-07-01T12:00:00+00:00"),
                event("2026-07-01T12:02:00+00:00"),
            ],
        );
        let result = friction(&[reader, editor], &opts());
        let file = find(&result, "shared.rs");
        assert_eq!(file.read.sessions, 1);
        assert_eq!(file.read.median_active_seconds, 40);
        assert_eq!(file.edit.sessions, 1);
        assert_eq!(file.edit.median_active_seconds, 120);
    }

    #[test]
    fn coordinator_sessions_are_excluded() {
        let coordinator = session(
            "coord",
            true,
            &["z.rs"],
            &["z.rs"],
            vec![
                event("2026-07-01T12:00:00+00:00"),
                event("2026-07-01T12:04:00+00:00"),
            ],
        );
        let normal = session(
            "normal",
            false,
            &[],
            &["kept.rs"],
            vec![
                event("2026-07-01T12:00:00+00:00"),
                event("2026-07-01T12:00:30+00:00"),
            ],
        );
        let result = friction(&[coordinator, normal], &opts());
        assert!(
            result.files.iter().all(|file| file.path != "z.rs"),
            "coordinator-only file leaked into friction output"
        );
        find(&result, "kept.rs");
    }

    #[test]
    fn median_and_p90_use_nearest_rank_across_sessions() {
        // Five editors of shared.rs with active times 10,20,30,40,50 seconds.
        let deltas = [
            ("s10", "2026-07-01T12:00:10+00:00"),
            ("s20", "2026-07-01T12:00:20+00:00"),
            ("s30", "2026-07-01T12:00:30+00:00"),
            ("s40", "2026-07-01T12:00:40+00:00"),
            ("s50", "2026-07-01T12:00:50+00:00"),
        ];
        let facts: Vec<SessionFacts> = deltas
            .iter()
            .map(|(id, end)| {
                session(
                    id,
                    false,
                    &[],
                    &["shared.rs"],
                    vec![event("2026-07-01T12:00:00+00:00"), event(end)],
                )
            })
            .collect();
        let result = friction(&facts, &opts());
        let file = find(&result, "shared.rs");
        assert_eq!(file.edit.sessions, 5);
        // nearest-rank: median -> 30, p90 -> 50.
        assert_eq!(file.edit.median_active_seconds, 30);
        assert_eq!(file.edit.p90_active_seconds, 50);
    }

    #[test]
    fn files_sort_desc_by_metric_then_path() {
        let high = session(
            "high",
            false,
            &[],
            &["high.rs"],
            vec![
                event("2026-07-01T12:00:00+00:00"),
                event("2026-07-01T12:05:00+00:00"),
            ],
        );
        let low = session(
            "low",
            false,
            &[],
            &["low.rs"],
            vec![
                event("2026-07-01T12:00:00+00:00"),
                event("2026-07-01T12:00:30+00:00"),
            ],
        );
        let result = friction(&[low, high], &opts());
        assert_eq!(result.files[0].path, "high.rs");
        assert_eq!(result.files[1].path, "low.rs");
    }

    #[test]
    fn ties_break_by_path_ascending() {
        let make = |id: &str, path: &str| {
            session(
                id,
                false,
                &[],
                &[path],
                vec![
                    event("2026-07-01T12:00:00+00:00"),
                    event("2026-07-01T12:00:30+00:00"),
                ],
            )
        };
        let result = friction(&[make("b", "b.rs"), make("a", "a.rs")], &opts());
        assert_eq!(result.files[0].path, "a.rs");
        assert_eq!(result.files[1].path, "b.rs");
    }

    #[test]
    fn empty_facts_yield_empty_result() {
        let result = friction(&[], &opts());
        assert!(result.files.is_empty());
    }
}
