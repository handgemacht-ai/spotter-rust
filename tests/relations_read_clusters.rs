//! Golden and behavioural tests for the read-together clusters metric.
//!
//! The golden test builds a fixed [`SessionFacts`] set and byte-compares the
//! serialized [`ReadClustersResult`] as it appears inside the relations
//! envelope; regenerate with `SPOTTER_REGEN_GOLDEN=1`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use spotter::metric_read_clusters::read_clusters;
use spotter::session_facts::{FileEvent, RelationsOptions, SessionFacts};

const GOLDEN: &str = "tests/golden/scan_relations/read_clusters.json";

fn opts() -> RelationsOptions {
    RelationsOptions {
        now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        since_days: 30,
        fanout_cap: 100,
    }
}

/// Build a read-only logical session from a list of read file paths.
fn reading_session(id: &str, coordinator: bool, reads: &[&str]) -> SessionFacts {
    SessionFacts {
        external_session_id: id.to_string(),
        is_coordinator: coordinator,
        rigs: BTreeSet::new(),
        edits: Vec::new(),
        reads: reads
            .iter()
            .map(|path| FileEvent {
                path: (*path).to_string(),
                ts: None,
                message_id: None,
            })
            .collect(),
        turns: Vec::new(),
        events: Vec::new(),
    }
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
    assert_eq!(actual, expected, "read_clusters result drifted from golden");
}

/// A representative fact set: one 3-file cluster (5 sessions), one 2-file
/// cluster (4 sessions), a sub-threshold pair (3 sessions), and a coordinator
/// that reads everything but must be excluded.
fn fixed_facts() -> Vec<SessionFacts> {
    let a = "/srv/town/rig-a/src/ast.rs";
    let b = "/srv/town/rig-a/src/lexer.rs";
    let c = "/srv/town/rig-a/src/parser.rs";
    let x = "/srv/town/rig-b/lib/client.go";
    let y = "/srv/town/rig-b/lib/server.go";
    let p = "/srv/town/rig-a/tmp/notes.rs";
    let q = "/srv/town/rig-a/tmp/scratch.rs";

    let mut facts = Vec::new();
    // Cluster 1: {a, b, c} co-read in 5 distinct sessions.
    for i in 0..5 {
        facts.push(reading_session(
            &format!("cluster1-s{i}"),
            false,
            &[a, b, c],
        ));
    }
    // Cluster 2: {x, y} co-read in 4 distinct sessions.
    for i in 0..4 {
        facts.push(reading_session(&format!("cluster2-s{i}"), false, &[x, y]));
    }
    // Sub-threshold: {p, q} co-read in only 3 sessions (support < 4).
    for i in 0..3 {
        facts.push(reading_session(&format!("noise-s{i}"), false, &[p, q]));
    }
    // Coordinator reading everything: excluded from pairwise metrics entirely.
    facts.push(reading_session("coordinator", true, &[a, b, c, x, y, p, q]));
    facts
}

#[test]
fn read_clusters_matches_golden() {
    let result = read_clusters(&fixed_facts(), &opts());
    // Render through `serde_json::Value` so the field order matches how the
    // result appears inside the relations envelope (`json!(...)`).
    let value = serde_json::to_value(&result).expect("to_value");
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("pretty json")
    );
    assert_or_regen(&rendered);
}

#[test]
fn coordinators_are_excluded() {
    // Only coordinator sessions co-read {a, b}: no cluster may form.
    let facts: Vec<SessionFacts> = (0..5)
        .map(|i| {
            reading_session(
                &format!("coord-{i}"),
                true,
                &["/srv/town/rig/src/a.rs", "/srv/town/rig/src/b.rs"],
            )
        })
        .collect();
    let result = read_clusters(&facts, &opts());
    assert!(
        result.clusters.is_empty(),
        "coordinator-only co-reads must not cluster: {:?}",
        result.clusters
    );
}

#[test]
fn low_jaccard_pair_is_dropped() {
    // A hub file read in 14 sessions; a partner read alongside it in 4 of them.
    // support = 4, union = 14, jaccard = 0.2857 < 0.3 -> no edge, no cluster.
    let hub = "/srv/town/rig/src/hub.rs";
    let partner = "/srv/town/rig/src/partner.rs";
    let facts: Vec<SessionFacts> = (0..14)
        .map(|i| {
            let reads: Vec<&str> = if i < 4 { vec![hub, partner] } else { vec![hub] };
            reading_session(&format!("s{i}"), false, &reads)
        })
        .collect();
    let result = read_clusters(&facts, &opts());
    assert!(
        result.clusters.is_empty(),
        "low-Jaccard hub pair must not cluster: {:?}",
        result.clusters
    );
}

#[test]
fn transitive_edges_form_one_component() {
    // a-b co-read in 4 sessions, b-c co-read in 4 other sessions; a and c are
    // never read together, yet connectivity through b yields one cluster.
    let a = "/srv/town/rig/src/a.rs";
    let b = "/srv/town/rig/src/b.rs";
    let c = "/srv/town/rig/src/c.rs";
    let mut facts = Vec::new();
    for i in 0..4 {
        facts.push(reading_session(&format!("ab-{i}"), false, &[a, b]));
    }
    for i in 0..4 {
        facts.push(reading_session(&format!("bc-{i}"), false, &[b, c]));
    }
    let result = read_clusters(&facts, &opts());
    assert_eq!(result.clusters.len(), 1, "{:?}", result.clusters);
    let cluster = &result.clusters[0];
    assert_eq!(cluster.size, 3);
    assert_eq!(
        cluster.files,
        vec![a.to_string(), b.to_string(), c.to_string()]
    );
    assert_eq!(cluster.label, "/srv/town/rig/src");
}

#[test]
fn oversized_cluster_is_capped_but_reports_true_size() {
    // 60 files all pairwise co-read in 4 sessions -> one 60-member component.
    // The emitted file list is capped at 50 while `size` stays 60.
    let files: Vec<String> = (0..60)
        .map(|i| format!("/srv/town/rig/mod/file{i:02}.rs"))
        .collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let facts: Vec<SessionFacts> = (0..4)
        .map(|i| reading_session(&format!("s{i}"), false, &refs))
        .collect();
    let result = read_clusters(&facts, &opts());
    assert_eq!(result.clusters.len(), 1);
    let cluster = &result.clusters[0];
    assert_eq!(cluster.size, 60, "true size preserved");
    assert_eq!(cluster.files.len(), 50, "emitted files capped");
    assert!(
        cluster.size > cluster.files.len(),
        "size > files.len() must flag the drop"
    );
    assert_eq!(cluster.label, "/srv/town/rig/mod");
}

#[test]
fn session_over_fanout_cap_emits_no_pairs() {
    // 5 sessions each read the same 10 files: under the default cap this is one
    // 10-member cluster, but a small fan-out cap marks each as a broad sweep and
    // skips it wholesale, so its O(n^2) co-read pairs are never emitted.
    let files: Vec<String> = (0..10)
        .map(|i| format!("/srv/town/rig/mod/file{i:02}.rs"))
        .collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let facts: Vec<SessionFacts> = (0..5)
        .map(|i| reading_session(&format!("s{i}"), false, &refs))
        .collect();

    // Default cap (100): the 10 files form one cluster.
    assert_eq!(read_clusters(&facts, &opts()).clusters.len(), 1);

    // Small cap (5): every session reads 10 > 5 distinct files, so all are
    // skipped and no cluster forms.
    let capped = RelationsOptions {
        fanout_cap: 5,
        ..opts()
    };
    assert!(
        read_clusters(&facts, &capped).clusters.is_empty(),
        "sessions over the fan-out cap must not emit co-read pairs"
    );
}

#[test]
fn output_is_deterministic() {
    let facts = fixed_facts();
    let first = serde_json::to_vec(&read_clusters(&facts, &opts())).expect("json");
    let second = serde_json::to_vec(&read_clusters(&facts, &opts())).expect("json");
    assert_eq!(
        first, second,
        "read_clusters serialization is not byte-stable"
    );
}
