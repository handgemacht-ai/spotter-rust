//! Behavioural tests for the DB-less scan loaders that feed the relations pass.
//!
//! Covers the file-collection, error-tolerant loading, mtime windowing, and
//! session-lookup paths of [`spotter::scan`] directly, using the checked-in
//! transcript fixtures so the parse path is real rather than mocked.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use spotter::config::Config;
use spotter::db::SessionRecord;
use spotter::scan::{audit_file, collect_targets, load, load_lean, plain_text_of, Store};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transcripts")
}

fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

fn record(id: &str, external: &str, is_subagent: bool) -> SessionRecord {
    SessionRecord {
        id: id.to_string(),
        external_session_id: external.to_string(),
        parent_session_id: None,
        is_subagent,
        agent_id: None,
        project_alias: String::new(),
        transcript_path: String::new(),
        cwd: None,
        slug: None,
        git_branch: None,
        version: None,
        started_at: None,
        ended_at: None,
        message_count: 0,
    }
}

#[test]
fn load_lean_prunes_files_older_than_the_window() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let aged = temp.path().join("old.jsonl");
    std::fs::copy(fixture("short.jsonl"), &aged).expect("copy fixture");
    let old = SystemTime::now() - Duration::from_secs(100 * 86_400);
    std::fs::File::options()
        .write(true)
        .open(&aged)
        .expect("open aged file")
        .set_modified(old)
        .expect("set mtime");

    let config = Config::default();
    let cancel = AtomicBool::new(false);
    let targets = vec![aged];

    let pruned = load_lean(&targets, &config, &cancel, 30).expect("load_lean with window");
    assert!(
        pruned.sessions.is_empty(),
        "a file whose mtime predates the 30-day window is pruned"
    );

    let unpruned = load_lean(&targets, &config, &cancel, 0).expect("load_lean without window");
    assert!(
        !unpruned.sessions.is_empty(),
        "since_days=0 disables mtime pruning and the transcript loads"
    );
}

#[test]
fn load_records_parse_errors_without_aborting() {
    let config = Config::default();
    let cancel = AtomicBool::new(false);
    let targets = vec![
        fixture("short.jsonl"),
        fixtures_dir().join("does-not-exist.jsonl"),
    ];
    let store = load(&targets, &config, &cancel).expect("load");
    assert!(
        !store.sessions.is_empty(),
        "the valid transcript is still parsed"
    );
    assert_eq!(
        store.errors.len(),
        1,
        "the unreadable file is collected as an error, not fatal"
    );
}

#[test]
fn collect_targets_dedups_explicit_files() {
    let config = Config::default();
    let file = fixture("short.jsonl");
    let targets = collect_targets(&[file.clone(), file.clone()], &[], false, &config);
    assert_eq!(
        targets,
        vec![file],
        "duplicate explicit files collapse to a single target"
    );
}

#[test]
fn collect_targets_walks_roots_and_honors_no_subagents() {
    let config = Config::default();
    let root = fixtures_dir();
    let with_subagents = collect_targets(&[], &[root.clone()], false, &config);
    let without_subagents = collect_targets(&[], &[root], true, &config);
    assert!(
        !with_subagents.is_empty(),
        "walking the fixture root finds transcripts"
    );
    assert!(
        without_subagents.len() <= with_subagents.len(),
        "--no-subagents never adds targets beyond the full walk"
    );
}

#[test]
fn collect_targets_falls_back_to_configured_roots() {
    let config = Config {
        transcript_roots: vec![fixtures_dir()],
        ..Default::default()
    };
    let targets = collect_targets(&[], &[], false, &config);
    assert!(
        !targets.is_empty(),
        "with no CLI files or roots, the configured transcript roots are walked"
    );
}

#[test]
fn find_session_prefers_main_over_subagent() {
    let store = Store {
        sessions: vec![
            record("ext:agent:a", "ext", true),
            record("ext", "ext", false),
        ],
        ..Default::default()
    };
    let found = store.find_session("ext").expect("session resolved by id");
    assert!(
        !found.is_subagent,
        "a main session is preferred over a subagent sidecar sharing the external id"
    );
    assert!(
        store.find_session("nope").is_none(),
        "an unknown id resolves to nothing"
    );
}

#[test]
fn plain_text_of_flattens_content_blocks() {
    assert_eq!(plain_text_of(&serde_json::json!("hello")), "hello");
    let blocks = serde_json::json!([
        { "type": "text", "text": "one" },
        { "type": "text", "text": "two" },
    ]);
    assert_eq!(plain_text_of(&blocks), "one\ntwo");
}

#[test]
fn audit_file_reports_line_and_message_counts() {
    let report = audit_file(&fixture("short.jsonl")).expect("audit short.jsonl");
    assert!(report.jsonl_lines > 0, "the fixture has JSONL lines");
    assert!(
        report.parsed_messages > 0,
        "the fixture parses into messages"
    );
    assert!(
        !report.jsonl_types.is_empty(),
        "the fixture has at least one message type"
    );
}
