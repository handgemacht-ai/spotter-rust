//! Golden and behaviour tests for the session co-change metric.
//!
//! The golden test builds a fixed [`SessionFacts`] corpus and byte-compares the
//! serialised [`CochangeSessionResult`]; regenerate with `SPOTTER_REGEN_GOLDEN=1`.
//! The remaining tests pin the definitional edges the golden alone cannot make
//! obvious: coordinator exclusion, directional asymmetry, and the fan-out cap.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use spotter::metric_cochange_session::cochange_session;
use spotter::session_facts::{CoordinationClass, FileEvent, RelationsOptions, SessionFacts};

const GOLDEN: &str = "tests/golden/metric_cochange_session/pairs.json";

fn opts(fanout_cap: usize) -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap,
    }
}

/// One logical session that edited `edits` (reads/turns/events irrelevant here).
fn sess(id: &str, coordinator: bool, edits: &[&str]) -> SessionFacts {
    SessionFacts {
        external_session_id: id.to_string(),
        coordination: if coordinator {
            CoordinationClass::MultiRig
        } else {
            CoordinationClass::Single
        },
        rigs: BTreeSet::new(),
        edits: edits
            .iter()
            .map(|&path| FileEvent {
                path: path.to_string(),
                ts: None,
                message_id: None,
            })
            .collect(),
        reads: Vec::new(),
        turns: Vec::new(),
        events: Vec::new(),
    }
}

/// Push `count` sessions, each editing `edits`, with ids prefixed by `tag`.
fn push_group(
    out: &mut Vec<SessionFacts>,
    tag: &str,
    count: usize,
    coordinator: bool,
    edits: &[&str],
) {
    for index in 0..count {
        out.push(sess(&format!("{tag}-{index}"), coordinator, edits));
    }
}

/// A corpus exercising every branch of the metric definition.
fn corpus() -> Vec<SessionFacts> {
    let mut facts = Vec::new();
    // Asymmetry via a hub: hub+leaf co-edited 3×, hub edited alone 5× → the
    // solo edits raise sessions_editing(hub) to 8 so hub→leaf (3/8 = 0.375)
    // falls below threshold while leaf→hub (3/3 = 1.0) survives.
    push_group(&mut facts, "a-pair", 3, false, &["hub.rs", "leaf.rs"]);
    push_group(&mut facts, "a-solo", 5, false, &["hub.rs"]);
    // Both directions survive: widget+gadget co-edited 4×, never alone.
    push_group(&mut facts, "b-pair", 4, false, &["widget.rs", "gadget.rs"]);
    // Support fails: alpha+beta co-edited only twice (< 3).
    push_group(&mut facts, "c-pair", 2, false, &["alpha.rs", "beta.rs"]);
    // Coordinator exclusion: core+util co-edited twice by real sessions plus once
    // by a coordinator; counting the coordinator would reach support 3, so the
    // pair must stay absent.
    push_group(&mut facts, "d-pair", 2, false, &["core.rs", "util.rs"]);
    push_group(&mut facts, "d-coord", 1, true, &["core.rs", "util.rs"]);
    // Non-trivial confidence: engine+spark co-edited 3×, engine alone 4× →
    // engine→spark = 3/7 = 0.4286 rounds to 0.43 (survives), spark→engine = 1.0.
    push_group(&mut facts, "e-pair", 3, false, &["engine.rs", "spark.rs"]);
    push_group(&mut facts, "e-solo", 4, false, &["engine.rs"]);
    facts
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN)
}

fn assert_or_regen(actual: &str) {
    let path = golden_path();
    if std::env::var_os("SPOTTER_REGEN_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create golden dir");
        }
        fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden {}; regenerate with SPOTTER_REGEN_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "cochange_session output drifted from golden"
    );
}

#[test]
fn cochange_matches_golden() {
    let result = cochange_session(&corpus(), &opts(100));
    let rendered = format!("{}\n", serde_json::to_string_pretty(&result).expect("json"));
    assert_or_regen(&rendered);
}

#[test]
fn cochange_is_deterministic() {
    let facts = corpus();
    let first = serde_json::to_vec(&cochange_session(&facts, &opts(100))).expect("json");
    let second = serde_json::to_vec(&cochange_session(&facts, &opts(100))).expect("json");
    assert_eq!(first, second, "cochange_session output is not byte-stable");
}

#[test]
fn coordinators_are_excluded_from_pairs() {
    // Three coordinator sessions co-editing the same two files would clear the
    // support threshold if counted — they must not.
    let mut facts = Vec::new();
    push_group(&mut facts, "coord", 3, true, &["x.rs", "y.rs"]);
    let result = cochange_session(&facts, &opts(100));
    assert!(
        result.pairs.is_empty(),
        "coordinator sessions leaked into pairs: {:?}",
        result.pairs
    );
}

#[test]
fn confidence_is_asymmetric() {
    let result = cochange_session(&corpus(), &opts(100));
    let has = |a: &str, b: &str| result.pairs.iter().any(|pair| pair.a == a && pair.b == b);
    // leaf→hub survives, hub→leaf is dropped by the confidence threshold.
    assert!(has("leaf.rs", "hub.rs"), "leaf→hub should survive");
    assert!(
        !has("hub.rs", "leaf.rs"),
        "hub→leaf should be dropped (conf < 0.4)"
    );
    // Support is symmetric; confidence is what differs.
    let leaf_hub = result
        .pairs
        .iter()
        .find(|pair| pair.a == "leaf.rs" && pair.b == "hub.rs")
        .expect("leaf→hub present");
    assert_eq!(leaf_hub.support, 3);
    assert!((leaf_hub.confidence - 1.0).abs() < 1e-9);
}

#[test]
fn fanout_cap_bounds_pair_emission() {
    // Three sessions each editing five files. With cap 2 only the first two
    // sorted files (f0, f1) form pairs; f2..f4 are held out of emission.
    let mut facts = Vec::new();
    push_group(
        &mut facts,
        "big",
        3,
        false,
        &["f0.rs", "f1.rs", "f2.rs", "f3.rs", "f4.rs"],
    );

    let capped = cochange_session(&facts, &opts(2));
    assert!(
        capped
            .pairs
            .iter()
            .all(|pair| ["f0.rs", "f1.rs"].contains(&pair.a.as_str())
                && ["f0.rs", "f1.rs"].contains(&pair.b.as_str())),
        "cap=2 leaked a pair outside the first two files: {:?}",
        capped.pairs
    );
    assert_eq!(capped.pairs.len(), 2, "cap=2 should emit only f0↔f1");

    // Lifting the cap re-admits the held-out pairs (5 files → 20 ordered pairs).
    let uncapped = cochange_session(&facts, &opts(100));
    assert_eq!(uncapped.pairs.len(), 20, "uncapped should emit every pair");
}
