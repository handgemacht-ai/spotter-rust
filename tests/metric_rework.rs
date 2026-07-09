//! Unit-level tests for the rework metric (`spotter::metric_rework::rework`).
//!
//! Rework counts, per file, the number of *distinct logical sessions* that made
//! at least one edit of the file inside the metric window. These tests pin the
//! behaviours the frozen contract cares about: distinct-session counting,
//! badge-at-5, coordinator inclusion, Bash-mediated edits, window pruning, and
//! the `desc metric, then path` ordering.

use std::collections::BTreeSet;

use chrono::{TimeZone, Utc};
use spotter::metric_rework::rework;
use spotter::session_facts::{FileEvent, RelationsOptions, SessionFacts};

fn opts() -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap: 100,
    }
}

fn edit(path: &str, ts: Option<&str>) -> FileEvent {
    FileEvent {
        path: path.to_string(),
        ts: ts.map(str::to_string),
        message_id: None,
    }
}

/// A minimal session identified by `id` with the given edit events.
fn session(id: &str, is_coordinator: bool, edits: Vec<FileEvent>) -> SessionFacts {
    SessionFacts {
        external_session_id: id.to_string(),
        is_coordinator,
        rigs: BTreeSet::new(),
        edits,
        reads: vec![],
        turns: vec![],
        events: vec![],
    }
}

/// Look up the rework count for a path in a result, or `None` when absent.
fn sessions_for(result: &spotter::metric_rework::ReworkResult, path: &str) -> Option<usize> {
    result
        .files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.sessions)
}

#[test]
fn counts_distinct_sessions_not_edit_events() {
    // One session editing the same file three times counts once; a second
    // session editing it once brings the distinct total to two.
    let facts = vec![
        session(
            "sess-a",
            false,
            vec![
                edit("/rig/src/a.rs", Some("2026-06-30T10:00:00+00:00")),
                edit("/rig/src/a.rs", Some("2026-06-30T10:05:00+00:00")),
                edit("/rig/src/a.rs", Some("2026-06-30T10:10:00+00:00")),
            ],
        ),
        session(
            "sess-b",
            false,
            vec![edit("/rig/src/a.rs", Some("2026-06-29T09:00:00+00:00"))],
        ),
    ];
    let result = rework(&facts, &opts());
    assert_eq!(sessions_for(&result, "/rig/src/a.rs"), Some(2));
    let file = &result.files[0];
    assert!(!file.badge, "two sessions is below the badge threshold");
}

#[test]
fn badge_flips_at_five_distinct_sessions() {
    let four: Vec<SessionFacts> = (0..4)
        .map(|n| {
            session(
                &format!("sess-{n}"),
                false,
                vec![edit("/rig/hot.rs", Some("2026-06-20T10:00:00+00:00"))],
            )
        })
        .collect();
    let below = rework(&four, &opts());
    assert_eq!(sessions_for(&below, "/rig/hot.rs"), Some(4));
    assert!(!below.files[0].badge, "four sessions must not badge");

    let mut five = four;
    five.push(session(
        "sess-4",
        false,
        vec![edit("/rig/hot.rs", Some("2026-06-21T10:00:00+00:00"))],
    ));
    let at = rework(&five, &opts());
    assert_eq!(sessions_for(&at, "/rig/hot.rs"), Some(5));
    assert!(at.files[0].badge, "five sessions must badge");
}

#[test]
fn coordinator_sessions_still_count() {
    // The coordinator guard excludes coordinators from pairwise/friction metrics
    // but NOT from per-file scalars like rework.
    let facts = vec![
        session(
            "sess-coord",
            true,
            vec![edit("/rig/main.go", Some("2026-06-29T08:00:00+00:00"))],
        ),
        session(
            "sess-plain",
            false,
            vec![edit("/rig/main.go", Some("2026-06-30T08:00:00+00:00"))],
        ),
    ];
    let result = rework(&facts, &opts());
    assert_eq!(sessions_for(&result, "/rig/main.go"), Some(2));
}

#[test]
fn edits_outside_the_window_are_pruned() {
    // now = 2026-07-01, since_days = 30 -> window starts 2026-06-01.
    let facts = vec![
        session(
            "recent",
            false,
            vec![edit("/rig/src/x.rs", Some("2026-06-15T10:00:00+00:00"))],
        ),
        session(
            "stale",
            false,
            vec![edit("/rig/src/x.rs", Some("2026-04-01T10:00:00+00:00"))],
        ),
    ];
    let result = rework(&facts, &opts());
    assert_eq!(
        sessions_for(&result, "/rig/src/x.rs"),
        Some(1),
        "only the in-window session should count"
    );
}

#[test]
fn undated_edits_are_kept() {
    // A Bash-mediated write may lack a timestamp; the file list is already
    // mtime-pruned, so an undated edit is not dropped.
    let facts = vec![session(
        "sess-a",
        false,
        vec![edit("/rig/config.toml", None)],
    )];
    let result = rework(&facts, &opts());
    assert_eq!(sessions_for(&result, "/rig/config.toml"), Some(1));
}

#[test]
fn files_sorted_desc_by_sessions_then_path() {
    let facts = vec![
        session(
            "s1",
            false,
            vec![
                edit("/rig/z.rs", Some("2026-06-20T10:00:00+00:00")),
                edit("/rig/a.rs", Some("2026-06-20T10:00:00+00:00")),
                edit("/rig/b.rs", Some("2026-06-20T10:00:00+00:00")),
            ],
        ),
        session(
            "s2",
            false,
            vec![edit("/rig/a.rs", Some("2026-06-21T10:00:00+00:00"))],
        ),
    ];
    let result = rework(&facts, &opts());
    let order: Vec<(&str, usize)> = result
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.sessions))
        .collect();
    // a.rs has 2 sessions (desc first); b.rs and z.rs tie at 1, path ascending.
    assert_eq!(
        order,
        vec![("/rig/a.rs", 2), ("/rig/b.rs", 1), ("/rig/z.rs", 1)]
    );
}

#[test]
fn empty_facts_yield_empty_result() {
    let result = rework(&[], &opts());
    assert!(result.files.is_empty());
}
