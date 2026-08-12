//! Golden test for the `docs_steer` relations metric (S5).
//!
//! Builds a fixed [`SessionFacts`] set exercising the read-before-edit ordering,
//! the session-start auto-read exclusion, doc-target exclusion, and the
//! desc-sessions-then-path sort, then byte-compares the serialized result to the
//! checked-in golden. Regenerate with `SPOTTER_REGEN_GOLDEN=1`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use spotter::metric_docs_steer::docs_steer;
use spotter::session_facts::{
    CoordinationClass, RelationsOptions, SessionEvent, SessionEventKind, SessionFacts,
};

const GOLDEN: &str = "tests/golden/docs_steer/steer.json";
const ROOT: &str = "/srv/town/rig";

fn fixed_opts() -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap: 100,
    }
}

fn read(name: &str, success: bool) -> SessionEvent {
    SessionEvent {
        ts: None,
        kind: SessionEventKind::Read,
        path: Some(format!("{ROOT}/{name}")),
        success,
        message_id: None,
    }
}

fn edit(name: &str) -> SessionEvent {
    SessionEvent {
        ts: None,
        kind: SessionEventKind::Edit,
        path: Some(format!("{ROOT}/{name}")),
        success: true,
        message_id: None,
    }
}

const fn grep() -> SessionEvent {
    SessionEvent {
        ts: None,
        kind: SessionEventKind::Grep,
        path: None,
        success: true,
        message_id: None,
    }
}

fn session(id: &str, events: Vec<SessionEvent>) -> SessionFacts {
    SessionFacts {
        external_session_id: id.to_string(),
        coordination: CoordinationClass::Single,
        rigs: BTreeSet::new(),
        edits: Vec::new(),
        reads: Vec::new(),
        turns: Vec::new(),
        events,
    }
}

/// A fixed set of logical sessions covering every branch of the metric.
fn fixed_facts() -> Vec<SessionFacts> {
    vec![
        // Auto-read CLAUDE.md ignored; architecture.md steers app.rs; the doc
        // edit of architecture.md is not itself a steer target.
        session(
            "sess-alpha",
            vec![
                read("CLAUDE.md", true),
                read("docs/architecture.md", true),
                edit("src/app.rs"),
                edit("docs/architecture.md"),
            ],
        ),
        // architecture.md read at index 0 is NOT an auto-read name, so it steers
        // server.rs and lifts architecture.md to two distinct sessions.
        session(
            "sess-bravo",
            vec![
                read("docs/architecture.md", true),
                grep(),
                edit("src/server.rs"),
            ],
        ),
        // README auto-read at session start: no steer.
        session(
            "sess-charlie",
            vec![read("README.md", true), edit("src/only.rs")],
        ),
        // Doc read AFTER the only edit: no later code edit to steer.
        session(
            "sess-delta",
            vec![edit("src/x.rs"), read("docs/guide.md", true)],
        ),
        // Deliberate re-read of CLAUDE.md past the session-start prefix steers.
        session(
            "sess-echo",
            vec![
                read("CLAUDE.md", true),
                grep(),
                grep(),
                read("CLAUDE.md", true),
                edit("src/z.rs"),
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
    assert_eq!(actual, expected, "docs_steer result drifted from golden");
}

fn render() -> String {
    let result = docs_steer(&fixed_facts(), &fixed_opts());
    format!("{}\n", serde_json::to_string_pretty(&result).expect("json"))
}

#[test]
fn docs_steer_matches_golden() {
    assert_or_regen(&render());
}

#[test]
fn docs_steer_is_deterministic() {
    assert_eq!(render(), render(), "docs_steer output is not stable");
}
