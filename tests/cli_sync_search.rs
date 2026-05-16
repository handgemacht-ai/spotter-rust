use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

#[test]
fn sync_file_then_search_tool_calls_and_content() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "sync",
            "--file",
            "tests/fixtures/transcripts/tool_heavy.jsonl",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Synced 1 session(s)."));

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "search",
            "--tool",
            "Bash",
            "--limit",
            "5",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("toolu_018GZVh9ymkrdx1TnR8reg5Y"));

    let json = Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "search",
            "--tool",
            "Bash",
            "--limit",
            "5",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice::<serde_json::Value>(&json).expect("valid json search output");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "search",
            "--content-contains",
            "phoenix",
            "--limit",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("start phoenix in background"));
}
