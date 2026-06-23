use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

const FIXTURE: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const LARGE_READS: &str = "tests/fixtures/large-reads/large_reads.jsonl";

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

/// `scan search` reuses the same derivation + filter as `transcripts search`,
/// so a Bash filter from a single fixture returns the same tool_use_id.
#[test]
fn scan_search_returns_expected_tool_use_id() {
    let (db, config) = temp_db_and_config();
    spotter(
        &[
            "scan", "--file", FIXTURE, "search", "--tool", "Bash", "--limit", "5",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("toolu_018GZVh9ymkrdx1TnR8reg5Y"));
}

/// `scan search --file-path` filters runs by file_path the same way the DB
/// path does. JSON output is a plain array of runs.
#[test]
fn scan_search_filters_by_file_path() {
    let (db, config) = temp_db_and_config();
    let output = spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "search",
            "--file-path",
            "phoenix",
            "--format",
            "json",
            "--limit",
            "10",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let runs: serde_json::Value = serde_json::from_slice(&output).expect("valid json");
    assert!(runs.as_array().is_some(), "scan json output is an array");
}

/// `--group-by-session` prints the same summary header as the DB path.
#[test]
fn scan_search_group_by_session_emits_summary_row() {
    let (db, config) = temp_db_and_config();
    spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "search",
            "--tool",
            "Bash",
            "--group-by-session",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("session_id | project"));
}

/// `scan` without a subcommand prints the help/index.
#[test]
fn scan_without_subcommand_prints_index() {
    let (db, config) = temp_db_and_config();
    spotter(
        &["scan"],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("Spotter Scan"));
}

/// Parity: `transcripts search` after `sync` and `scan search` on the same
/// fixture produce the same JSON when filtering by tool name. This pins the
/// derivation contract.
#[test]
fn scan_search_matches_transcripts_search_json() {
    let (db, config) = temp_db_and_config();
    let db_path = db.path().to_str().unwrap();
    let config_path = config.path().to_str().unwrap();

    spotter(
        &["transcripts", "sync", "--file", FIXTURE],
        db_path,
        config_path,
    )
    .success();

    let transcripts = spotter(
        &[
            "transcripts",
            "search",
            "--tool",
            "Bash",
            "--limit",
            "20",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let scan = spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "search",
            "--tool",
            "Bash",
            "--limit",
            "20",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&transcripts).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&scan).unwrap(),
        "transcripts search and scan search disagree on JSON output"
    );
}

/// Parity: aggregate by status across the same fixture yields identical JSON.
#[test]
fn scan_aggregate_matches_transcripts_aggregate_json() {
    let (db, config) = temp_db_and_config();
    let db_path = db.path().to_str().unwrap();
    let config_path = config.path().to_str().unwrap();

    spotter(
        &["transcripts", "sync", "--file", FIXTURE],
        db_path,
        config_path,
    )
    .success();

    let transcripts = spotter(
        &[
            "transcripts",
            "aggregate",
            "--group-by",
            "tool_name,status",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let scan = spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "aggregate",
            "--group-by",
            "tool_name,status",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&transcripts).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&scan).unwrap(),
        "transcripts aggregate and scan aggregate disagree on JSON output"
    );
}

/// Parity: error analysis with classify=true yields identical JSON.
#[test]
fn scan_errors_matches_transcripts_errors_json() {
    let (db, config) = temp_db_and_config();
    let db_path = db.path().to_str().unwrap();
    let config_path = config.path().to_str().unwrap();

    spotter(
        &["transcripts", "sync", "--file", FIXTURE],
        db_path,
        config_path,
    )
    .success();

    let transcripts = spotter(
        &[
            "transcripts",
            "errors",
            "--classify",
            "--top",
            "10",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let scan = spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "errors",
            "--classify",
            "--top",
            "10",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&transcripts).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&scan).unwrap(),
        "transcripts errors and scan errors disagree on JSON output"
    );
}

/// Parity: sequence analysis with recovery yields identical JSON.
#[test]
fn scan_sequences_matches_transcripts_sequences_json() {
    let (db, config) = temp_db_and_config();
    let db_path = db.path().to_str().unwrap();
    let config_path = config.path().to_str().unwrap();

    spotter(
        &["transcripts", "sync", "--file", FIXTURE],
        db_path,
        config_path,
    )
    .success();

    let transcripts = spotter(
        &[
            "transcripts",
            "sequences",
            "--min-occurrences",
            "1",
            "--recovery",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let scan = spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "sequences",
            "--min-occurrences",
            "1",
            "--recovery",
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&transcripts).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&scan).unwrap(),
        "transcripts sequences and scan sequences disagree on JSON output"
    );
}

/// Parity: per-session health agrees on the same fixture.
#[test]
fn scan_health_session_matches_transcripts_health_json() {
    let (db, config) = temp_db_and_config();
    let db_path = db.path().to_str().unwrap();
    let config_path = config.path().to_str().unwrap();

    spotter(
        &["transcripts", "sync", "--file", FIXTURE],
        db_path,
        config_path,
    )
    .success();

    let session_id = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";
    let transcripts = spotter(
        &[
            "transcripts",
            "health",
            "--session",
            session_id,
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let scan = spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "health",
            "--session",
            session_id,
            "--format",
            "json",
        ],
        db_path,
        config_path,
    )
    .success()
    .get_output()
    .stdout
    .clone();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&transcripts).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&scan).unwrap(),
        "transcripts health and scan health disagree on JSON output"
    );
}

/// `scan inspect` resolves a session and prints runs sorted by ordinal.
#[test]
fn scan_inspect_session_returns_runs() {
    let (db, config) = temp_db_and_config();
    spotter(
        &[
            "scan",
            "--file",
            FIXTURE,
            "inspect",
            "--session",
            "d6e0bada-1959-4eec-a9d2-0bfade768d8f",
            "--format",
            "json",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("toolu_018GZVh9ymkrdx1TnR8reg5Y"));
}

/// `scan audit --file` reports JSONL line count and parsed message count.
#[test]
fn scan_audit_reports_line_counts() {
    let (db, config) = temp_db_and_config();
    spotter(
        &[
            "scan", "--file", FIXTURE, "audit", "--format", "json",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("jsonl_lines"))
    .stdout(predicate::str::contains("parsed_messages"));
}

/// `scan search` exposes the file line counts a `Read` recorded in the
/// transcript, taken from `toolUseResult.file` rather than the visible body.
#[test]
fn scan_search_reports_read_line_counts() {
    let (db, config) = temp_db_and_config();
    let output = spotter(
        &[
            "scan",
            "--file",
            LARGE_READS,
            "search",
            "--tool",
            "Read",
            "--format",
            "json",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let runs: Vec<serde_json::Value> = serde_json::from_slice(&output).expect("valid json");
    let trunc = runs
        .iter()
        .find(|run| run["tool_use_id"] == "toolu_read_trunc")
        .expect("truncated read present");
    // The file is 3000 lines even though only 200 were returned into context.
    assert_eq!(trunc["read_total_lines"], 3000);
    assert_eq!(trunc["read_lines"], 200);
    assert_eq!(trunc["read_truncated"], true);
}

/// `--min-read-lines` keeps reads of large files and drops small ones, judging
/// by the file's true size so a token-cap-truncated read still qualifies.
#[test]
fn scan_search_filters_by_min_read_lines() {
    let (db, config) = temp_db_and_config();
    let output = spotter(
        &[
            "scan",
            "--file",
            LARGE_READS,
            "search",
            "--min-read-lines",
            "1000",
            "--format",
            "json",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let runs: Vec<serde_json::Value> = serde_json::from_slice(&output).expect("valid json");
    let mut ids: Vec<&str> = runs
        .iter()
        .map(|run| run["tool_use_id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["toolu_read_big", "toolu_read_trunc"]);
}
