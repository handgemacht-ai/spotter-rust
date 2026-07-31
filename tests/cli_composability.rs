//! Composability contract: stable run IDs in output, ID-based drill-down,
//! and `--fields` projection, exercised end to end on both backends.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;

const TOOL_HEAVY: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const SHORT: &str = "tests/fixtures/transcripts/short.jsonl";
const TOOL_SESSION: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";

struct Harness {
    db: NamedTempFile,
    config: NamedTempFile,
}

impl Harness {
    fn new() -> Self {
        Self {
            db: NamedTempFile::new().expect("temp db"),
            config: NamedTempFile::new().expect("temp config"),
        }
    }

    fn seeded() -> Self {
        let harness = Self::new();
        for fixture in [TOOL_HEAVY, SHORT] {
            harness
                .spotter(&["transcripts", "sync", "--file", fixture])
                .success();
        }
        harness
    }

    /// Seed only the `tool_heavy` fixture so `transcripts` (DB) and
    /// `scan --file TOOL_HEAVY` (single file) cover the same sessions.
    fn seeded_tool_heavy() -> Self {
        let harness = Self::new();
        harness
            .spotter(&["transcripts", "sync", "--file", TOOL_HEAVY])
            .success();
        harness
    }

    fn spotter(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("spotter")
            .expect("binary")
            .args([
                "--db",
                self.db.path().to_str().unwrap(),
                "--config",
                self.config.path().to_str().unwrap(),
            ])
            .args(args)
            .assert()
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self.spotter(args).success().get_output().stdout.clone();
        serde_json::from_slice(&output).expect("valid json")
    }
}

#[test]
fn search_runs_carry_stable_run_ids_on_both_backends() {
    let harness = Harness::seeded_tool_heavy();
    let args = ["search", "--limit", "20", "--format", "json"];
    let transcripts = harness.json(&[&["transcripts"], &args[..]].concat());
    let scan = harness.json(&[&["scan", "--file", TOOL_HEAVY], &args[..]].concat());

    for (backend, runs) in [("transcripts", &transcripts), ("scan", &scan)] {
        let runs = runs.as_array().expect("run array");
        assert!(!runs.is_empty(), "{backend} returned no runs");
        for run in runs {
            let expected = format!(
                "{}:{}",
                run["session_id"].as_str().expect("session_id"),
                run["tool_use_id"].as_str().expect("tool_use_id")
            );
            assert_eq!(
                run["run_id"].as_str().expect("run_id present"),
                expected,
                "{backend} run id must compose session_id and tool_use_id"
            );
        }
    }
    assert_eq!(
        transcripts, scan,
        "transcripts and scan search must emit identical run JSON"
    );
}

#[test]
fn run_id_round_trips_into_inspect_on_both_backends() {
    let harness = Harness::seeded();
    let search = harness.json(&["transcripts", "search", "--limit", "1", "--format", "json"]);
    let run_id = search[0]["run_id"].as_str().expect("run_id").to_string();
    let tool_use_id = search[0]["tool_use_id"].as_str().expect("tool_use_id");

    let transcripts = harness.json(&[
        "transcripts",
        "inspect",
        "--run",
        &run_id,
        "--context",
        "0",
        "--format",
        "json",
    ]);
    let scan = harness.json(&[
        "scan",
        "--file",
        TOOL_HEAVY,
        "inspect",
        "--run",
        &run_id,
        "--context",
        "0",
        "--format",
        "json",
    ]);

    for (backend, runs) in [("transcripts", &transcripts), ("scan", &scan)] {
        let runs = runs.as_array().expect("run array");
        assert_eq!(runs.len(), 1, "{backend} --run must select exactly one run");
        assert_eq!(runs[0]["tool_use_id"].as_str().unwrap(), tool_use_id);
        assert_eq!(runs[0]["run_id"].as_str().unwrap(), run_id);
    }
}

#[test]
fn inspect_ordinals_window_selects_overlapping_runs() {
    let harness = Harness::seeded();
    let all = harness.json(&[
        "transcripts",
        "inspect",
        "--session",
        TOOL_SESSION,
        "--format",
        "json",
    ]);
    let run = &all.as_array().expect("run array")[1];
    let start = run["start_ordinal"].as_i64().expect("start_ordinal");
    let tool_use_id = run["tool_use_id"].as_str().expect("tool_use_id");

    let window = harness.json(&[
        "transcripts",
        "inspect",
        "--session",
        TOOL_SESSION,
        "--ordinals",
        &format!("{start}:{start}"),
        "--format",
        "json",
    ]);
    let window = window.as_array().expect("run array");
    assert!(
        window
            .iter()
            .any(|hit| hit["tool_use_id"].as_str().unwrap() == tool_use_id),
        "ordinal window must include the run starting at {start}"
    );

    let outside = harness.json(&[
        "transcripts",
        "inspect",
        "--session",
        TOOL_SESSION,
        "--ordinals",
        "1000000:1000001",
        "--format",
        "json",
    ]);
    assert_eq!(
        outside.as_array().expect("run array").len(),
        0,
        "window outside the transcript must select nothing"
    );

    harness
        .spotter(&[
            "transcripts",
            "inspect",
            "--session",
            TOOL_SESSION,
            "--ordinals",
            "9:3",
        ])
        .failure()
        .stderr(predicate::str::contains("--ordinals"));
}

#[test]
fn fields_projection_filters_top_level_keys() {
    let harness = Harness::seeded_tool_heavy();
    let args = [
        "search",
        "--limit",
        "5",
        "--format",
        "json",
        "--fields",
        "run_id,tool_name",
    ];
    let transcripts = harness.json(&[&["transcripts"], &args[..]].concat());
    let scan = harness.json(&[&["scan", "--file", TOOL_HEAVY], &args[..]].concat());

    for (backend, runs) in [("transcripts", &transcripts), ("scan", &scan)] {
        for run in runs.as_array().expect("run array") {
            let keys: Vec<&str> = run
                .as_object()
                .expect("run object")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, ["run_id", "tool_name"], "{backend} projection");
        }
    }
    assert_eq!(
        transcripts, scan,
        "projection must be identical on both paths"
    );

    // Object output: only the named top-level keys survive.
    let inspect = harness.json(&[
        "transcripts",
        "inspect",
        "--session",
        TOOL_SESSION,
        "--with-messages",
        "--format",
        "json",
        "--fields",
        "runs",
    ]);
    let keys: Vec<&str> = inspect
        .as_object()
        .expect("inspect object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["runs"]);

    // Repeatable form and unknown fields.
    let repeated = harness.json(&[
        "transcripts",
        "search",
        "--limit",
        "1",
        "--format",
        "json",
        "--fields",
        "run_id",
        "--fields",
        "status",
    ]);
    let keys: Vec<&str> = repeated[0]
        .as_object()
        .expect("run object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["run_id", "status"]);

    harness
        .spotter(&[
            "transcripts",
            "search",
            "--format",
            "json",
            "--fields",
            "no_such_key",
        ])
        .failure()
        .stderr(predicate::str::contains(
            "unknown field 'no_such_key'; available top-level fields:",
        ));
}

#[test]
fn error_patterns_expose_sample_run_ids_for_drill_down() {
    let harness = Harness::seeded();
    let errors = harness.json(&["transcripts", "errors", "--format", "json"]);
    let patterns = errors["patterns"].as_array().expect("patterns");
    assert!(!patterns.is_empty());

    let sample = patterns
        .iter()
        .flat_map(|pattern| pattern["sample_runs"].as_array().expect("sample_runs"))
        .next()
        .expect("at least one sample run id")
        .as_str()
        .expect("run id string")
        .to_string();
    let tool_use_id = sample
        .rsplit_once(':')
        .expect("run id has session:tool_use_id shape")
        .1;

    let inspect = harness.json(&[
        "transcripts",
        "inspect",
        "--run",
        &sample,
        "--context",
        "0",
        "--format",
        "json",
    ]);
    let runs = inspect.as_array().expect("run array");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["tool_use_id"].as_str().unwrap(), tool_use_id);
    assert_eq!(runs[0]["status"].as_str().unwrap(), "error");
}

#[test]
fn inspect_rejects_conflicting_or_missing_targets() {
    let harness = Harness::seeded();
    harness
        .spotter(&[
            "transcripts",
            "inspect",
            "--session",
            TOOL_SESSION,
            "--run",
            &format!("{TOOL_SESSION}:toolu_018GZVh9ymkrdx1TnR8reg5Y"),
        ])
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    harness
        .spotter(&["transcripts", "inspect"])
        .failure()
        .stderr(predicate::str::contains("--session"));

    harness
        .spotter(&["transcripts", "inspect", "--run", "not-a-run-id"])
        .failure()
        .stderr(predicate::str::contains("invalid --run id"));
}
