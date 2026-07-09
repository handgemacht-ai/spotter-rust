//! Read-together clusters: files co-read within logical sessions, grouped into
//! connected components.

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// A connected component of files that are frequently read together.
#[derive(Debug, Clone, Serialize)]
pub struct Cluster {
    /// Human label, the deepest common directory of the member files.
    pub label: String,
    /// Member file paths, sorted.
    pub files: Vec<String>,
    /// Number of member files.
    pub size: usize,
}

/// Result payload for the read-together clusters metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReadClustersResult {
    /// Connected components meeting the Jaccard and support thresholds.
    pub clusters: Vec<Cluster>,
}

/// Compute read-together clusters (co-read pairs with `jaccard >= 0.3` and
/// `support >= 4`, joined into connected components; coordinators excluded).
///
/// Wave-0 stub: returns an empty, contract-valid result. A later slice fills in
/// the co-read pairing over [`SessionFacts::reads`].
#[must_use]
pub fn read_clusters(facts: &[SessionFacts], opts: &RelationsOptions) -> ReadClustersResult {
    let _ = (facts, opts);
    ReadClustersResult::default()
}
