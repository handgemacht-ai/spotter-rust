#![allow(clippy::too_many_lines)]

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

#[test]
fn audit_errors_health_sequences_projects_and_init_work() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");
    let session = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "sync",
            "--transcript-root",
            "tests/fixtures/transcripts",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync for subagent a881341"));

    let parent_inspect = command_json(&[
        "--db",
        db_path,
        "transcripts",
        "inspect",
        "--session",
        "258c7280-ae70-4798-800f-63464d01a85d",
        "--format",
        "json",
    ]);
    assert!(parent_inspect.as_array().expect("array").iter().any(|run| {
        run["is_subagent"].as_bool() == Some(true) && run["tool_name"].as_str() == Some("WebFetch")
    }));

    let audit = command_json(&[
        "--db",
        db_path,
        "transcripts",
        "audit",
        "--session",
        session,
        "--format",
        "json",
    ]);
    assert_eq!(audit[0]["missing"], 0);

    let errors = command_json(&[
        "--db",
        db_path,
        "transcripts",
        "errors",
        "--top",
        "5",
        "--classify",
        "--format",
        "json",
    ]);
    assert!(errors["patterns"].as_array().expect("patterns").len() <= 5);
    assert!(errors["total_errors"].as_u64().expect("total errors") > 0);

    let health = command_json(&[
        "--db",
        db_path,
        "transcripts",
        "health",
        "--session",
        session,
        "--format",
        "json",
    ]);
    assert!(health["message_count"].as_u64().expect("count") > 0);

    let global_health =
        command_json(&["--db", db_path, "transcripts", "health", "--format", "json"]);
    assert!(global_health["session_count"].as_u64().expect("sessions") > 0);
    assert!(global_health["total_cache_read_tokens"].is_number());
    assert!(global_health["peak_cache_creation_tokens"].is_number());

    let sequences = command_json(&[
        "--db",
        db_path,
        "transcripts",
        "sequences",
        "--min-occurrences",
        "1",
        "--format",
        "json",
    ]);
    assert!(sequences["session_count"].as_u64().expect("sessions") > 0);

    let config = NamedTempFile::new().expect("temp config");
    let config_path = config.path().to_str().expect("utf8 temp path");
    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--config",
            config_path,
            "projects",
            "add",
            "fixture",
            "tests/fixtures/transcripts",
        ])
        .assert()
        .success();
    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--config", config_path, "projects", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "fixture | tests/fixtures/transcripts",
        ));

    let init_config = NamedTempFile::new().expect("temp init config");
    let init_path = init_config.path().to_str().expect("utf8 temp path");
    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--config",
            init_path,
            "init",
            "--claude-projects",
            "tests/fixtures/transcripts",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote"));

    let config_db = NamedTempFile::new().expect("temp config db");
    let config_db_path = config_db.path().to_str().expect("utf8 temp path");
    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            config_db_path,
            "--config",
            init_path,
            "transcripts",
            "sync",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Synced"));

    let interactive_config = NamedTempFile::new().expect("temp interactive config");
    let interactive_path = interactive_config.path().to_str().expect("utf8 temp path");
    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--config",
            interactive_path,
            "init",
            "--claude-projects",
            "tests/fixtures/transcripts",
        ])
        .write_stdin("1\nchosen-alias\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote 1 project(s)"));
    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--config", interactive_path, "projects", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("chosen-alias"));
}

#[test]
fn init_interactive_all_selection_writes_multiple_aliases() {
    let config = NamedTempFile::new().expect("temp config");
    let config_path = config.path().to_str().expect("utf8 temp path");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--config",
            config_path,
            "init",
            "--claude-projects",
            "tests/fixtures/transcripts",
        ])
        .write_stdin("all\nfirst-alias\nsecond-alias\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote 2 project(s)"));

    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--config", config_path, "projects", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("first-alias"))
        .stdout(predicate::str::contains("second-alias"));
}

fn command_json(args: &[&str]) -> serde_json::Value {
    let output = Command::cargo_bin("spotter")
        .expect("binary")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid json")
}
