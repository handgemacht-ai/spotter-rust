//! Golden, determinism, and behavioral tests for the discoverability metric.
//!
//! The golden test builds a fixed [`SessionFacts`] set exercising the
//! pre-first-read search counting and byte-compares the serialized result;
//! regenerate with `SPOTTER_REGEN_GOLDEN=1`.

// Median costs are medians of integer counts, so they are exact integers or
// half-integers — both exactly representable in `f64`. Exact `==` assertions are
// intentional and correct here.
#![allow(clippy::float_cmp)]

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use spotter::metric_discoverability::discoverability;
use spotter::session_facts::{
    CoordinationClass, RelationsOptions, SessionEvent, SessionEventKind, SessionFacts,
};

const GOLDEN: &str = "tests/golden/metric_discoverability/result.json";

const HIDDEN: &str = "/srv/town/rig-a/src/deep/hidden.rs";
const EASY: &str = "/srv/town/rig-a/src/easy.rs";
const MID: &str = "/srv/town/rig-a/src/mid.rs";
const RARE: &str = "/srv/town/rig-a/src/rare.rs";

fn opts() -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap: 100,
    }
}

fn ev(kind: SessionEventKind, path: Option<&str>, success: bool) -> SessionEvent {
    SessionEvent {
        ts: None,
        kind,
        path: path.map(str::to_string),
        success,
        message_id: None,
    }
}

fn read(path: &str, success: bool) -> SessionEvent {
    ev(SessionEventKind::Read, Some(path), success)
}

fn grep() -> SessionEvent {
    ev(SessionEventKind::Grep, None, true)
}

fn glob() -> SessionEvent {
    ev(SessionEventKind::Glob, None, true)
}

fn session(id: &str, is_coordinator: bool, events: Vec<SessionEvent>) -> SessionFacts {
    SessionFacts {
        external_session_id: id.to_string(),
        coordination: if is_coordinator {
            CoordinationClass::MultiRig
        } else {
            CoordinationClass::Single
        },
        rigs: BTreeSet::new(),
        edits: Vec::new(),
        reads: Vec::new(),
        turns: Vec::new(),
        events,
    }
}

/// Fixed sessions producing three above-threshold files (HIDDEN, MID, EASY) and
/// one excluded file (RARE, only two successful-read sessions).
///
/// Expected per-file costs (search effort = Grep/Glob + failed reads before the
/// first successful read of the file):
///   HIDDEN: s1=2, s2=2, s3=0  -> median 2.0 (3 sessions)
///   EASY:   s1=2, s2=0, s3=0  -> median 0.0 (3 sessions)
///   MID:    s3=2, s4=1, s5=4, s6=0 -> median 1.5 (4 sessions)
///   RARE:   s4=1, s5=4  (s2's read of RARE fails) -> 2 sessions, excluded
fn fixed_facts() -> Vec<SessionFacts> {
    vec![
        session(
            "sess-1",
            false,
            vec![grep(), glob(), read(HIDDEN, true), read(EASY, true)],
        ),
        session(
            "sess-2",
            false,
            vec![
                read(EASY, true),
                grep(),
                read(RARE, false),
                read(HIDDEN, true),
            ],
        ),
        session(
            "sess-3",
            false,
            vec![
                read(HIDDEN, true),
                read(EASY, true),
                glob(),
                glob(),
                read(MID, true),
            ],
        ),
        session(
            "sess-4",
            false,
            vec![grep(), read(MID, true), read(RARE, true)],
        ),
        session(
            "sess-5",
            false,
            vec![
                grep(),
                grep(),
                grep(),
                grep(),
                read(MID, true),
                read(RARE, true),
            ],
        ),
        session("sess-6", false, vec![read(MID, true)]),
    ]
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN)
}

fn assert_or_regen(actual: &str) {
    let path = golden_path();
    if std::env::var_os("SPOTTER_REGEN_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create golden dir");
        }
        fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden {}; regenerate with SPOTTER_REGEN_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "discoverability result drifted from golden"
    );
}

#[test]
fn discoverability_matches_golden() {
    let result = discoverability(&fixed_facts(), &opts());
    let rendered = format!("{}\n", serde_json::to_string_pretty(&result).expect("json"));
    assert_or_regen(&rendered);
}

#[test]
fn discoverability_is_deterministic() {
    let facts = fixed_facts();
    let first = serde_json::to_vec(&discoverability(&facts, &opts())).expect("json");
    let second = serde_json::to_vec(&discoverability(&facts, &opts())).expect("json");
    assert_eq!(
        first, second,
        "discoverability serialization is not byte-stable"
    );
}

#[test]
fn ranks_desc_by_cost_then_path() {
    let result = discoverability(&fixed_facts(), &opts());
    let ranked: Vec<(&str, usize, f64)> = result
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.sessions, file.median_cost))
        .collect();
    assert_eq!(
        ranked,
        vec![(HIDDEN, 3, 2.0), (MID, 4, 1.5), (EASY, 3, 0.0)],
        "unexpected ranking / values (RARE must be excluded at 2 sessions)"
    );
}

#[test]
fn excludes_files_below_three_sessions() {
    let result = discoverability(&fixed_facts(), &opts());
    assert!(
        result.files.iter().all(|file| file.path != RARE),
        "RARE has only two successful-read sessions and must be excluded"
    );
}

#[test]
fn failed_read_counts_as_search_effort() {
    // Three sessions: each does one failed read before the successful read, so
    // every per-session cost is 1 and the median is 1.0.
    let facts: Vec<SessionFacts> = (0..3)
        .map(|i| {
            session(
                &format!("s{i}"),
                false,
                vec![read(HIDDEN, false), read(HIDDEN, true)],
            )
        })
        .collect();
    let result = discoverability(&facts, &opts());
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].path, HIDDEN);
    assert_eq!(result.files[0].median_cost, 1.0);
}

#[test]
fn first_successful_read_freezes_cost() {
    // The cost is frozen at the FIRST successful read; later greps and a second
    // read of the same file do not raise it.
    let facts: Vec<SessionFacts> = (0..3)
        .map(|i| {
            session(
                &format!("s{i}"),
                false,
                vec![read(HIDDEN, true), grep(), grep(), read(HIDDEN, true)],
            )
        })
        .collect();
    let result = discoverability(&facts, &opts());
    assert_eq!(result.files[0].median_cost, 0.0);
}

#[test]
fn coordinator_sessions_are_counted() {
    // Two normal sessions plus one coordinator session all read the file: the
    // coordinator's scalar contribution keeps it at the >= 3 threshold.
    let facts = vec![
        session("normal-a", false, vec![grep(), read(HIDDEN, true)]),
        session("normal-b", false, vec![grep(), read(HIDDEN, true)]),
        session("coord", true, vec![read(HIDDEN, true)]),
    ];
    let result = discoverability(&facts, &opts());
    assert_eq!(
        result.files.len(),
        1,
        "coordinator must count toward sessions"
    );
    assert_eq!(result.files[0].sessions, 3);
    // Costs [1, 1, 0] -> median 1.0.
    assert_eq!(result.files[0].median_cost, 1.0);
}

#[test]
fn empty_facts_yield_empty_result() {
    let result = discoverability(&[], &opts());
    assert!(result.files.is_empty());
}
