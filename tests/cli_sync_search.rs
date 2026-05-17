use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::{tempdir, NamedTempFile};

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

#[test]
fn sync_error_reports_invalid_transcript_path() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");
    let transcript = NamedTempFile::new().expect("temp transcript");
    fs::write(transcript.path(), "{not-json\n").expect("write invalid transcript");
    let transcript_path = transcript.path().to_str().expect("utf8 transcript path");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "sync",
            "--file",
            transcript_path,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to scan transcript"))
        .stderr(predicate::str::contains(transcript_path));
}

#[test]
fn sync_root_keeps_valid_transcripts_when_one_file_is_invalid() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");
    let root = tempdir().expect("temp root");
    let valid = root.path().join("tool_heavy.jsonl");
    let invalid = root.path().join("broken.jsonl");
    fs::copy("tests/fixtures/transcripts/tool_heavy.jsonl", valid).expect("copy fixture");
    fs::write(&invalid, "{not-json\n").expect("write invalid transcript");
    let root_path = root.path().to_str().expect("utf8 root path");
    let invalid_path = invalid.to_str().expect("utf8 invalid path");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "sync",
            "--transcript-root",
            root_path,
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Synced 1 session(s)."))
        .stderr(predicate::str::contains("failed 1 transcript(s)"))
        .stderr(predicate::str::contains(invalid_path));

    let aggregate = Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "aggregate",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let aggregate = serde_json::from_slice::<serde_json::Value>(&aggregate).expect("json");
    assert_eq!(aggregate["total_runs"], 6);
}

#[test]
fn sync_root_counts_parent_when_subagent_is_invalid() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");
    let root = tempdir().expect("temp root");
    let session = "258c7280-ae70-4798-800f-63464d01a85d";
    let main = root.path().join(format!("{session}.jsonl"));
    let subagent_dir = root.path().join(session).join("subagents");
    fs::create_dir_all(&subagent_dir).expect("subagent dir");
    let invalid = subagent_dir.join("broken.jsonl");
    fs::copy(
        "tests/fixtures/transcripts/258c7280-ae70-4798-800f-63464d01a85d.jsonl",
        main,
    )
    .expect("copy fixture");
    fs::write(&invalid, "{not-json\n").expect("write invalid subagent");
    let root_path = root.path().to_str().expect("utf8 root path");
    let invalid_path = invalid.to_str().expect("utf8 invalid path");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "sync",
            "--transcript-root",
            root_path,
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("Synced 1 session(s)."))
        .stderr(predicate::str::contains("failed 1 transcript(s)"))
        .stderr(predicate::str::contains(invalid_path));
}
