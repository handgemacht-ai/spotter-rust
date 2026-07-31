//! The `sample` verb: stratified, seeded sampling of tool-call runs.
//! Covers determinism, rare-stratum coverage, scoping, drill-down IDs,
//! parity, and `--fields` projection.

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
    fn seeded() -> Self {
        let harness = Self {
            db: NamedTempFile::new().expect("temp db"),
            config: NamedTempFile::new().expect("temp config"),
        };
        for fixture in [TOOL_HEAVY, SHORT] {
            harness
                .spotter(&["transcripts", "sync", "--file", fixture])
                .success();
        }
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

    fn stdout(&self, args: &[&str]) -> String {
        String::from_utf8(self.spotter(args).success().get_output().stdout.clone())
            .expect("utf8 stdout")
    }

    fn json(&self, args: &[&str]) -> Value {
        serde_json::from_str(&self.stdout(args)).expect("valid json")
    }
}

#[test]
fn envelope_shape_and_seed_echo() {
    let harness = Harness::seeded();
    let sample = harness.json(&["transcripts", "sample", "-n", "5", "--seed", "7"]);
    let keys: Vec<&str> = sample
        .as_object()
        .expect("envelope object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["population", "runs", "seed", "strata", "stratify_by"]
    );
    assert_eq!(sample["seed"], 7);
    assert_eq!(sample["stratify_by"], "tool_name");
    assert_eq!(sample["population"], 8);
    assert_eq!(sample["runs"].as_array().expect("runs").len(), 5);
    // Runs carry the decorated run_id like search output.
    let run = &sample["runs"][0];
    assert_eq!(
        run["run_id"].as_str().expect("run_id"),
        format!(
            "{}:{}",
            run["session_id"].as_str().unwrap(),
            run["tool_use_id"].as_str().unwrap()
        )
    );
}

#[test]
fn same_seed_is_byte_identical_across_invocations() {
    let harness = Harness::seeded();
    let args = ["transcripts", "sample", "-n", "6", "--seed", "123"];
    assert_eq!(harness.stdout(&args), harness.stdout(&args));
}

#[test]
fn rare_stratum_is_always_represented() {
    let harness = Harness::seeded();
    // 7 completed vs 1 error: the error run must survive every seed.
    for seed in ["1", "2", "3", "42"] {
        let sample = harness.json(&[
            "transcripts",
            "sample",
            "-n",
            "4",
            "--stratify-by",
            "status",
            "--seed",
            seed,
        ]);
        let statuses: Vec<&str> = sample["runs"]
            .as_array()
            .expect("runs")
            .iter()
            .map(|run| run["status"].as_str().unwrap())
            .collect();
        assert!(
            statuses.contains(&"error"),
            "seed {seed} dropped the error stratum: {statuses:?}"
        );
        let error_stratum = sample["strata"]
            .as_array()
            .expect("strata")
            .iter()
            .find(|stratum| stratum["key"] == "error")
            .expect("error stratum summary");
        assert_eq!(error_stratum["population"], 1);
        assert_eq!(error_stratum["sampled"], 1);
    }
}

#[test]
fn scoping_and_filters_narrow_the_population() {
    let harness = Harness::seeded();
    let session = harness.json(&["transcripts", "sample", "--session", TOOL_SESSION]);
    assert_eq!(session["population"], 6);

    let tool = harness.json(&["transcripts", "sample", "--tool", "Read"]);
    assert_eq!(tool["population"], 1);

    let status = harness.json(&["transcripts", "sample", "--status", "error"]);
    assert_eq!(status["population"], 1);

    let since = harness.json(&["transcripts", "sample", "--since", "2999-01-01"]);
    assert_eq!(since["population"], 0);
    assert!(since["runs"].as_array().expect("runs").is_empty());
}

#[test]
fn sampled_run_ids_resolve_via_inspect_run() {
    let harness = Harness::seeded();
    let sample = harness.json(&["transcripts", "sample", "-n", "3"]);
    for run in sample["runs"].as_array().expect("runs") {
        let run_id = run["run_id"].as_str().expect("run_id");
        let inspect = harness.json(&[
            "transcripts",
            "inspect",
            "--run",
            run_id,
            "--context",
            "0",
            "--format",
            "json",
        ]);
        let runs = inspect.as_array().expect("run array");
        assert_eq!(runs.len(), 1, "inspect --run {run_id}");
        assert_eq!(
            runs[0]["tool_use_id"].as_str().unwrap(),
            run["tool_use_id"].as_str().unwrap()
        );
    }
}

#[test]
fn transcripts_and_scan_emit_byte_identical_samples() {
    let harness = Harness::seeded();
    for extra in [
        vec!["-n", "6"],
        vec!["-n", "4", "--stratify-by", "status", "--seed", "9"],
        vec!["--stratify-by", "session"],
    ] {
        let mut transcripts_args = vec!["transcripts", "sample"];
        transcripts_args.extend(extra.iter().copied());
        let mut scan_args = vec!["scan", "--file", TOOL_HEAVY, "--file", SHORT, "sample"];
        scan_args.extend(extra.iter().copied());
        assert_eq!(
            harness.stdout(&transcripts_args),
            harness.stdout(&scan_args),
            "sample parity diverged for {extra:?}"
        );
    }
}

#[test]
fn fields_projection_selects_envelope_keys() {
    let harness = Harness::seeded();
    let sample = harness.json(&[
        "transcripts",
        "sample",
        "-n",
        "2",
        "--fields",
        "runs,strata",
    ]);
    let keys: Vec<&str> = sample
        .as_object()
        .expect("envelope object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["runs", "strata"]);

    harness
        .spotter(&["transcripts", "sample", "--fields", "no_such_key"])
        .failure()
        .stderr(predicate::str::contains("unknown field 'no_such_key'"));
}

#[test]
fn sample_table_output_renders_strata() {
    let harness = Harness::seeded();
    harness
        .spotter(&["transcripts", "sample", "-n", "4", "--format", "table"])
        .success()
        .stdout(predicate::str::contains("Sample (4 of 8 runs"))
        .stdout(predicate::str::contains("tool_use_id | tool_name"));
}
