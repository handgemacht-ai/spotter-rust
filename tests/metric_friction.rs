//! Golden and determinism tests for the friction metric.
//!
//! The golden test builds a fixed [`SessionFacts`] set and byte-compares the
//! serialized [`friction`] result; regenerate with `SPOTTER_REGEN_GOLDEN=1`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use spotter::metric_friction::friction;
use spotter::session_facts::{
    CoordinationClass, FileEvent, RelationsOptions, SessionEvent, SessionEventKind, SessionFacts,
};
use spotter::timestamp::Timestamp;

const GOLDEN: &str = "tests/golden/metric_friction/friction.json";

fn fixed_opts() -> RelationsOptions {
    use chrono::{TimeZone, Utc};
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

/// A fixed cohort mix: two editors of `core.rs` (one hitting the 300s gap cap),
/// two readers of `guide.md`, and a coordinator that must be excluded.
fn fixed_facts() -> Vec<SessionFacts> {
    vec![
        session(
            "s-doc",
            false,
            &["guide.md"],
            &[],
            vec![
                event("2026-06-30T12:00:00+00:00"),
                event("2026-06-30T12:03:00+00:00"),
            ],
        ),
        session(
            "s-edit-long",
            false,
            &[],
            &["core.rs"],
            vec![
                event("2026-06-30T12:00:00+00:00"),
                event("2026-06-30T12:10:00+00:00"),
            ],
        ),
        session(
            "s-edit-short",
            false,
            &["guide.md"],
            &["core.rs"],
            vec![
                event("2026-06-30T12:00:00+00:00"),
                event("2026-06-30T12:01:00+00:00"),
            ],
        ),
        session(
            "s-coord",
            true,
            &["guide.md"],
            &["core.rs"],
            vec![
                event("2026-06-29T08:00:00+00:00"),
                event("2026-06-29T08:04:00+00:00"),
            ],
        ),
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
    assert_eq!(actual, expected, "friction result drifted from golden");
}

#[test]
fn friction_matches_golden() {
    let result = friction(&fixed_facts(), &fixed_opts());
    let rendered = format!("{}\n", serde_json::to_string_pretty(&result).expect("json"));
    assert_or_regen(&rendered);
}

#[test]
fn friction_is_deterministic() {
    let opts = fixed_opts();
    let first = serde_json::to_vec(&friction(&fixed_facts(), &opts)).expect("json");
    let second = serde_json::to_vec(&friction(&fixed_facts(), &opts)).expect("json");
    assert_eq!(first, second, "friction serialization is not byte-stable");
}
