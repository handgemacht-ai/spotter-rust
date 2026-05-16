use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

#[test]
fn inspect_compare_and_aggregate_after_sync() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");
    let session = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";

    sync_fixture(db_path);

    let inspect_json = Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "inspect",
            "--session",
            session,
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspect = serde_json::from_slice::<serde_json::Value>(&inspect_json).expect("json");
    assert_eq!(inspect.as_array().expect("array").len(), 6);

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "compare",
            "--left-session",
            session,
            "--right-session",
            session,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Left cohort:"))
        .stdout(predicate::str::contains("Right cohort:"));

    let compare_json = Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "transcripts",
            "compare",
            "--left-session",
            session,
            "--right-session",
            session,
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let compare = serde_json::from_slice::<serde_json::Value>(&compare_json).expect("json");
    assert!(!compare["left"].as_array().expect("left array").is_empty());
    assert!(!compare["right"].as_array().expect("right array").is_empty());

    let aggregate_json = Command::cargo_bin("spotter")
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
    let aggregate = serde_json::from_slice::<serde_json::Value>(&aggregate_json).expect("json");
    assert_eq!(aggregate["total_runs"], 6);
}

fn sync_fixture(db_path: &str) {
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
        .success();
}
