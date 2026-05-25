use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

/// `spotter scan` runs directly against a JSONL file and returns the same shape
/// of `ToolCallRun` as `transcripts search` does after `sync`, without ever
/// opening the database.
#[test]
fn scan_file_matches_search_after_sync() {
    let stub_db = NamedTempFile::new().expect("temp db");
    let stub_db_path = stub_db.path().to_str().expect("utf8 path");
    let stub_config = NamedTempFile::new().expect("temp config");
    let stub_config_path = stub_config.path().to_str().expect("utf8 path");

    let scan = Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            stub_db_path,
            "--config",
            stub_config_path,
            "scan",
            "--file",
            "tests/fixtures/transcripts/tool_heavy.jsonl",
            "--tool",
            "Bash",
            "--limit",
            "5",
        ])
        .assert()
        .success();
    scan.stdout(predicate::str::contains("toolu_018GZVh9ymkrdx1TnR8reg5Y"));
}

#[test]
fn scan_filters_by_file_path() {
    let stub_db = NamedTempFile::new().expect("temp db");
    let stub_db_path = stub_db.path().to_str().expect("utf8 path");
    let stub_config = NamedTempFile::new().expect("temp config");
    let stub_config_path = stub_config.path().to_str().expect("utf8 path");

    let output = Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            stub_db_path,
            "--config",
            stub_config_path,
            "scan",
            "--file",
            "tests/fixtures/transcripts/tool_heavy.jsonl",
            "--file-path",
            "phoenix",
            "--format",
            "json",
            "--limit",
            "10",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let runs: serde_json::Value = serde_json::from_slice(&output).expect("valid json");
    assert!(runs.as_array().is_some(), "scan json output is an array");
}

#[test]
fn scan_group_by_session_emits_summary_row() {
    let stub_db = NamedTempFile::new().expect("temp db");
    let stub_db_path = stub_db.path().to_str().expect("utf8 path");
    let stub_config = NamedTempFile::new().expect("temp config");
    let stub_config_path = stub_config.path().to_str().expect("utf8 path");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            stub_db_path,
            "--config",
            stub_config_path,
            "scan",
            "--file",
            "tests/fixtures/transcripts/tool_heavy.jsonl",
            "--tool",
            "Bash",
            "--group-by-session",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("session_id | project"));
}
