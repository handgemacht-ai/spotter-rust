//! The `profile` verb: one-call dimensional overview for anomaly discovery.
//! Covers envelope sections, scoping, drill-down IDs, `--top` capping,
//! `--fields` projection, and transcripts↔scan parity.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;

const TOOL_HEAVY: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const SHORT: &str = "tests/fixtures/transcripts/short.jsonl";
const TOOL_SESSION: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";
const SHORT_SESSION: &str = "55604662-cf2a-4331-851a-ec234028f8ca";

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

    fn json(&self, args: &[&str]) -> Value {
        let output = self.spotter(args).success().get_output().stdout.clone();
        serde_json::from_slice(&output).expect("valid json")
    }
}

const SCAN_FILES: &[&str] = &["scan", "--file", TOOL_HEAVY, "--file", SHORT];

#[test]
fn envelope_has_all_sections_with_content() {
    let harness = Harness::seeded();
    let profile = harness.json(&["transcripts", "profile"]);
    let keys: Vec<&str> = profile
        .as_object()
        .expect("envelope object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "errors",
            "outliers",
            "overview",
            "recovery",
            "token_health",
            "tools"
        ]
    );

    assert_eq!(profile["overview"]["session_count"], 2);
    assert_eq!(profile["overview"]["run_count"], 8);
    assert_eq!(
        profile["overview"]["projects"],
        serde_json::json!(["spotter"])
    );
    assert!(!profile["tools"].as_array().expect("tools").is_empty());
    assert_eq!(profile["errors"]["total_errors"], 1);
    assert_eq!(
        profile["token_health"]["total_jumps"].as_u64().unwrap(),
        2,
        "short fixture contributes token jumps"
    );
    assert!(!profile["outliers"]["slowest_runs"]
        .as_array()
        .expect("slowest")
        .is_empty());
}

#[test]
fn scoping_flags_narrow_the_envelope() {
    let harness = Harness::seeded();
    let profile = harness.json(&["transcripts", "profile", "--session", TOOL_SESSION]);
    assert_eq!(profile["overview"]["session_count"], 1);
    assert_eq!(profile["overview"]["run_count"], 6);
    assert_eq!(profile["errors"]["total_errors"], 0);

    let project = harness.json(&["transcripts", "profile", "--project", "spotter"]);
    assert_eq!(project["overview"]["session_count"], 2);
    let missing = harness.json(&["transcripts", "profile", "--project", "missing"]);
    assert_eq!(missing["overview"]["session_count"], 0);

    let empty = harness.json(&["transcripts", "profile", "--since", "2999-01-01"]);
    assert_eq!(empty["overview"]["session_count"], 0);
    assert_eq!(empty["overview"]["run_count"], 0);
    // Sections still render (empty, not missing) on an empty scope.
    assert!(empty["tools"].is_array());
    assert!(empty["errors"]["top_patterns"].is_array());
}

#[test]
fn outlier_and_error_ids_resolve_via_inspect_run() {
    let harness = Harness::seeded();
    let profile = harness.json(&["transcripts", "profile"]);

    let slowest = profile["outliers"]["slowest_runs"][0]["run_id"]
        .as_str()
        .expect("run_id")
        .to_string();
    let sample = profile["errors"]["top_patterns"][0]["sample_runs"][0]
        .as_str()
        .expect("sample run")
        .to_string();

    for run_id in [&slowest, &sample] {
        let expected_tool = run_id.rsplit_once(':').expect("run id shape").1;
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
        assert_eq!(runs[0]["tool_use_id"].as_str().unwrap(), expected_tool);
    }

    // The error hotspot session id scopes a follow-up profile.
    let hotspot = profile["outliers"]["error_hotspots"][0]["session_id"]
        .as_str()
        .expect("hotspot session")
        .to_string();
    assert_eq!(hotspot, SHORT_SESSION);
    let scoped = harness.json(&["transcripts", "profile", "--session", &hotspot]);
    assert_eq!(scoped["errors"]["total_errors"], 1);
}

#[test]
fn transcripts_and_scan_emit_byte_identical_profile() {
    let harness = Harness::seeded();
    for extra in [vec![], vec!["--session", TOOL_SESSION], vec!["--top", "1"]] {
        let mut transcripts_args = vec!["transcripts", "profile"];
        transcripts_args.extend(extra.iter().copied());
        let mut scan_args = SCAN_FILES.to_vec();
        scan_args.push("profile");
        scan_args.extend(extra.iter().copied());

        let transcripts = harness
            .spotter(&transcripts_args)
            .success()
            .get_output()
            .stdout
            .clone();
        let scan = harness
            .spotter(&scan_args)
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            String::from_utf8_lossy(&transcripts),
            String::from_utf8_lossy(&scan),
            "profile parity diverged for {extra:?}"
        );
    }
}

#[test]
fn top_caps_error_patterns_and_outlier_lists() {
    let harness = Harness::seeded();
    let profile = harness.json(&["transcripts", "profile", "--top", "1"]);
    assert!(
        profile["errors"]["top_patterns"]
            .as_array()
            .expect("patterns")
            .len()
            <= 1
    );
    assert_eq!(
        profile["outliers"]["slowest_runs"]
            .as_array()
            .expect("slowest")
            .len(),
        1
    );
    assert!(
        profile["outliers"]["error_hotspots"]
            .as_array()
            .expect("hotspots")
            .len()
            <= 1
    );
    assert!(
        profile["token_health"]["flagged_sessions"]
            .as_array()
            .expect("flagged")
            .len()
            <= 1
    );
}

#[test]
fn fields_projection_selects_sections() {
    let harness = Harness::seeded();
    let profile = harness.json(&["transcripts", "profile", "--fields", "overview,tools"]);
    let keys: Vec<&str> = profile
        .as_object()
        .expect("envelope object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["overview", "tools"]);

    harness
        .spotter(&["transcripts", "profile", "--fields", "no_such_section"])
        .failure()
        .stderr(predicate::str::contains("unknown field 'no_such_section'"));
}

#[test]
fn profile_table_output_renders_sections() {
    let harness = Harness::seeded();
    harness
        .spotter(&["transcripts", "profile", "--format", "table"])
        .success()
        .stdout(predicate::str::contains("Profile: 2 sessions"))
        .stdout(predicate::str::contains("Tools:"))
        .stdout(predicate::str::contains("Token health:"))
        .stdout(predicate::str::contains("Outliers:"));
}
