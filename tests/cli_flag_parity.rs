#![allow(clippy::too_many_lines)]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;

const FIXTURE_ROOT: &str = "tests/fixtures/transcripts";
const TOOL_HEAVY: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const SHORT: &str = "tests/fixtures/transcripts/short.jsonl";
const DISCOVERABLE_SESSION: &str = "258c7280-ae70-4798-800f-63464d01a85d";
const TOOL_SESSION: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";
const SHORT_SESSION: &str = "55604662-cf2a-4331-851a-ec234028f8ca";

#[test]
fn carried_over_flags_accept_representative_values() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");
    let config = NamedTempFile::new().expect("temp config");
    let config_path = config.path().to_str().expect("utf8 temp path");

    assert_success(
        db_path,
        config_path,
        &["init", "--claude-projects", FIXTURE_ROOT, "--yes"],
    );
    assert_success(
        db_path,
        config_path,
        &["transcripts", "sync", "--session", DISCOVERABLE_SESSION],
    );
    assert_success(
        db_path,
        config_path,
        &["transcripts", "sync", "--file", TOOL_HEAVY],
    );
    assert_success(
        db_path,
        config_path,
        &["transcripts", "sync", "--file", SHORT],
    );

    let grouped_search = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "search",
            "--project",
            "spotter",
            "--worktree",
            "spotter",
            "--session",
            TOOL_SESSION,
            "--tool",
            "Bash",
            "--command-contains",
            "mix",
            "--min-duration",
            "1",
            "--max-duration",
            "2000",
            "--status",
            "completed",
            "--limit",
            "10",
            "--group-by-session",
            "--format",
            "json",
        ],
    );
    assert_eq!(grouped_search[0]["matches"], 1);

    let file_path_search = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "search",
            "--file-path",
            "assets/js/app.js",
            "--format",
            "json",
        ],
    );
    assert_eq!(file_path_search[0]["tool_name"], "Read");

    let error_search = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "search",
            "--error-contains",
            "rejected",
            "--status",
            "error",
            "--format",
            "json",
        ],
    );
    assert!(!error_search.as_array().expect("array").is_empty());

    let content_search = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "search",
            "--content-contains",
            "phoenix",
            "--limit",
            "2",
            "--format",
            "json",
        ],
    );
    assert!(!content_search.as_array().expect("array").is_empty());

    let inspect = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "inspect",
            "--session",
            TOOL_SESSION,
            "--tool-use-id",
            "toolu_01FZ9TWbiy1LPzHm1AAyUazo",
            "--context",
            "1",
            "--status",
            "completed",
            "--with-messages",
            "--format",
            "json",
        ],
    );
    assert!(!inspect["runs"].as_array().expect("runs").is_empty());
    assert!(!inspect["context"].as_array().expect("context").is_empty());

    let compare = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "compare",
            "--left-session",
            TOOL_SESSION,
            "--right-session",
            SHORT_SESSION,
            "--tool",
            "Bash",
            "--command-contains",
            "mix",
            "--group-by",
            "status",
            "--format",
            "json",
        ],
    );
    assert!(!compare["left"].as_array().expect("left").is_empty());
    assert!(!compare["right"].as_array().expect("right").is_empty());

    let repeatable_compare = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "compare",
            "--left-session",
            TOOL_SESSION,
            "--left-session",
            SHORT_SESSION,
            "--right-session",
            TOOL_SESSION,
            "--group-by",
            "status",
            "--format",
            "json",
        ],
    );
    assert!(sum_counts(&repeatable_compare["left"]) > sum_counts(&repeatable_compare["right"]));
    assert!(repeatable_compare["left"]
        .as_array()
        .expect("left")
        .iter()
        .any(|group| group["key"] == "error"));

    let aggregate = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "aggregate",
            "--project",
            "spotter",
            "--since",
            "2026-02-01",
            "--tool",
            "Bash",
            "--group-by",
            "tool_name,status",
            "--format",
            "json",
        ],
    );
    assert!(aggregate["total_runs"].as_u64().expect("total runs") > 0);

    assert!(
        command_json(
            db_path,
            config_path,
            &[
                "transcripts",
                "audit",
                "--file",
                TOOL_HEAVY,
                "--format",
                "json"
            ]
        )
        .as_array()
        .expect("file audit")
        .len()
            == 1
    );
    assert!(
        command_json(
            db_path,
            config_path,
            &[
                "transcripts",
                "audit",
                "--session",
                TOOL_SESSION,
                "--format",
                "json",
            ],
        )
        .as_array()
        .expect("session audit")
        .len()
            == 1
    );
    assert!(
        command_json(
            db_path,
            config_path,
            &[
                "transcripts",
                "audit",
                "--project",
                "spotter",
                "--limit",
                "2",
                "--format",
                "json",
            ],
        )
        .as_array()
        .expect("project audit")
        .len()
            <= 2
    );

    let errors = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "errors",
            "--project",
            "spotter",
            "--session",
            SHORT_SESSION,
            "--since",
            "2026-02-01",
            "--tool",
            "Bash",
            "--top",
            "3",
            "--classify",
            "--format",
            "json",
        ],
    );
    assert!(!errors.as_array().expect("errors").is_empty());

    let health = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "health",
            "--project",
            "spotter",
            "--since",
            "2026-02-01",
            "--limit",
            "3",
            "--format",
            "json",
        ],
    );
    assert!(health["session_count"].as_u64().expect("session count") > 0);

    let sequences = command_json(
        db_path,
        config_path,
        &[
            "transcripts",
            "sequences",
            "--project",
            "spotter",
            "--since",
            "2026-02-01",
            "--min-length",
            "2",
            "--max-length",
            "4",
            "--min-occurrences",
            "1",
            "--recovery",
            "--format",
            "json",
        ],
    );
    assert!(
        sequences["session_count"]
            .as_u64()
            .expect("sequence sessions")
            > 0
    );

    assert_success(
        db_path,
        config_path,
        &["projects", "add", "fixture", FIXTURE_ROOT],
    );
    assert_success(
        db_path,
        config_path,
        &["projects", "alias", "fixture", "renamed"],
    );
    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--db", db_path, "--config", config_path, "projects", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("renamed"));
    assert_success(db_path, config_path, &["projects", "remove", "renamed"]);
}

fn assert_success(db_path: &str, config_path: &str, args: &[&str]) {
    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--db", db_path, "--config", config_path])
        .args(args)
        .assert()
        .success();
}

fn command_json(db_path: &str, config_path: &str, args: &[&str]) -> Value {
    let output = Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--db", db_path, "--config", config_path])
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid json")
}

fn sum_counts(groups: &Value) -> u64 {
    groups
        .as_array()
        .expect("groups")
        .iter()
        .map(|group| group["count"].as_u64().expect("count"))
        .sum()
}
