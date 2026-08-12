//! Golden, determinism, and unit tests for the `cost` relations metric.
//!
//! The golden test builds a fixed [`SessionFacts`] set exercising even token
//! splitting, cross-turn/cross-session accumulation, coordinator inclusion, the
//! empty-turn skip, and relative tiering, then byte-compares the serialized
//! [`CostResult`]. Regenerate with `SPOTTER_REGEN_GOLDEN=1`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use spotter::metric_cost::cost;
use spotter::session_facts::{CoordinationClass, RelationsOptions, SessionFacts, TurnUsage};

const GOLDEN: &str = "tests/golden/metric_cost/cost.json";

fn opts() -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap: 100,
    }
}

fn turn(message: &str, tokens: i64, files: &[&str]) -> TurnUsage {
    TurnUsage {
        message_id: message.to_string(),
        attributed_tokens: tokens,
        files: files.iter().map(|f| (*f).to_string()).collect(),
    }
}

/// A single-rig session and a coordinator session.
///
/// `a.rs` accumulates an even split (500 of the 1000-token `m1` turn shared with
/// `b.rs`) plus the whole 300-token `m2` turn -> 800 tokens over two turns.
/// `b.rs` keeps its 500-token half of `m1`. `m3` edits nothing and is skipped.
/// `d.rs` is edited in a zero-token turn (`m5`) so it appears with 0 tokens.
/// The coordinator's `c.go` (200 tokens) is kept: cost keeps per-file scalars
/// for coordinators.
fn fixed_facts() -> Vec<SessionFacts> {
    vec![
        SessionFacts {
            external_session_id: "sess-single".to_string(),
            coordination: CoordinationClass::Single,
            rigs: BTreeSet::from(["/srv/town/rig-a".to_string()]),
            edits: Vec::new(),
            reads: Vec::new(),
            turns: vec![
                turn(
                    "m1",
                    1000,
                    &["/srv/town/rig-a/a.rs", "/srv/town/rig-a/b.rs"],
                ),
                turn("m2", 300, &["/srv/town/rig-a/a.rs"]),
                turn("m3", 5000, &[]),
                turn("m5", 0, &["/srv/town/rig-a/d.rs"]),
            ],
            events: Vec::new(),
        },
        SessionFacts {
            external_session_id: "sess-coord".to_string(),
            coordination: CoordinationClass::MultiRig,
            rigs: BTreeSet::from(["/srv/town/rig-a".to_string(), "/srv/town/rig-b".to_string()]),
            edits: Vec::new(),
            reads: Vec::new(),
            turns: vec![turn("m4", 200, &["/srv/town/rig-b/c.go"])],
            events: Vec::new(),
        },
    ]
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
    assert_eq!(actual, expected, "cost result drifted from golden");
}

#[test]
fn cost_matches_golden() {
    let result = cost(&fixed_facts(), &opts());
    let rendered = format!("{}\n", serde_json::to_string_pretty(&result).expect("json"));
    assert_or_regen(&rendered);
}

#[test]
fn cost_is_deterministic() {
    let facts = fixed_facts();
    let opts = opts();
    let first = serde_json::to_vec(&cost(&facts, &opts)).expect("json");
    let second = serde_json::to_vec(&cost(&facts, &opts)).expect("json");
    assert_eq!(first, second, "cost serialization is not byte-stable");
}

#[test]
fn cost_splits_evenly_and_accumulates() {
    let result = cost(&fixed_facts(), &opts());
    let by_path = |path: &str| {
        result
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("missing {path}"))
    };

    // a.rs = 500 (half of m1) + 300 (m2) across two turns.
    let a = by_path("/srv/town/rig-a/a.rs");
    assert_eq!(a.tokens, 800);
    assert_eq!(a.turns, 2);

    // b.rs = 500 (the other half of m1), one turn.
    let b = by_path("/srv/town/rig-a/b.rs");
    assert_eq!(b.tokens, 500);
    assert_eq!(b.turns, 1);

    // Coordinator scalar cost is kept.
    let c = by_path("/srv/town/rig-b/c.go");
    assert_eq!(c.tokens, 200);
    assert_eq!(c.turns, 1);

    // Zero-token edit still surfaces the file with one contributing turn.
    let d = by_path("/srv/town/rig-a/d.rs");
    assert_eq!(d.tokens, 0);
    assert_eq!(d.turns, 1);
}

#[test]
fn cost_tiers_are_relative_to_the_max() {
    let result = cost(&fixed_facts(), &opts());
    let tier = |path: &str| {
        result
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .tier
    };
    // max = 800 (a.rs). 5*tokens/max, floored, clamped 0..=4.
    assert_eq!(tier("/srv/town/rig-a/a.rs"), 4); // 5*800/800 = 5 -> 4
    assert_eq!(tier("/srv/town/rig-a/b.rs"), 3); // 5*500/800 = 3
    assert_eq!(tier("/srv/town/rig-b/c.go"), 1); // 5*200/800 = 1
    assert_eq!(tier("/srv/town/rig-a/d.rs"), 0); // 5*0/800   = 0
}

#[test]
fn cost_sorted_desc_by_tokens_then_path() {
    let result = cost(&fixed_facts(), &opts());
    let order: Vec<&str> = result.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        order,
        vec![
            "/srv/town/rig-a/a.rs", // 800
            "/srv/town/rig-a/b.rs", // 500
            "/srv/town/rig-b/c.go", // 200
            "/srv/town/rig-a/d.rs", // 0
        ]
    );
}

#[test]
fn cost_of_empty_facts_is_empty() {
    let result = cost(&[], &opts());
    assert!(result.files.is_empty());
}
