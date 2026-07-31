//! Keeps `docs/json-schema.md` honest: the key sets in the doc's `json-keys`
//! block are extracted and compared against real `--format json` output, so
//! the documented shapes cannot silently drift from the CLI.

#![allow(clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::NamedTempFile;

const DOC: &str = include_str!("../docs/json-schema.md");
const TOOL_HEAVY: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const SHORT: &str = "tests/fixtures/transcripts/short.jsonl";
const TOOL_SESSION: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";
const EXEMPLAR: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f:toolu_018GZVh9ymkrdx1TnR8reg5Y";

/// The `similar.*` doc entries need the local embedding model and FAIL when
/// it is absent (fail-fast, like the CLI): run `spotter embed init` once, or
/// point `SPOTTER_MODEL_DIR` at a populated cache. They are skipped only on an
/// explicit opt-out via `SPOTTER_SKIP_MODEL_TESTS=1`.
fn require_model() -> bool {
    let available = spotter::paths::model_dir(None)
        .map(|dir| spotter::embed::ensure_model(&dir).is_ok())
        .unwrap_or(false);
    if available {
        return true;
    }
    if std::env::var_os("SPOTTER_SKIP_MODEL_TESTS").is_some() {
        eprintln!("skipping similar.* doc checks: SPOTTER_SKIP_MODEL_TESTS is set");
        return false;
    }
    panic!(
        "embedding model not cached; run `spotter embed init` or set \
         SPOTTER_MODEL_DIR to the model cache dir (set SPOTTER_SKIP_MODEL_TESTS=1 \
         to skip model-dependent tests)"
    );
}

#[test]
fn documented_json_keys_match_cli_output() {
    let db = NamedTempFile::new().expect("temp db");
    let db_path = db.path().to_str().expect("utf8 temp path");
    let config = NamedTempFile::new().expect("temp config");
    let config_path = config.path().to_str().expect("utf8 temp path");

    for fixture in [TOOL_HEAVY, SHORT] {
        Command::cargo_bin("spotter")
            .expect("binary")
            .args([
                "--db",
                db_path,
                "--config",
                config_path,
                "transcripts",
                "sync",
                "--file",
                fixture,
            ])
            .assert()
            .success();
    }

    let cases: &[(&str, &[&str], &str)] = &[
        (
            "search.run",
            &["transcripts", "search", "--limit", "1", "--format", "json"],
            "[]",
        ),
        (
            "search.message_hit",
            &[
                "transcripts",
                "search",
                "--content-contains",
                "phoenix",
                "--limit",
                "1",
                "--format",
                "json",
            ],
            "[]",
        ),
        (
            "search.session_group",
            &[
                "transcripts",
                "search",
                "--limit",
                "1",
                "--group-by-session",
                "--format",
                "json",
            ],
            "[]",
        ),
        (
            "inspect.run",
            &[
                "transcripts",
                "inspect",
                "--session",
                TOOL_SESSION,
                "--format",
                "json",
            ],
            "[]",
        ),
        (
            "inspect.with_messages",
            &[
                "transcripts",
                "inspect",
                "--session",
                TOOL_SESSION,
                "--with-messages",
                "--format",
                "json",
            ],
            "",
        ),
        (
            "inspect.message_hit",
            &[
                "transcripts",
                "inspect",
                "--session",
                TOOL_SESSION,
                "--with-messages",
                "--format",
                "json",
            ],
            "context[]",
        ),
        (
            "errors.top_level",
            &["transcripts", "errors", "--format", "json"],
            "",
        ),
        (
            "errors.pattern",
            &["transcripts", "errors", "--format", "json"],
            "patterns[]",
        ),
        (
            "aggregate.top_level",
            &["transcripts", "aggregate", "--format", "json"],
            "",
        ),
        (
            "aggregate.group",
            &["transcripts", "aggregate", "--format", "json"],
            "groups[]",
        ),
        (
            "aggregate.top_error",
            &["transcripts", "aggregate", "--format", "json"],
            "top_errors[]",
        ),
        (
            "sequences.top_level",
            &[
                "transcripts",
                "sequences",
                "--min-occurrences",
                "1",
                "--format",
                "json",
            ],
            "",
        ),
        (
            "sequences.row",
            &[
                "transcripts",
                "sequences",
                "--min-occurrences",
                "1",
                "--format",
                "json",
            ],
            "frequent_sequences[]",
        ),
        (
            "sequences.recovery_row",
            &[
                "transcripts",
                "sequences",
                "--min-occurrences",
                "1",
                "--recovery",
                "--format",
                "json",
            ],
            "recovery_stats[]",
        ),
        ("profile.top_level", &["transcripts", "profile"], ""),
        ("profile.overview", &["transcripts", "profile"], "overview"),
        ("profile.tool", &["transcripts", "profile"], "tools[]"),
        (
            "profile.error_pattern",
            &["transcripts", "profile"],
            "errors.top_patterns[]",
        ),
        (
            "profile.error_category",
            &["transcripts", "profile"],
            "errors.categories[]",
        ),
        (
            "profile.recovery_row",
            &["transcripts", "profile"],
            "recovery[]",
        ),
        (
            "profile.token_health",
            &["transcripts", "profile"],
            "token_health",
        ),
        (
            "profile.flagged_session",
            &["transcripts", "profile"],
            "token_health.flagged_sessions[]",
        ),
        (
            "profile.slowest_run",
            &["transcripts", "profile"],
            "outliers.slowest_runs[]",
        ),
        (
            "profile.error_hotspot",
            &["transcripts", "profile"],
            "outliers.error_hotspots[]",
        ),
        (
            "sample.top_level",
            &["transcripts", "sample", "-n", "3"],
            "",
        ),
        (
            "sample.run",
            &["transcripts", "sample", "-n", "3"],
            "runs[]",
        ),
        (
            "sample.stratum",
            &["transcripts", "sample", "-n", "3"],
            "strata[]",
        ),
        (
            "similar.run",
            &["transcripts", "similar", "--to-run", EXEMPLAR, "-n", "1"],
            "[]",
        ),
    ];

    let model_available = require_model();
    let mut actual = BTreeMap::new();
    for (label, args, path) in cases {
        if label.starts_with("similar.") && !model_available {
            continue;
        }
        let output = Command::cargo_bin("spotter")
            .expect("binary")
            .args(["--db", db_path, "--config", config_path])
            .args(*args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let json = serde_json::from_slice::<Value>(&output).expect("valid json");
        actual.insert((*label).to_string(), keys_at(&json, path));
    }

    // The scan backend must expose the same documented shape.
    let scan_output = Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db_path,
            "--config",
            config_path,
            "scan",
            "--file",
            TOOL_HEAVY,
            "search",
            "--limit",
            "1",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scan_json = serde_json::from_slice::<Value>(&scan_output).expect("valid json");
    assert_eq!(
        actual["search.run"],
        keys_at(&scan_json, "[]"),
        "scan search run keys differ from transcripts search run keys"
    );

    let documented = documented_key_sets();
    let documented = documented
        .into_iter()
        .filter(|(label, _)| model_available || !label.starts_with("similar."))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        documented.keys().collect::<Vec<_>>(),
        actual.keys().collect::<Vec<_>>(),
        "doc json-keys labels and test cases out of sync; update both together"
    );
    for (label, keys) in &actual {
        assert_eq!(
            &documented[label], keys,
            "documented keys for '{label}' drifted from CLI output; \
             update docs/json-schema.md or revert the shape change"
        );
    }
}

/// Parse the `json-keys` code block: one `label: key, key, ...` entry per line.
fn documented_key_sets() -> BTreeMap<String, BTreeSet<String>> {
    let block = DOC
        .split("```json-keys")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("docs/json-schema.md must contain a ```json-keys block");
    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (label, keys) = line
                .split_once(':')
                .unwrap_or_else(|| panic!("malformed json-keys line: {line}"));
            (
                label.trim().to_string(),
                keys.split(',').map(|key| key.trim().to_string()).collect(),
            )
        })
        .collect()
}

/// Top-level keys at `path`: dot-separated object keys, with `[]` selecting
/// the first array element ("", "[]", "key", "key[]", "key.nested[]").
fn keys_at(value: &Value, path: &str) -> BTreeSet<String> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        if segment == "[]" {
            current = &current
                .as_array()
                .unwrap_or_else(|| panic!("path '{path}' is not an array"))[0];
            continue;
        }
        if let Some(key) = segment.strip_suffix("[]") {
            current = &current
                .get(key)
                .unwrap_or_else(|| panic!("missing key '{key}' for path '{path}'"))
                .as_array()
                .unwrap_or_else(|| panic!("path '{path}' is not an array"))[0];
        } else {
            current = current
                .get(segment)
                .unwrap_or_else(|| panic!("missing key '{segment}' for path '{path}'"));
        }
    }
    current
        .as_object()
        .unwrap_or_else(|| panic!("path '{path}' is not an object"))
        .keys()
        .cloned()
        .collect()
}
