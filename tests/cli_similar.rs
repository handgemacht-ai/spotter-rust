//! The `similar` verb: exemplar similarity via the local embedding model.
//!
//! Error paths are tested unconditionally. End-to-end ranking tests need the
//! model weights in the cache dir and FAIL when they are absent (fail-fast,
//! like the CLI): run `spotter embed init` once, or point `SPOTTER_MODEL_DIR`
//! at a populated cache. Environments without network access can opt out
//! explicitly with `SPOTTER_SKIP_MODEL_TESTS=1`, which skips just these tests
//! (with a note on stderr).

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;

const TOOL_HEAVY: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const SHORT: &str = "tests/fixtures/transcripts/short.jsonl";
const TOOL_SESSION: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";
const EXEMPLAR: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f:toolu_018GZVh9ymkrdx1TnR8reg5Y";

fn harness() -> (NamedTempFile, NamedTempFile) {
    let db = NamedTempFile::new().expect("temp db");
    let config = NamedTempFile::new().expect("temp config");
    for fixture in [TOOL_HEAVY, SHORT] {
        Command::cargo_bin("spotter")
            .expect("binary")
            .args([
                "--db",
                db.path().to_str().unwrap(),
                "--config",
                config.path().to_str().unwrap(),
                "transcripts",
                "sync",
                "--file",
                fixture,
            ])
            .assert()
            .success();
    }
    (db, config)
}

fn run(db: &NamedTempFile, config: &NamedTempFile, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--db",
            db.path().to_str().unwrap(),
            "--config",
            config.path().to_str().unwrap(),
        ])
        .args(args)
        .assert()
}

/// Fail fast when the embedding model is not cached; returns false only on an
/// explicit opt-out via `SPOTTER_SKIP_MODEL_TESTS=1`.
fn require_model(test: &str) -> bool {
    let available = spotter::paths::model_dir(None)
        .map(|dir| spotter::embed::ensure_model(&dir).is_ok())
        .unwrap_or(false);
    if available {
        return true;
    }
    if std::env::var_os("SPOTTER_SKIP_MODEL_TESTS").is_some() {
        eprintln!("skipping {test}: SPOTTER_SKIP_MODEL_TESTS is set");
        return false;
    }
    panic!(
        "{test}: embedding model not cached; run `spotter embed init` or set \
         SPOTTER_MODEL_DIR to the model cache dir (set SPOTTER_SKIP_MODEL_TESTS=1 \
         to skip model-dependent tests)"
    );
}

#[test]
fn invalid_run_id_is_rejected() {
    let (db, config) = harness();
    run(
        &db,
        &config,
        &["transcripts", "similar", "--to-run", "bogus"],
    )
    .failure()
    .stderr(predicate::str::contains("invalid --to-run id"));
}

#[test]
fn unknown_run_id_is_rejected_without_model() {
    let (db, config) = harness();
    run(
        &db,
        &config,
        &[
            "transcripts",
            "similar",
            "--to-run",
            &format!("{TOOL_SESSION}:toolu_nope"),
        ],
    )
    .failure()
    .stderr(predicate::str::contains("run not found"));
}

#[test]
fn missing_model_fails_fast_with_remedy() {
    let (db, config) = harness();
    let empty = tempfile::TempDir::new().expect("temp model dir");
    run(
        &db,
        &config,
        &[
            "transcripts",
            "similar",
            "--to-run",
            EXEMPLAR,
            "--model-dir",
            empty.path().to_str().unwrap(),
        ],
    )
    .failure()
    .stderr(predicate::str::contains("embedding model not found"))
    .stderr(predicate::str::contains("spotter embed init"));
}

#[test]
fn similar_ranks_exemplar_neighbours_and_excludes_itself() {
    if !require_model("similar_ranks_exemplar_neighbours_and_excludes_itself") {
        return;
    }
    let (db, config) = harness();
    let output = run(
        &db,
        &config,
        &["transcripts", "similar", "--to-run", EXEMPLAR, "-n", "4"],
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let hits: Vec<Value> = serde_json::from_slice(&output).expect("valid json");
    assert_eq!(hits.len(), 4);

    let similarities: Vec<f64> = hits
        .iter()
        .map(|hit| hit["similarity"].as_f64().expect("similarity"))
        .collect();
    assert!(
        similarities
            .windows(2)
            .all(|pair| pair[0] >= pair[1] && (-1.0..=1.0).contains(&pair[0])),
        "similarities must be descending in [-1, 1]: {similarities:?}"
    );
    assert!(hits
        .iter()
        .all(|hit| hit["run_id"].as_str().unwrap() != EXEMPLAR
            || hit["tool_use_id"].as_str().unwrap() != "toolu_018GZVh9ymkrdx1TnR8reg5Y"));
    // The exemplar is `mix phx.server`; its nearest neighbours should be the
    // other `mix` Bash commands, not the Read/Edit/Grep runs.
    assert_eq!(hits[0]["tool_name"].as_str().unwrap(), "Bash");
    assert!(hits[0]["command"]
        .as_str()
        .unwrap_or_default()
        .starts_with("mix "));
    // run_id decoration applies like any run array.
    assert_eq!(
        hits[0]["run_id"].as_str().unwrap(),
        format!(
            "{}:{}",
            hits[0]["session_id"].as_str().unwrap(),
            hits[0]["tool_use_id"].as_str().unwrap()
        )
    );
}

#[test]
fn similar_fields_projection_and_scoping() {
    if !require_model("similar_fields_projection_and_scoping") {
        return;
    }
    let (db, config) = harness();
    let output = run(
        &db,
        &config,
        &[
            "transcripts",
            "similar",
            "--to-run",
            EXEMPLAR,
            "--session",
            TOOL_SESSION,
            "-n",
            "3",
            "--fields",
            "run_id,similarity",
        ],
    )
    .success()
    .get_output()
    .stdout
    .clone();
    let hits: Vec<Value> = serde_json::from_slice(&output).expect("valid json");
    assert!(!hits.is_empty());
    for hit in &hits {
        let keys: Vec<&str> = hit
            .as_object()
            .expect("hit object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["run_id", "similarity"]);
        assert!(hit["run_id"].as_str().unwrap().starts_with(TOOL_SESSION));
    }
}
