use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

const TOOL_HEAVY: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const SHORT: &str = "tests/fixtures/transcripts/short.jsonl";
const SUBAGENT: &str = "tests/fixtures/transcripts/subagent.jsonl";

const TOOL_HEAVY_SESSION: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";
const SHORT_SESSION: &str = "55604662-cf2a-4331-851a-ec234028f8ca";
const SUBAGENT_SESSION: &str = "491e126e-1e71-469c-ade2-fcc8af567c74";

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

fn compare_json(db: &str, config: &str, extra_args: &[&str]) -> serde_json::Value {
    let mut args = vec![
        "scan", "--file", TOOL_HEAVY, "--file", SHORT, "--file", SUBAGENT, "compare",
    ];
    args.extend_from_slice(extra_args);
    let output = spotter(&args, db, config)
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid json")
}

/// Collapse compare groups to (key, count) pairs for exact-order assertions.
fn group_counts(groups: &serde_json::Value) -> Vec<(String, i64)> {
    groups
        .as_array()
        .expect("groups array")
        .iter()
        .map(|group| {
            (
                group["key"].as_str().expect("key").to_string(),
                group["count"].as_i64().expect("count"),
            )
        })
        .collect()
}

/// `scan compare` groups tool usage per session cohort. The left cohort is a
/// repeatable multi-session selection (`tool_heavy` plus the standalone
/// subagent transcript), so its Read count is the sum of both sessions'
/// reads while the right cohort only sees its own Bash calls.
#[test]
fn scan_compare_groups_tool_usage_between_session_cohorts() {
    let (db, config) = temp_db_and_config();
    let result = compare_json(
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
        &[
            "--left-session",
            TOOL_HEAVY_SESSION,
            "--left-session",
            SUBAGENT_SESSION,
            "--right-session",
            SHORT_SESSION,
            "--group-by",
            "tool_name",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        group_counts(&result["left"]),
        vec![
            ("Bash".to_string(), 3),
            ("Edit".to_string(), 1),
            ("Grep".to_string(), 1),
            ("Read".to_string(), 2),
        ],
        "left cohort groups tool_heavy plus the subagent read"
    );
    assert_eq!(
        group_counts(&result["right"]),
        vec![("Bash".to_string(), 2)],
        "right cohort only sees the short session"
    );
    // Fixture tool calls run one second assistant-to-result, so every group
    // averages 1000ms.
    for group in result["left"]
        .as_array()
        .unwrap()
        .iter()
        .chain(result["right"].as_array().unwrap().iter())
    {
        assert_eq!(
            group["avg_duration_ms"].as_i64().expect("avg duration"),
            1000
        );
    }
}

/// `--tool` plus `--command-contains` narrow both cohorts before grouping:
/// only the Bash calls mentioning `mix` survive, and `--group-by status`
/// splits the right cohort into its completed and error runs.
#[test]
fn scan_compare_filters_commands_and_groups_by_status() {
    let (db, config) = temp_db_and_config();
    let result = compare_json(
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
        &[
            "--left-session",
            TOOL_HEAVY_SESSION,
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
    assert_eq!(
        group_counts(&result["left"]),
        vec![("completed".to_string(), 1)],
        "left cohort keeps only `mix phx.server`"
    );
    assert_eq!(
        group_counts(&result["right"]),
        vec![("completed".to_string(), 1), ("error".to_string(), 1),],
        "right cohort splits `mix compile` and the failed `mix run`"
    );
}

/// Table output names both cohorts and inlines per-group counts.
#[test]
fn scan_compare_table_format_prints_cohorts() {
    let (db, config) = temp_db_and_config();
    spotter(
        &[
            "scan",
            "--file",
            TOOL_HEAVY,
            "--file",
            SHORT,
            "compare",
            "--left-session",
            TOOL_HEAVY_SESSION,
            "--right-session",
            SHORT_SESSION,
            "--format",
            "table",
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .success()
    .stdout(predicate::str::contains("Left cohort:"))
    .stdout(predicate::str::contains("Right cohort:"))
    .stdout(predicate::str::contains(
        "Bash: count=3, avg_duration=1000ms",
    ));
}

/// `--left-session` is required: omitting it fails with a usage error.
#[test]
fn scan_compare_requires_left_session() {
    let (db, config) = temp_db_and_config();
    spotter(
        &[
            "scan",
            "--file",
            TOOL_HEAVY,
            "compare",
            "--right-session",
            SHORT_SESSION,
        ],
        db.path().to_str().unwrap(),
        config.path().to_str().unwrap(),
    )
    .failure()
    .code(2)
    .stderr(predicate::str::contains(
        "the following required arguments were not provided",
    ))
    .stderr(predicate::str::contains("--left-session <LEFT_SESSION>"));
}
