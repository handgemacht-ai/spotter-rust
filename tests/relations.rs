//! Golden, determinism, and CLI-surface tests for `spotter scan relations`.
//!
//! The golden test builds a fixed [`SessionFacts`] set and byte-compares the
//! frozen envelope; regenerate with `SPOTTER_REGEN_GOLDEN=1`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use chrono::{TimeZone, Utc};
use serde_json::Value;
use spotter::cli::build_relations_envelope;
use spotter::session_facts::{
    CoordinationClass, FileEvent, RelationsOptions, SessionEvent, SessionEventKind, SessionFacts,
    TurnUsage,
};
use spotter::timestamp::Timestamp;
use tempfile::NamedTempFile;

const FIXTURE_ROOT: &str = "tests/fixtures/transcripts";
const GOLDEN: &str = "tests/golden/scan_relations/envelope.json";

fn fixed_opts() -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap: 100,
    }
}

/// A fixed set of logical sessions: one single-rig editing session, one
/// coordinator spanning two rigs, and one doc-reading session.
fn fixed_facts() -> Vec<SessionFacts> {
    let edit = |path: &str, ts: &str, message: &str| FileEvent {
        path: path.to_string(),
        ts: Timestamp::parse(ts),
        message_id: Some(message.to_string()),
    };
    vec![
        SessionFacts {
            external_session_id: "sess-alpha".to_string(),
            coordination: CoordinationClass::Single,
            rigs: BTreeSet::from(["/srv/town/rig-a".to_string()]),
            edits: vec![
                edit(
                    "/srv/town/rig-a/src/lib.rs",
                    "2026-06-30T10:00:00+00:00",
                    "msg-1",
                ),
                edit(
                    "/srv/town/rig-a/src/cli.rs",
                    "2026-06-30T10:05:00+00:00",
                    "msg-2",
                ),
            ],
            reads: vec![FileEvent {
                path: "/srv/town/rig-a/README.md".to_string(),
                ts: Timestamp::parse("2026-06-30T09:59:00+00:00"),
                message_id: Some("msg-0".to_string()),
            }],
            turns: vec![
                TurnUsage {
                    message_id: "msg-1".to_string(),
                    attributed_tokens: 1200,
                    files: vec!["/srv/town/rig-a/src/lib.rs".to_string()],
                },
                TurnUsage {
                    message_id: "msg-2".to_string(),
                    attributed_tokens: 800,
                    files: vec!["/srv/town/rig-a/src/cli.rs".to_string()],
                },
            ],
            events: vec![
                SessionEvent {
                    ts: Timestamp::parse("2026-06-30T09:59:00+00:00"),
                    kind: SessionEventKind::Read,
                    path: Some("/srv/town/rig-a/README.md".to_string()),
                    success: true,
                    message_id: Some("msg-0".to_string()),
                },
                SessionEvent {
                    ts: Timestamp::parse("2026-06-30T10:00:00+00:00"),
                    kind: SessionEventKind::Edit,
                    path: Some("/srv/town/rig-a/src/lib.rs".to_string()),
                    success: true,
                    message_id: Some("msg-1".to_string()),
                },
            ],
        },
        SessionFacts {
            external_session_id: "sess-coord".to_string(),
            coordination: CoordinationClass::MultiRig,
            rigs: BTreeSet::from(["/srv/town/rig-a".to_string(), "/srv/town/rig-b".to_string()]),
            edits: vec![edit(
                "/srv/town/rig-b/main.go",
                "2026-06-29T08:00:00+00:00",
                "msg-9",
            )],
            reads: vec![],
            turns: vec![TurnUsage {
                message_id: "msg-9".to_string(),
                attributed_tokens: 400,
                files: vec!["/srv/town/rig-b/main.go".to_string()],
            }],
            events: vec![SessionEvent {
                ts: Timestamp::parse("2026-06-29T08:00:00+00:00"),
                kind: SessionEventKind::Edit,
                path: Some("/srv/town/rig-b/main.go".to_string()),
                success: true,
                message_id: Some("msg-9".to_string()),
            }],
        },
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
    assert_eq!(actual, expected, "relations envelope drifted from golden");
}

#[test]
fn envelope_matches_golden() {
    let facts = fixed_facts();
    let opts = fixed_opts();
    let envelope = build_relations_envelope(&facts, &opts);
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(&envelope).expect("json")
    );
    assert_or_regen(&rendered);
}

#[test]
fn envelope_is_deterministic() {
    let facts = fixed_facts();
    let opts = fixed_opts();
    let first = serde_json::to_vec(&build_relations_envelope(&facts, &opts)).expect("json");
    let second = serde_json::to_vec(&build_relations_envelope(&facts, &opts)).expect("json");
    assert_eq!(first, second, "envelope serialization is not byte-stable");
}

fn run_relations(args: &[&str]) -> Value {
    let db = NamedTempFile::new().expect("temp db");
    let config = NamedTempFile::new().expect("temp config");
    let output = Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--db", db.path().to_str().unwrap()])
        .args(["--config", config.path().to_str().unwrap()])
        .args(["scan", "--root", FIXTURE_ROOT, "relations"])
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid json envelope")
}

#[test]
fn cli_relations_emits_frozen_envelope() {
    let envelope = run_relations(&["--since", "0", "--format", "json"]);
    for key in [
        "generated_at",
        "since_days",
        "fanout_cap",
        "session_count",
        "coordinator_count",
        "cochange_session",
        "read_clusters",
        "rework",
        "friction",
        "cost",
        "docs_steer",
        "discoverability",
        "provenance",
    ] {
        assert!(envelope.get(key).is_some(), "envelope missing key {key}");
    }
    assert!(envelope["cochange_session"]["pairs"].is_array());
    assert!(envelope["read_clusters"]["clusters"].is_array());
    assert_eq!(envelope["session_count"], 4);
    assert!(
        !envelope["provenance"].as_array().expect("array").is_empty(),
        "provenance should be populated from fixtures"
    );
}

#[test]
fn relations_flags_round_trip() {
    let envelope = run_relations(&["--since", "7", "--fanout-cap", "42", "--format", "json"]);
    assert_eq!(envelope["since_days"], 7);
    assert_eq!(envelope["fanout_cap"], 42);
}

#[test]
fn cli_relations_table_summary_renders_counts() {
    let db = NamedTempFile::new().expect("temp db");
    let config = NamedTempFile::new().expect("temp config");
    let output = Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--db", db.path().to_str().unwrap()])
        .args(["--config", config.path().to_str().unwrap()])
        .args(["scan", "--root", FIXTURE_ROOT, "relations"])
        .args(["--since", "0", "--format", "table"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf8 summary");
    assert!(
        text.contains("Relations ("),
        "table summary is missing its header: {text}"
    );
    for label in [
        "cochange_session pairs",
        "read_clusters clusters",
        "rework files",
        "friction files",
        "cost files",
        "docs_steer docs",
        "discoverability files",
        "provenance files",
    ] {
        assert!(
            text.contains(label),
            "table summary missing line {label}: {text}"
        );
    }
}

#[test]
fn relations_under_filters_paths() {
    let envelope = run_relations(&["--since", "0", "--under", "assets", "--format", "json"]);
    let provenance = envelope["provenance"].as_array().expect("array");
    assert!(
        provenance.iter().all(|entry| entry["path"]
            .as_str()
            .unwrap_or_default()
            .starts_with("assets")),
        "under filter leaked non-assets paths: {provenance:?}"
    );
    assert!(
        !provenance.is_empty(),
        "assets/js/app.js should survive the filter"
    );
}
