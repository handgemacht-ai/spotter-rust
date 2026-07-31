//! Ranked full-text retrieval: BM25 ordering, `--exact` phrase semantics,
//! chunk-to-ordinal resolution, and DB/scan parity on the `ranked` fixture.
//!
//! Fixture layout (tests/fixtures/ranked/ranked.jsonl, kept out of
//! tests/fixtures/transcripts so the shared relations corpus is unaffected):
//! - ordinal 1: "zephyr quokka checklist" (both terms, short)
//! - ordinal 2: "zephyr" x5 (highest term frequency)
//! - ordinal 3: both terms, but not the phrase "quokka checklist"
//! - ordinal 5: >2000-char tool result, both terms near the end (chunked)

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;

const FIXTURE: &str = "tests/fixtures/ranked/ranked.jsonl";
const SESSION: &str = "7f3a9c1e-2b4d-4e5f-8a6b-9c0d1e2f3a4b";

fn harness() -> (NamedTempFile, NamedTempFile) {
    let db = NamedTempFile::new().expect("temp db");
    let config = NamedTempFile::new().expect("temp config");
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
            FIXTURE,
        ])
        .assert()
        .success();
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

fn json(db: &NamedTempFile, config: &NamedTempFile, args: &[&str]) -> Value {
    let output = run(db, config, args).success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("valid json")
}

fn content_search(needle: &str, extra: &[&str]) -> Vec<String> {
    let (db, config) = harness();
    let mut args = vec![
        "transcripts",
        "search",
        "--content-contains",
        needle,
        "--format",
        "json",
    ];
    args.extend_from_slice(extra);
    json(&db, &config, &args)
        .as_array()
        .expect("hit array")
        .iter()
        .map(|hit| hit["ordinal"].to_string())
        .collect()
}

#[test]
fn default_match_orders_by_bm25_relevance() {
    let ordinals = content_search("zephyr", &[]);
    assert_eq!(ordinals, ["2", "1", "3", "5"]);

    let (db, config) = harness();
    let hits = json(
        &db,
        &config,
        &[
            "transcripts",
            "search",
            "--content-contains",
            "zephyr",
            "--format",
            "json",
        ],
    );
    let scores: Vec<f64> = hits
        .as_array()
        .expect("hit array")
        .iter()
        .map(|hit| hit["score"].as_f64().expect("numeric score"))
        .collect();
    assert!(
        scores.windows(2).all(|pair| pair[0] <= pair[1]),
        "scores must be ordered best-first (lower is better): {scores:?}"
    );
}

#[test]
fn default_match_requires_all_terms() {
    // Ordinal 2 lacks "quokka" and drops out of the AND match.
    let ordinals = content_search("zephyr quokka", &[]);
    assert_eq!(ordinals, ["1", "3", "5"]);
}

#[test]
fn exact_keeps_phrase_semantics() {
    // Ordinal 3 contains both terms but not the adjacent phrase.
    let exact = content_search("quokka checklist", &["--exact"]);
    assert_eq!(exact, ["1"]);
    let ranked = content_search("quokka checklist", &[]);
    assert_eq!(ranked, ["1", "3"]);
}

#[test]
fn chunk_hits_resolve_to_parent_ordinal_and_drill_down() {
    let (db, config) = harness();
    let hits = json(
        &db,
        &config,
        &[
            "transcripts",
            "search",
            "--content-contains",
            "quokka",
            "--format",
            "json",
        ],
    );
    let long_hit = hits
        .as_array()
        .expect("hit array")
        .iter()
        .find(|hit| hit["ordinal"] == 5)
        .expect("long message hit");
    let snippet = long_hit["snippet"].as_str().expect("snippet");
    assert!(
        snippet.contains("quokka"),
        "snippet must anchor at the match past the chunk boundary: {snippet}"
    );

    // The parent ordinal feeds straight into the deliverable-1 drill-down.
    let runs = json(
        &db,
        &config,
        &[
            "transcripts",
            "inspect",
            "--session",
            SESSION,
            "--ordinals",
            "5:5",
            "--format",
            "json",
        ],
    );
    let runs = runs.as_array().expect("run array");
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0]["tool_use_id"].as_str().unwrap(),
        "toolu_ranked_read"
    );
}

#[test]
fn transcripts_and_scan_emit_byte_identical_ranked_json() {
    let (db, config) = harness();
    for (needle, flags) in [
        ("zephyr", vec![]),
        ("zephyr quokka", vec![]),
        ("quokka checklist", vec!["--exact"]),
    ] {
        let mut base = vec!["search", "--content-contains", needle];
        base.extend(flags.iter().copied());
        base.extend(["--format", "json"]);

        let mut transcripts_args = vec!["transcripts"];
        transcripts_args.extend(base.iter().copied());
        let mut scan_args = vec!["scan", "--file", FIXTURE];
        scan_args.extend(base.iter().copied());

        let transcripts = run(&db, &config, &transcripts_args)
            .success()
            .get_output()
            .stdout
            .clone();
        let scan = run(&db, &config, &scan_args)
            .success()
            .get_output()
            .stdout
            .clone();
        assert_eq!(
            String::from_utf8_lossy(&transcripts),
            String::from_utf8_lossy(&scan),
            "transcripts/scan ranked output diverged for {needle:?} {flags:?}"
        );
    }
}

#[test]
fn needle_without_tokens_matches_nothing_on_both_backends() {
    let (db, config) = harness();
    for args in [
        vec![
            "transcripts",
            "search",
            "--content-contains",
            "!!!",
            "--format",
            "json",
        ],
        vec![
            "scan",
            "--file",
            FIXTURE,
            "search",
            "--content-contains",
            "!!!",
            "--format",
            "json",
        ],
    ] {
        let hits = json(&db, &config, &args);
        assert_eq!(hits.as_array().expect("hit array").len(), 0);
    }
}

#[test]
fn fields_projection_covers_score_and_snippet() {
    let (db, config) = harness();
    let hits = json(
        &db,
        &config,
        &[
            "transcripts",
            "search",
            "--content-contains",
            "zephyr",
            "--format",
            "json",
            "--fields",
            "ordinal,score,snippet",
        ],
    );
    for hit in hits.as_array().expect("hit array") {
        let keys: Vec<&str> = hit
            .as_object()
            .expect("hit object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["ordinal", "score", "snippet"]);
    }
}

#[test]
fn content_search_table_output_still_works() {
    let (db, config) = harness();
    run(
        &db,
        &config,
        &["transcripts", "search", "--content-contains", "zephyr"],
    )
    .success()
    .stdout(predicate::str::contains(
        "session_id | ordinal | role | snippet",
    ));
}
