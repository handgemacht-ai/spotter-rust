//! Read-together clusters: files co-read within logical sessions, grouped into
//! connected components.
//!
//! Two files form a co-read *edge* when they are read within the same logical
//! session in at least [`MIN_SUPPORT`] distinct sessions and their session-set
//! Jaccard similarity is at least [`MIN_JACCARD`]. Coordinator sessions (which
//! span many rigs or cwds) are excluded so a cross-cutting orchestration run
//! never fuses otherwise-unrelated files. Surviving edges are joined into
//! connected components; every component with two or more files is a cluster,
//! labelled by the deepest directory common to its members.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionFacts};

/// Minimum distinct sessions co-reading a pair for it to form an edge.
const MIN_SUPPORT: usize = 4;
/// Minimum session-set Jaccard similarity for a pair to form an edge.
const MIN_JACCARD: f64 = 0.3;
/// Upper bound on files *emitted* per cluster. Larger components keep their true
/// [`Cluster::size`] but list only the most-connected [`MAX_CLUSTER_FILES`]
/// members, so `size > files.len()` flags a truncated cluster.
const MAX_CLUSTER_FILES: usize = 50;

/// A connected component of files that are frequently read together.
#[derive(Debug, Clone, Serialize)]
pub struct Cluster {
    /// Human label, the deepest common directory of the member files.
    pub label: String,
    /// Member file paths, sorted; truncated to [`MAX_CLUSTER_FILES`] for very
    /// large components (see `size` for the true member count).
    pub files: Vec<String>,
    /// True number of member files, before any emit cap. `size > files.len()`
    /// signals that the emitted list was truncated.
    pub size: usize,
}

/// Result payload for the read-together clusters metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReadClustersResult {
    /// Connected components meeting the Jaccard and support thresholds, sorted
    /// desc by `size`, then by `label`, then by member files.
    pub clusters: Vec<Cluster>,
}

/// Compute read-together clusters (co-read pairs with `jaccard >= 0.3` and
/// `support >= 4`, joined into connected components; coordinators excluded).
///
/// Each cluster's `label` is the deepest directory common to its members and
/// its `size` is the true member count; the emitted `files` list is capped at
/// [`MAX_CLUSTER_FILES`] (a drop shows as `size > files.len()`).
#[must_use]
pub fn read_clusters(facts: &[SessionFacts], opts: &RelationsOptions) -> ReadClustersResult {
    // Windowing is applied upstream by the `--since` mtime prune. The fan-out cap
    // bounds pair emission: a session that reads more than `fanout_cap` distinct
    // files is a broad sweep, not focused co-reading, and emitting its full
    // O(n^2) co-read pairs would explode memory. Such sessions are skipped
    // wholesale, mirroring the coordinator guard and metric_cochange_session's cap.
    let fanout_cap = opts.fanout_cap;

    // Per file: the logical sessions that read it. Per unordered pair: the
    // logical sessions that read both. Session identity is the fact index.
    let mut file_sessions: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
    let mut pair_sessions: BTreeMap<(&str, &str), BTreeSet<usize>> = BTreeMap::new();

    for (idx, session) in facts.iter().enumerate() {
        if session.coordination.is_coordinator() {
            continue;
        }
        // Distinct files read in this session (BTreeSet keeps them sorted).
        let read_set: BTreeSet<&str> = session
            .reads
            .iter()
            .map(|event| event.path.as_str())
            .collect();
        // Cap pair emission: skip a session whose distinct-read count exceeds the
        // fan-out cap, bounding its pair contribution at C(fanout_cap, 2).
        if read_set.len() > fanout_cap {
            continue;
        }
        for file in &read_set {
            file_sessions.entry(file).or_default().insert(idx);
        }
        let files: Vec<&str> = read_set.into_iter().collect();
        for (i, left) in files.iter().enumerate() {
            for right in &files[i + 1..] {
                // `left < right` already holds (sorted iteration).
                pair_sessions.entry((left, right)).or_default().insert(idx);
            }
        }
    }

    // Keep only edges clearing both thresholds, and build the adjacency graph.
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for ((left, right), sessions) in &pair_sessions {
        let support = sessions.len();
        if support < MIN_SUPPORT {
            continue;
        }
        let sessions_left = file_sessions.get(left).map_or(0, BTreeSet::len);
        let sessions_right = file_sessions.get(right).map_or(0, BTreeSet::len);
        // support <= min(sessions_left, sessions_right), so union >= support > 0.
        let union = sessions_left + sessions_right - support;
        if union == 0 {
            continue;
        }
        let jaccard = support as f64 / union as f64;
        if jaccard < MIN_JACCARD {
            continue;
        }
        adjacency
            .entry((*left).to_string())
            .or_default()
            .insert((*right).to_string());
        adjacency
            .entry((*right).to_string())
            .or_default()
            .insert((*left).to_string());
    }

    let mut clusters: Vec<Cluster> = connected_components(&adjacency)
        .into_iter()
        .filter(|members| members.len() >= 2)
        .map(|members| build_cluster(&members, &adjacency))
        .collect();

    // Bigger clusters first; ties broken by label then member files for a total,
    // deterministic order.
    clusters.sort_by(|left, right| {
        right
            .size
            .cmp(&left.size)
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.files.cmp(&right.files))
    });

    ReadClustersResult { clusters }
}

/// Connected components of the co-read graph, discovered in sorted-key order.
///
/// Each returned component is sorted; the outer order is deterministic because
/// discovery walks `adjacency.keys()` (a `BTreeMap`) in ascending order.
fn connected_components(adjacency: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut components: Vec<Vec<String>> = Vec::new();
    for start in adjacency.keys() {
        if visited.contains(start.as_str()) {
            continue;
        }
        let mut members: BTreeSet<&str> = BTreeSet::new();
        let mut stack = vec![start.as_str()];
        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            members.insert(node);
            if let Some(neighbors) = adjacency.get(node) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor.as_str()) {
                        stack.push(neighbor.as_str());
                    }
                }
            }
        }
        components.push(members.into_iter().map(ToString::to_string).collect());
    }
    components
}

/// Turn a component's members into a [`Cluster`], applying the emit cap.
fn build_cluster(members: &[String], adjacency: &BTreeMap<String, BTreeSet<String>>) -> Cluster {
    let size = members.len();
    let label = deepest_common_dir(members);
    let files = if size <= MAX_CLUSTER_FILES {
        members.to_vec()
    } else {
        // Keep the most-connected members: intra-cluster degree desc, path asc.
        let mut ranked: Vec<&String> = members.iter().collect();
        ranked.sort_by(|left, right| {
            let degree_left = adjacency.get(*left).map_or(0, BTreeSet::len);
            let degree_right = adjacency.get(*right).map_or(0, BTreeSet::len);
            degree_right.cmp(&degree_left).then_with(|| left.cmp(right))
        });
        let mut kept: Vec<String> = ranked
            .into_iter()
            .take(MAX_CLUSTER_FILES)
            .cloned()
            .collect();
        kept.sort();
        kept
    };
    Cluster { label, files, size }
}

/// The deepest directory common to a set of file paths.
///
/// Compares the parent directories component-by-component (never mid-segment),
/// so `/a/b/c/x.rs` and `/a/b/d/y.rs` share `/a/b`. Falls back to `/` when the
/// only shared ancestor is the filesystem root.
fn deepest_common_dir(paths: &[String]) -> String {
    let parent = |path: &str| -> String {
        match path.rfind('/') {
            Some(0) => "/".to_string(),
            Some(idx) => path[..idx].to_string(),
            None => String::new(),
        }
    };
    let dirs: Vec<String> = paths.iter().map(|path| parent(path)).collect();
    let Some((first, rest)) = dirs.split_first() else {
        return String::new();
    };
    let mut common: Vec<&str> = first.split('/').collect();
    for dir in rest {
        let components: Vec<&str> = dir.split('/').collect();
        let shared = common
            .iter()
            .zip(&components)
            .take_while(|(lhs, rhs)| lhs == rhs)
            .count();
        common.truncate(shared);
        if common.is_empty() {
            break;
        }
    }
    let joined = common.join("/");
    if joined.is_empty() {
        "/".to_string()
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepest_common_dir_shares_parent_across_subdirs() {
        let paths = vec![
            "/srv/town/rig/a/one.rs".to_string(),
            "/srv/town/rig/b/two.rs".to_string(),
        ];
        assert_eq!(deepest_common_dir(&paths), "/srv/town/rig");
    }

    #[test]
    fn deepest_common_dir_same_directory() {
        let paths = vec![
            "/srv/town/rig/src/a.rs".to_string(),
            "/srv/town/rig/src/b.rs".to_string(),
        ];
        assert_eq!(deepest_common_dir(&paths), "/srv/town/rig/src");
    }

    #[test]
    fn deepest_common_dir_disjoint_roots_fall_back_to_root() {
        let paths = vec!["/alpha/a.rs".to_string(), "/beta/b.rs".to_string()];
        assert_eq!(deepest_common_dir(&paths), "/");
    }
}
