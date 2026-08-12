//! Behavioural tests for the shared session-facts fold and its helpers.
//!
//! The metric modules each have their own golden/behaviour suites; this file
//! pins the plumbing that feeds them — [`build_session_facts`] and the
//! canonicalization, provenance, and Bash-write helpers — driving the fold from
//! a hand-built [`LeanStore`] so the branch-heavy edges (Bash-mediated writes,
//! per-turn token attribution, the coordinator guard, the cwd/rig noise filter,
//! `filter_under`, provenance recency, and git worktree folding) are exercised
//! directly rather than only through the CLI.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};
use spotter::db::{SessionRecord, ToolCallRun};
use spotter::scan::{LeanMessage, LeanStore};
use spotter::session_facts::{
    bash_write_targets, build_canonicalizer, build_provenance, build_session_facts, filter_under,
    Canonicalizer, CoordinationClass, FileEvent, RelationsOptions, SessionEvent, SessionEventKind,
    SessionFacts, TurnUsage,
};
use spotter::timestamp::Timestamp;

fn opts() -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap: 100,
    }
}

/// A fully-defaulted `Read` run under `/srv/town/rig`; tests override the fields
/// they exercise.
fn base_run(external: &str, session: &str, ordinal: i64) -> ToolCallRun {
    ToolCallRun {
        tool_use_id: format!("{session}-{ordinal}"),
        session_id: session.to_string(),
        external_session_id: external.to_string(),
        parent_session_id: None,
        is_subagent: false,
        agent_id: None,
        tool_name: "Read".to_string(),
        command: None,
        command_program: None,
        command_args: Vec::new(),
        command_fingerprint: None,
        input_summary: None,
        input_size: None,
        output_size: None,
        file_paths: Vec::new(),
        status: "ok".to_string(),
        started_at: Timestamp::parse(&format!(
            "2026-06-15T10:{:02}:00+00:00",
            ordinal.clamp(0, 59)
        )),
        finished_at: None,
        duration_ms: None,
        start_ordinal: Some(ordinal),
        end_ordinal: None,
        source_scope: None,
        error_content: None,
        project_alias: String::new(),
        worktree_name: None,
        canonical_cwd: Some("/srv/town/rig".to_string()),
        read_total_lines: None,
        read_lines: None,
        read_truncated: None,
    }
}

fn assistant_msg(
    session: &str,
    ordinal: i64,
    message_id: Option<&str>,
    input: Option<i64>,
    output: i64,
    cache_creation: i64,
) -> LeanMessage {
    LeanMessage {
        session_id: session.to_string(),
        external_session_id: session.to_string(),
        ordinal,
        message_id: message_id.map(str::to_string),
        role: Some("assistant".to_string()),
        timestamp: None,
        cwd: Some("/srv/town/rig".to_string()),
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: cache_creation,
        cache_read_input_tokens: 0,
    }
}

fn user_msg(session: &str, ordinal: i64) -> LeanMessage {
    LeanMessage {
        role: Some("user".to_string()),
        ..assistant_msg(session, ordinal, Some("u"), Some(1), 1, 1)
    }
}

fn session_record(id: &str, external: &str, cwd: &str) -> SessionRecord {
    SessionRecord {
        id: id.to_string(),
        external_session_id: external.to_string(),
        parent_session_id: None,
        is_subagent: false,
        agent_id: None,
        project_alias: String::new(),
        transcript_path: String::new(),
        cwd: Some(cwd.to_string()),
        slug: None,
        git_branch: None,
        version: None,
        started_at: None,
        ended_at: None,
        message_count: 0,
    }
}

fn fe(path: &str) -> FileEvent {
    FileEvent {
        path: path.to_string(),
        ts: None,
        message_id: None,
    }
}

fn se_path(path: Option<&str>) -> SessionEvent {
    SessionEvent {
        ts: None,
        kind: SessionEventKind::Other,
        path: path.map(str::to_string),
        success: true,
        message_id: None,
    }
}

fn se_ts(ts: &str) -> SessionEvent {
    SessionEvent {
        ts: Timestamp::parse(ts),
        kind: SessionEventKind::Other,
        path: None,
        success: true,
        message_id: None,
    }
}

/// Fold with an identity canonicalizer (no worktree map, no rig roots — every
/// working directory is "known").
fn build(store: &LeanStore) -> Vec<SessionFacts> {
    let canon = Canonicalizer::new(BTreeMap::new(), BTreeSet::new());
    build_session_facts(store, &canon, &opts())
}

#[test]
fn turn_usage_is_deduped_and_attributed_to_edited_files() {
    // The edit run at ordinal 0 maps to assistant turn "m1", so m1's per-file
    // set carries a.rs. A duplicate m1 message, a user message, an id-less
    // assistant message, and an assistant message without token usage must all
    // be dropped, leaving exactly one attributed turn.
    let mut edit = base_run("s", "s", 0);
    edit.tool_name = "Edit".to_string();
    edit.file_paths = vec!["/srv/town/rig/a.rs".to_string()];
    let messages = vec![
        assistant_msg("s", 0, Some("m1"), Some(10), 100, 50),
        assistant_msg("s", 1, Some("m1"), Some(10), 999, 999),
        user_msg("s", 2),
        assistant_msg("s", 3, None, Some(10), 7, 7),
        assistant_msg("s", 4, Some("m2"), None, 7, 7),
    ];
    let store = LeanStore {
        runs: vec![edit],
        messages,
        ..Default::default()
    };
    let facts = build(&store);
    assert_eq!(facts.len(), 1);
    assert_eq!(
        facts[0].turns.len(),
        1,
        "only the first m1 assistant turn with usage survives dedup and the skips"
    );
    let turn = &facts[0].turns[0];
    assert_eq!(turn.message_id, "m1");
    assert_eq!(
        turn.attributed_tokens, 150,
        "attributed = output(100) + cache_creation(50); cache_read is excluded"
    );
    assert_eq!(turn.files, vec!["/srv/town/rig/a.rs".to_string()]);
}

#[test]
fn bash_writes_are_attributed_and_failures_are_recorded_but_not_counted() {
    let mut sed = base_run("s", "s", 0);
    sed.tool_name = "Bash".to_string();
    sed.command = Some("sed -i s/x/y/ /srv/town/rig/main.rs".to_string());
    let mut plain = base_run("s", "s", 1);
    plain.tool_name = "Bash".to_string();
    plain.command = Some("cargo test --all".to_string());
    let mut failed = base_run("s", "s", 2);
    failed.tool_name = "Bash".to_string();
    failed.command = Some("tee /srv/town/rig/other.rs".to_string());
    failed.status = "error".to_string();

    let store = LeanStore {
        runs: vec![sed, plain, failed],
        ..Default::default()
    };
    let facts = build(&store);
    assert_eq!(facts.len(), 1);
    let session = &facts[0];
    let edited: Vec<&str> = session.edits.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        edited,
        vec!["/srv/town/rig/main.rs"],
        "only the successful sed write is attributed as an edit"
    );
    assert_eq!(session.events.len(), 3);
    assert_eq!(
        session.events[0].kind,
        SessionEventKind::Edit,
        "a Bash write is an Edit event"
    );
    assert_eq!(
        session.events[1].kind,
        SessionEventKind::Other,
        "a Bash command that writes nothing is an Other event"
    );
    assert!(
        !session.events[2].success,
        "the failed tee write is recorded as an event but not attributed"
    );
}

#[test]
fn four_distinct_cwds_flag_a_coordinator() {
    let dirs = ["/srv/a", "/srv/b", "/srv/c", "/srv/d"];
    let runs: Vec<ToolCallRun> = dirs
        .iter()
        .enumerate()
        .map(|(index, dir)| {
            let mut run = base_run("s", "s", i64::try_from(index).unwrap_or(0));
            run.tool_name = "Edit".to_string();
            run.file_paths = vec![format!("{dir}/f.rs")];
            run.canonical_cwd = Some((*dir).to_string());
            run
        })
        .collect();
    let store = LeanStore {
        runs,
        ..Default::default()
    };
    let facts = build(&store);
    assert_eq!(facts.len(), 1);
    assert!(
        facts[0].coordination.is_coordinator(),
        "four distinct working directories cross the >3-cwd coordinator threshold"
    );
}

#[test]
fn three_distinct_cwds_stay_below_the_coordinator_threshold() {
    let dirs = ["/srv/a", "/srv/b", "/srv/c"];
    let runs: Vec<ToolCallRun> = dirs
        .iter()
        .enumerate()
        .map(|(index, dir)| {
            let mut run = base_run("s", "s", i64::try_from(index).unwrap_or(0));
            run.tool_name = "Edit".to_string();
            run.file_paths = vec![format!("{dir}/f.rs")];
            run.canonical_cwd = Some((*dir).to_string());
            run
        })
        .collect();
    let store = LeanStore {
        runs,
        ..Default::default()
    };
    let facts = build(&store);
    assert_eq!(facts.len(), 1);
    assert!(
        !facts[0].coordination.is_coordinator(),
        "three working directories are below the coordinator threshold"
    );
}

#[test]
fn runs_without_any_working_directory_are_skipped() {
    let mut skip = base_run("s", "s", 0);
    skip.file_paths = vec!["/skip.rs".to_string()];
    skip.canonical_cwd = None; // no message, no session record, no canonical cwd
    let mut keep = base_run("s", "s", 1);
    keep.file_paths = vec!["/srv/town/rig/keep.rs".to_string()];

    let store = LeanStore {
        runs: vec![skip, keep],
        ..Default::default()
    };
    let facts = build(&store);
    assert_eq!(facts.len(), 1);
    let reads: Vec<&str> = facts[0].reads.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        reads,
        vec!["/srv/town/rig/keep.rs"],
        "a run with no resolvable cwd contributes nothing"
    );
}

#[test]
fn runs_outside_a_known_rig_are_dropped() {
    let canon = Canonicalizer::new(
        BTreeMap::new(),
        BTreeSet::from(["/srv/town/rig".to_string()]),
    );
    let mut outside = base_run("s", "s", 0);
    outside.file_paths = vec!["/other/x.rs".to_string()];
    outside.canonical_cwd = Some("/other/place".to_string());
    let mut inside = base_run("s", "s", 1);
    inside.file_paths = vec!["/srv/town/rig/x.rs".to_string()];
    inside.canonical_cwd = Some("/srv/town/rig".to_string());

    let store = LeanStore {
        runs: vec![outside, inside],
        ..Default::default()
    };
    let facts = build_session_facts(&store, &canon, &opts());
    assert_eq!(facts.len(), 1);
    let reads: Vec<&str> = facts[0].reads.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        reads,
        vec!["/srv/town/rig/x.rs"],
        "the known-rig noise filter drops the out-of-rig run"
    );
    assert!(
        !facts[0].coordination.is_coordinator(),
        "one surviving rig is not a coordinator"
    );
}

#[test]
fn filter_under_keeps_only_paths_beneath_the_root() {
    let inside = "/srv/town/rig/src/a.rs";
    let outside = "/srv/other/b.rs";
    let kept = SessionFacts {
        external_session_id: "keep".to_string(),
        coordination: CoordinationClass::Single,
        rigs: BTreeSet::new(),
        edits: vec![fe(inside), fe(outside)],
        reads: vec![fe(outside)],
        turns: vec![TurnUsage {
            message_id: "m".to_string(),
            attributed_tokens: 10,
            files: vec![inside.to_string(), outside.to_string()],
        }],
        events: vec![se_path(Some(inside)), se_path(Some(outside)), se_path(None)],
    };
    let dropped = SessionFacts {
        external_session_id: "drop".to_string(),
        coordination: CoordinationClass::Single,
        rigs: BTreeSet::new(),
        edits: vec![fe(outside)],
        reads: Vec::new(),
        turns: Vec::new(),
        events: vec![se_path(Some(outside))],
    };

    let result = filter_under(vec![kept, dropped], "/srv/town/rig");
    assert_eq!(
        result.len(),
        1,
        "the session with nothing under the root is dropped entirely"
    );
    let session = &result[0];
    assert_eq!(
        session
            .edits
            .iter()
            .map(|e| e.path.as_str())
            .collect::<Vec<_>>(),
        vec![inside]
    );
    assert!(
        session.reads.is_empty(),
        "the only read was outside the root"
    );
    assert_eq!(session.turns[0].files, vec![inside.to_string()]);
    assert_eq!(
        session.events.len(),
        2,
        "the path-less event is kept for active-time context; the outside-path event is dropped"
    );
    assert!(session.events.iter().any(|event| event.path.is_none()));
    assert!(session
        .events
        .iter()
        .all(|event| event.path.as_deref() != Some(outside)));
}

#[test]
fn provenance_orders_by_recency_then_caps_at_twenty() {
    let file = "/srv/town/rig/hot.rs";
    // Each session both edits and reads the file (two touches), so the recency
    // fold sees the "already-recorded, not newer" branch as well as the first.
    let touch_session = |id: &str, ts: &str| SessionFacts {
        external_session_id: id.to_string(),
        coordination: CoordinationClass::Single,
        rigs: BTreeSet::new(),
        edits: vec![fe(file)],
        reads: vec![fe(file)],
        turns: Vec::new(),
        events: vec![se_ts(ts)],
    };

    let prov = build_provenance(&[
        touch_session("older", "2026-06-01T10:00:00+00:00"),
        touch_session("newer", "2026-06-02T10:00:00+00:00"),
    ]);
    let entry = prov
        .iter()
        .find(|p| p.path == file)
        .expect("touched file present in provenance");
    assert_eq!(
        entry.session_ids,
        vec!["newer".to_string(), "older".to_string()],
        "most-recent session id comes first"
    );

    let many: Vec<SessionFacts> = (0..21)
        .map(|index| touch_session(&format!("s{index:02}"), "2026-06-01T10:00:00+00:00"))
        .collect();
    let capped = build_provenance(&many);
    let hot = capped
        .iter()
        .find(|p| p.path == file)
        .expect("touched file present in provenance");
    assert_eq!(
        hot.session_ids.len(),
        20,
        "provenance keeps at most twenty session ids per file"
    );
}

#[test]
fn canonicalizer_folds_worktree_map_and_strips_segments() {
    let mut map = BTreeMap::new();
    map.insert("/wt/x".to_string(), "/repo".to_string());
    let canon = Canonicalizer::new(map, BTreeSet::new());

    // An exact prefix match resolves to the mapped root itself.
    assert_eq!(canon.canonical("/wt/x"), "/repo");
    // A prefix plus a '/'-separated remainder is rebased onto the root.
    assert_eq!(canon.canonical("/wt/x/src/a.rs"), "/repo/src/a.rs");
    // A prefix that is not on a path boundary is left untouched.
    assert_eq!(canon.canonical("/wt/xtra/a.rs"), "/wt/xtra/a.rs");
    // An in-repo worktree marker with no trailing component is left as-is.
    assert_eq!(
        canon.canonical("/repo/.claude/worktrees/wt"),
        "/repo/.claude/worktrees/wt"
    );
    // A complete in-repo worktree segment folds back onto the main tree.
    assert_eq!(
        canon.canonical("/repo/.claude/worktrees/wt/src/a.rs"),
        "/repo/src/a.rs"
    );
}

#[test]
fn bash_write_targets_covers_fd_redirects_tee_flags_and_rejections() {
    // File-descriptor redirect forms.
    assert_eq!(
        bash_write_targets("build 1> logs/out.txt"),
        vec!["logs/out.txt"]
    );
    assert_eq!(
        bash_write_targets("build 1>> logs/out.txt"),
        vec!["logs/out.txt"]
    );
    // tee skips flags and stops at a shell separator.
    assert_eq!(
        bash_write_targets("printf x | tee -a keep.txt ; echo done"),
        vec!["keep.txt"]
    );
    // Redirect targets that are shell variables or flags are not paths.
    assert!(bash_write_targets("echo hi > $OUT").is_empty());
    assert!(bash_write_targets("echo hi > -n").is_empty());
    // A bare filename with an alphanumeric extension still looks like a path.
    assert_eq!(bash_write_targets("echo x > notes.log"), vec!["notes.log"]);
}

#[test]
fn build_canonicalizer_learns_worktree_map_from_git() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let main = temp.path().join("main");
    std::fs::create_dir_all(&main).expect("create main dir");

    let git = |args: &[&str], cwd: &std::path::Path| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };

    git(&["init", "-q", "-b", "main"], &main);
    std::fs::write(main.join("f.txt"), "x").expect("seed file");
    git(&["add", "."], &main);
    git(&["commit", "-q", "-m", "init"], &main);
    let wt = temp.path().join("wt");
    git(
        &[
            "worktree",
            "add",
            "-q",
            wt.to_str().unwrap(),
            "-b",
            "feature",
        ],
        &main,
    );

    // Learn the exact path strings git reports, so the assertions do not depend
    // on symlink resolution of the temp directory.
    let listing = std::process::Command::new("git")
        .args([
            "-C",
            wt.to_str().unwrap(),
            "worktree",
            "list",
            "--porcelain",
        ])
        .output()
        .expect("worktree list");
    let text = String::from_utf8_lossy(&listing.stdout);
    let paths: Vec<String> = text
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(ToString::to_string)
        .collect();
    let main_git = paths[0].clone();
    let wt_git = paths
        .iter()
        .find(|path| **path != main_git)
        .expect("linked worktree present")
        .clone();

    let store = LeanStore {
        sessions: vec![session_record("s", "s", &wt_git)],
        ..Default::default()
    };
    let canon = build_canonicalizer(&store);

    assert_eq!(
        canon.canonical(&format!("{wt_git}/src/a.rs")),
        format!("{main_git}/src/a.rs"),
        "the linked worktree path folds onto the main tree"
    );
    assert!(
        canon.known(&main_git),
        "the main tree is registered as a known rig root"
    );
    assert_eq!(
        canon.rig_for(&format!("{main_git}/src/a.rs")),
        Some(main_git.as_str())
    );
}
