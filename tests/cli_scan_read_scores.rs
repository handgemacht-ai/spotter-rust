use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

/// Reads are timestamped in the fixture far future (2099) and far past
/// (2001): with a 365-day half-life the future reads decay-clip to full
/// weight while the ancient read rounds to a zero score, so assertions do
/// not depend on the wall clock the test runs at.
const FIXTURE: &str = "tests/fixtures/read-scores/read_scores.jsonl";

fn temp_db_and_config() -> (NamedTempFile, NamedTempFile) {
    (
        NamedTempFile::new().expect("temp db"),
        NamedTempFile::new().expect("temp config"),
    )
}

fn spotter(args: &[&str], db: &str, config: &str) -> assert_cmd::assert::Assert {
    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["--db", db, "--config", config])
        .args(args)
        .assert()
}

fn read_scores_json(db: &str, config: &str, extra_args: &[&str]) -> serde_json::Value {
    let mut args = vec!["scan", "--file", FIXTURE, "read-scores"];
    args.extend_from_slice(extra_args);
    let output = spotter(&args, db, config)
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid json")
}

fn file_paths(result: &serde_json::Value) -> Vec<String> {
    result["files"]
        .as_array()
        .expect("files array")
        .iter()
        .map(|file| file["path"].as_str().expect("path").to_string())
        .collect()
}

/// `scan read-scores` scores only `Read` tool calls, folds the worktree read
/// of `src/main.rs` onto the canonical checkout path, decays the 2001-era
/// read to zero, and orders files highest score first. The command's default
/// output format is JSON.
#[test]
fn scan_read_scores_folds_worktrees_and_decays_old_reads() {
    let (db, config) = temp_db_and_config();
    let result = read_scores_json(
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
        &["--half-life-days", "365"],
    );
    assert_eq!(result["half_life_days"], serde_json::json!(365.0));
    assert_eq!(result["total_reads"].as_i64().expect("total reads"), 5);
    assert_eq!(result["file_count"].as_i64().expect("file count"), 4);

    let files = result["files"].as_array().expect("files array");
    assert_eq!(
        file_paths(&result)[0],
        "/home/USER/projects/spotter/src/main.rs"
    );
    assert_eq!(files[0]["score"], serde_json::json!(2.0));
    assert_eq!(files[0]["reads"].as_i64().expect("reads"), 2);
    assert_eq!(
        files[0]["last_read"].as_str().expect("last read"),
        "2099-01-02T00:00:01+00:00",
        "worktree read is the newer of the two main.rs reads"
    );

    // Equal 1.0 scores order alphabetically by path, ahead of the decayed
    // legacy read.
    assert_eq!(
        file_paths(&result),
        vec![
            "/home/USER/projects/spotter/src/main.rs",
            "/home/USER/projects/other/notes.txt",
            "/home/USER/projects/spotter/docs/guide.md",
            "/home/USER/projects/spotter/src/legacy.rs",
        ]
    );
    let legacy = files.last().expect("legacy entry");
    assert_eq!(legacy["score"], serde_json::json!(0.0));
    assert_eq!(legacy["reads"].as_i64().expect("reads"), 1);
}

/// `--under`, `--ext` and `--limit` combine: the prefix and extension filters
/// keep only the `spotter` Rust reads, and `--limit 1` truncates the list to the
/// highest-scoring file while `file_count` still reports both matches.
#[test]
fn scan_read_scores_filters_under_ext_and_limit() {
    let (db, config) = temp_db_and_config();
    let db_path = db.path().to_str().unwrap();
    let config_path = config.path().to_str().unwrap();

    let result = read_scores_json(
        db_path,
        config_path,
        &[
            "--half-life-days",
            "365",
            "--under",
            "/home/USER/projects/spotter",
            "--ext",
            "rs",
            "--limit",
            "1",
        ],
    );
    assert_eq!(result["total_reads"].as_i64().expect("total reads"), 3);
    assert_eq!(result["file_count"].as_i64().expect("file count"), 2);
    let files = result["files"].as_array().expect("files array");
    assert_eq!(files.len(), 1, "limit truncates to the top file");
    assert_eq!(
        files[0]["path"].as_str().expect("path"),
        "/home/USER/projects/spotter/src/main.rs"
    );
    assert_eq!(files[0]["score"], serde_json::json!(2.0));

    // `--ext md` alone keeps the single markdown read and nothing else.
    let result = read_scores_json(
        db_path,
        config_path,
        &["--half-life-days", "365", "--ext", "md"],
    );
    assert_eq!(result["total_reads"].as_i64().expect("total reads"), 1);
    assert_eq!(
        file_paths(&result),
        vec!["/home/USER/projects/spotter/docs/guide.md"]
    );
}

/// Table output keeps the aggregate header and per-file rows.
#[test]
fn scan_read_scores_table_format_prints_header_and_rows() {
    let (db, config) = temp_db_and_config();
    spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "read-scores",
            "--half-life-days",
            "365",
            "--format",
            "table",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .stdout(predicate::str::contains(
        "Read Scores (4 files, 5 reads, 365-day half-life):",
    ))
    .stdout(predicate::str::contains(
        "/home/USER/projects/spotter/src/main.rs",
    ));
}
