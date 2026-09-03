//! Shared per-session facts mined from transcripts for the relations metrics.
//!
//! [`build_session_facts`] folds a lean transcript store (see
//! [`crate::scan::load_lean`]) into one [`SessionFacts`] per *logical* session:
//! a parent session and all of its subagent sidecars share an
//! `external_session_id`, so grouping on that key folds them together (grouping
//! on the internal `session_id`, which is `parent:agent:<id>` for subagents,
//! would split them).
//!
//! Every relations metric consumes the same `&[SessionFacts]` slice; the metric
//! modules never touch a database or re-parse transcripts.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::scan::{LeanMessage, LeanStore};
use crate::timestamp::Timestamp;

/// Options shared by every relations metric.
#[derive(Debug, Clone)]
pub struct RelationsOptions {
    /// Reference time used for windowing and recency.
    pub now: DateTime<Utc>,
    /// Metric window in days.
    pub since_days: u32,
    /// Fan-out cap `K`, applied only to pair emission.
    pub fanout_cap: usize,
}

/// A single file touch (read or edit) attributed to an assistant turn.
#[derive(Debug, Clone, Serialize)]
pub struct FileEvent {
    /// Canonical file path.
    pub path: String,
    /// RFC3339 timestamp of the tool call, when known.
    pub ts: Option<Timestamp>,
    /// `message.id` of the assistant turn that issued the tool call, when known.
    pub message_id: Option<String>,
}

/// Token usage for one assistant turn, deduped by `message.id`.
#[derive(Debug, Clone, Serialize)]
pub struct TurnUsage {
    /// `message.id` of the assistant turn.
    pub message_id: String,
    /// Attributed tokens, `output + cache_creation` (cache-read excluded).
    pub attributed_tokens: i64,
    /// Canonical files edited in this turn, sorted.
    pub files: Vec<String>,
}

/// The kind of a session event, used for active-time and discoverability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    /// A `Read` tool call.
    Read,
    /// A file-editing tool call (`Edit`/`Write`/`MultiEdit`/`NotebookEdit` or a
    /// Bash-mediated write).
    Edit,
    /// A `Grep` tool call.
    Grep,
    /// A `Glob` tool call.
    Glob,
    /// Any other tool call.
    Other,
}

/// One ordered tool-call event within a logical session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionEvent {
    /// RFC3339 timestamp, when known.
    pub ts: Option<Timestamp>,
    /// Event kind.
    pub kind: SessionEventKind,
    /// Canonical file path the event targets, when applicable.
    pub path: Option<String>,
    /// Whether the tool call succeeded (a failed `Read` still counts as an event).
    pub success: bool,
    /// `message.id` of the issuing assistant turn, when known.
    pub message_id: Option<String>,
}

/// Thresholds for the coordinator classification. Every other threshold in
/// this module is a named `const`; these two were the only inline magic
/// literals.
const COORD_RIG_THRESHOLD: usize = 1;
const COORD_CWD_THRESHOLD: usize = 3;

/// How broadly a session spreads across rigs and working directories.
///
/// `Single` sessions stay within one rig and at most [`COORD_CWD_THRESHOLD`]
/// cwds and feed the pairwise metrics; `MultiRig` and `ManyCwd` sessions are
/// coordinators and are excluded from them. Replacing the prior `is_coordinator:
/// bool` field, this named the two inline magic literals (`1` and `3`) that were
/// the only unnamed thresholds in the fold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationClass {
    /// One rig and at most [`COORD_CWD_THRESHOLD`] cwds: feeds pairwise metrics.
    Single,
    /// More than [`COORD_RIG_THRESHOLD`] rig.
    MultiRig,
    /// More than [`COORD_CWD_THRESHOLD`] cwd within a single rig.
    ManyCwd,
}

impl CoordinationClass {
    /// Classify a session by how many distinct rigs and cwds it touched.
    ///
    /// A session crossing both thresholds is `MultiRig` (rig spread dominates);
    /// this matches the prior `rigs.len() > 1 || distinct_cwds.len() > 3` guard.
    #[must_use]
    pub fn classify(rigs: &BTreeSet<String>, distinct_cwds: &BTreeSet<String>) -> Self {
        if rigs.len() > COORD_RIG_THRESHOLD {
            Self::MultiRig
        } else if distinct_cwds.len() > COORD_CWD_THRESHOLD {
            Self::ManyCwd
        } else {
            Self::Single
        }
    }

    /// Whether this class is a coordinator (excluded from pairwise metrics).
    ///
    /// Equivalent to `*self != Self::Single`; kept as a named accessor so the
    /// consumer sites read as the prior `is_coordinator` bool check.
    #[must_use]
    pub const fn is_coordinator(self) -> bool {
        !matches!(self, Self::Single)
    }
}

/// All facts mined for one logical session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionFacts {
    /// Logical grouping key (parent + sidecars folded).
    pub external_session_id: String,
    /// How broadly the session spreads; coordinators (`MultiRig`/`ManyCwd`) are
    /// excluded from pairwise metrics. See [`CoordinationClass::classify`].
    pub coordination: CoordinationClass,
    /// Canonical rig repo roots the session touched.
    pub rigs: BTreeSet<String>,
    /// File edits, in event order.
    pub edits: Vec<FileEvent>,
    /// File reads, in event order.
    pub reads: Vec<FileEvent>,
    /// Per-turn token usage, deduped by `message.id`.
    pub turns: Vec<TurnUsage>,
    /// Ordered tool events for active-time and discoverability.
    pub events: Vec<SessionEvent>,
}

impl SessionFacts {
    /// The most recent event timestamp in the session, used for provenance
    /// recency ordering.
    #[must_use]
    pub fn last_activity(&self) -> Option<Timestamp> {
        self.events.iter().filter_map(|event| event.ts).max()
    }
}

/// Provenance byproduct: the most-recent logical session ids that touched a file.
#[derive(Debug, Clone, Serialize)]
pub struct FileProvenance {
    /// Canonical file path.
    pub path: String,
    /// Most-recent (up to 20) logical session ids that touched the file.
    pub session_ids: Vec<String>,
}

/// Folds worktree paths onto their canonical repo root and classifies whether a
/// working directory belongs to a known rig.
#[derive(Debug, Clone)]
pub struct Canonicalizer {
    /// Worktree path prefix → canonical repo root, longest prefix wins.
    worktree_map: Vec<(String, String)>,
    /// Known canonical rig roots, longest first for prefix matching.
    rig_roots: Vec<String>,
}

/// The in-repo worktree marker stripped during canonicalization.
const WORKTREE_MARKER: &str = "/.claude/worktrees/";

/// Strip every `/.claude/worktrees/<name>/` segment, folding an in-repo worktree
/// path back onto its main tree (the regex-free equivalent of the old
/// `replace_all`). Terminates because each replacement shortens the string.
fn strip_worktree_segments(path: &str) -> String {
    let mut result = path.to_string();
    while let Some(start) = result.find(WORKTREE_MARKER) {
        let name_start = start + WORKTREE_MARKER.len();
        // The `<name>` segment must end in a `/`; without one there is nothing
        // to fold and we leave the tail untouched.
        let Some(offset) = result[name_start..].find('/') else {
            break;
        };
        result.replace_range(start..=name_start + offset, "/");
    }
    result
}

impl Canonicalizer {
    /// Build a canonicalizer from a worktree map and the set of known rig roots.
    ///
    /// `worktree_map` maps an absolute worktree path to its canonical repo root.
    /// When `rig_roots` is empty the noise filter degrades to accepting every
    /// working directory (git-only environments still produce usable facts).
    #[must_use]
    pub fn new(worktree_map: BTreeMap<String, String>, rig_roots: BTreeSet<String>) -> Self {
        let mut worktree_map: Vec<(String, String)> = worktree_map.into_iter().collect();
        // Longest prefix first so nested worktrees resolve before their parents.
        worktree_map.sort_by(|left, right| {
            right
                .0
                .len()
                .cmp(&left.0.len())
                .then_with(|| left.0.cmp(&right.0))
        });
        let mut rig_roots: Vec<String> = rig_roots.into_iter().collect();
        rig_roots.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
        Self {
            worktree_map,
            rig_roots,
        }
    }

    /// Fold a raw path onto its canonical form: first apply the worktree map,
    /// then strip any residual `/.claude/worktrees/<name>/` segment.
    #[must_use]
    pub fn canonical(&self, raw: &str) -> String {
        let mapped = self.apply_worktree_map(raw);
        strip_worktree_segments(&mapped)
    }

    fn apply_worktree_map(&self, raw: &str) -> String {
        for (prefix, root) in &self.worktree_map {
            if raw == prefix {
                return root.clone();
            }
            if let Some(rest) = raw.strip_prefix(prefix) {
                if rest.starts_with('/') {
                    return format!("{root}{rest}");
                }
            }
        }
        raw.to_string()
    }

    /// The canonical rig root a working directory belongs to, if any.
    #[must_use]
    pub fn rig_for(&self, canonical_cwd: &str) -> Option<&str> {
        self.rig_roots
            .iter()
            .find(|root| under(canonical_cwd, root))
            .map(String::as_str)
    }

    /// Whether a working directory is under a known rig root.
    ///
    /// Degrades to `true` when no rig roots are known.
    #[must_use]
    pub fn known(&self, canonical_cwd: &str) -> bool {
        self.rig_roots.is_empty() || self.rig_for(canonical_cwd).is_some()
    }
}

fn under(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Build a worktree map and rig-root set by querying git across the working
/// directories observed in the store.
///
/// For each distinct cwd, `git worktree list --porcelain` yields every worktree
/// of that repo; the first entry is the canonical main tree. Errors (a cwd that
/// is gone, or not a git repo) are ignored so a partial checkout still produces
/// facts.
#[must_use]
pub fn build_canonicalizer(store: &LeanStore) -> Canonicalizer {
    let mut cwds: BTreeSet<String> = BTreeSet::new();
    for session in &store.sessions {
        if let Some(cwd) = &session.cwd {
            cwds.insert(cwd.clone());
        }
    }
    for message in &store.messages {
        if let Some(cwd) = &message.cwd {
            cwds.insert(cwd.clone());
        }
    }

    let mut worktree_map: BTreeMap<String, String> = BTreeMap::new();
    let mut rig_roots: BTreeSet<String> = BTreeSet::new();
    let mut resolved: BTreeSet<String> = BTreeSet::new();
    for cwd in &cwds {
        // Skip cwds already covered by a discovered worktree entry.
        if worktree_map.contains_key(cwd) {
            continue;
        }
        let Some(worktrees) = git_worktrees(cwd) else {
            continue;
        };
        let Some((canonical, _)) = worktrees.first() else {
            continue;
        };
        if !resolved.insert(canonical.clone()) {
            // Already learned this repo from another cwd.
            continue;
        }
        rig_roots.insert(canonical.clone());
        for (path, _) in &worktrees {
            worktree_map.insert(path.clone(), canonical.clone());
        }
    }

    Canonicalizer::new(worktree_map, rig_roots)
}

/// Parse `git -C <dir> worktree list --porcelain` into `(path, branch)` pairs,
/// the main tree first. Returns `None` when the command fails.
fn git_worktrees(dir: &str) -> Option<Vec<(String, Option<String>)>> {
    let output = std::process::Command::new("git")
        .args(["-C", dir, "worktree", "list", "--porcelain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current: Option<String> = None;
    let mut branch: Option<String> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(prev) = current.take() {
                worktrees.push((prev, branch.take()));
            }
            current = Some(path.to_string());
        } else if let Some(name) = line.strip_prefix("branch ") {
            branch = Some(name.to_string());
        }
    }
    if let Some(prev) = current.take() {
        worktrees.push((prev, branch.take()));
    }
    if worktrees.is_empty() {
        None
    } else {
        Some(worktrees)
    }
}

/// A typed, zero-allocation view over the raw `tool_name` string stored on a
/// [`crate::db::ToolCallRun`].
///
/// The string is kept as `String` at the DB/serialization boundary because
/// transcripts may carry arbitrary tool names and an unknown value must never
/// fail to parse. This borrowed newtype is constructed at the read seam and
/// centralizes the [`SessionEventKind`] classification — plus the edit and
/// Bash recognition — that was previously scattered across the free `classify`
/// function, `is_edit_tool`, and inline `== "Bash"` literals. It mirrors the
/// already-typed [`SessionEventKind`] the classification produces.
#[derive(Debug, Clone, Copy)]
pub struct ToolName<'a>(&'a str);

impl<'a> ToolName<'a> {
    /// The `Bash` tool name.
    const BASH: &'static str = "Bash";

    /// Wrap a raw `tool_name` string without allocating.
    #[must_use]
    pub const fn new(name: &'a str) -> Self {
        Self(name)
    }

    /// The raw tool name string.
    #[must_use]
    pub const fn as_str(&self) -> &'a str {
        self.0
    }

    /// Whether this is a `Bash` tool call.
    #[must_use]
    pub fn is_bash(&self) -> bool {
        self.0 == Self::BASH
    }

    /// Whether this is a file-editing tool (`Edit`/`Write`/`MultiEdit`/`NotebookEdit`).
    #[must_use]
    pub fn is_edit(&self) -> bool {
        EDIT_TOOLS.contains(&self.0)
    }

    /// Classify the tool into a [`SessionEventKind`].
    #[must_use]
    pub fn classify(&self) -> SessionEventKind {
        match self.0 {
            "Read" => SessionEventKind::Read,
            "Grep" => SessionEventKind::Grep,
            "Glob" => SessionEventKind::Glob,
            _ if self.is_edit() => SessionEventKind::Edit,
            _ => SessionEventKind::Other,
        }
    }
}

const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Whether a tool name is a file-editing tool.
///
/// Delegates to [`ToolName::is_edit`] so the string-match lives on the typed
/// view; kept as a free function for callers that already hold a raw `&str`.
#[must_use]
pub fn is_edit_tool(tool: &str) -> bool {
    ToolName::new(tool).is_edit()
}

/// Detect files written by a Bash command, best-effort, from the command string.
///
/// Recognizes `sed -i`, `tee`, `git mv`/`mv`, `cat > file`, heredoc redirection,
/// and plain `>`/`>>` stdout redirection. False positives are acceptable; the
/// caller canonicalizes and de-duplicates the results.
#[must_use]
pub fn bash_write_targets(command: &str) -> Vec<String> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut targets: BTreeSet<String> = BTreeSet::new();

    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        // Stdout redirection: `>`, `>>`, `1>`, and attached `>file` forms.
        if let Some(rest) = token.strip_prefix(">>").or_else(|| token.strip_prefix('>')) {
            if rest.is_empty() {
                if let Some(next) = tokens.get(index + 1) {
                    push_path(&mut targets, next);
                    index += 1;
                }
            } else {
                push_path(&mut targets, rest);
            }
        } else if token == "1>" || token == "1>>" {
            if let Some(next) = tokens.get(index + 1) {
                push_path(&mut targets, next);
                index += 1;
            }
        } else if token == "tee" {
            for next in &tokens[index + 1..] {
                if next.starts_with('-') {
                    continue;
                }
                if next.starts_with('|') || next.starts_with('>') || *next == ";" || *next == "&&" {
                    break;
                }
                push_path(&mut targets, next);
            }
        }
        index += 1;
    }

    // `sed -i[suffix] ... FILE`: treat the trailing operand as the edited file.
    if tokens.iter().any(|token| *token == "sed")
        && tokens.iter().any(|token| token.starts_with("-i"))
    {
        if let Some(last) = tokens.last() {
            push_path(&mut targets, last);
        }
    }

    // `git mv SRC DST` / `mv SRC DST`: both operands are canonical file touches.
    if is_move_command(&tokens) {
        for token in tokens.iter().rev().take(2) {
            push_path(&mut targets, token);
        }
    }

    targets.into_iter().collect()
}

fn is_move_command(tokens: &[&str]) -> bool {
    match tokens.first().copied() {
        Some("mv") => true,
        Some("git") => tokens.get(1).copied() == Some("mv"),
        _ => false,
    }
}

fn push_path(targets: &mut BTreeSet<String>, raw: &str) {
    let trimmed = raw.trim_matches(|ch: char| matches!(ch, '\'' | '"' | '`' | '(' | ')' | ';'));
    if trimmed.is_empty()
        || trimmed.starts_with('-')
        || trimmed.starts_with('$')
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
    {
        return;
    }
    let looks_like_path = trimmed.contains('/')
        || trimmed
            .rsplit_once('.')
            .is_some_and(|(_, ext)| !ext.is_empty() && ext.chars().all(char::is_alphanumeric));
    if looks_like_path {
        targets.insert(trimmed.to_string());
    }
}

/// Fold a lean transcript store into one [`SessionFacts`] per logical session.
///
/// Applies path canonicalization, the per-event noise filter (events outside a
/// known rig are dropped, not whole sessions), the coordinator guard, and token
/// dedup by `message.id`. The output is deterministic: sessions are keyed and
/// sorted by `external_session_id`, and every inner vector is ordered.
// One linear fold over the store; splitting the per-run classification out would
// obscure the single pass it deliberately makes.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn build_session_facts(
    store: &LeanStore,
    canon: &Canonicalizer,
    opts: &RelationsOptions,
) -> Vec<SessionFacts> {
    let _ = opts;

    // internal session_id -> session cwd, for the per-event cwd fallback.
    let mut session_cwd: BTreeMap<String, Option<String>> = BTreeMap::new();
    for session in &store.sessions {
        session_cwd.insert(session.id.clone(), session.cwd.clone());
    }

    // (internal session_id, ordinal) -> assistant turn message id + cwd.
    let mut message_index: BTreeMap<(String, i64), &LeanMessage> = BTreeMap::new();
    for message in &store.messages {
        message_index.insert((message.session_id.clone(), message.ordinal), message);
    }

    // Group runs by logical (external) session id, ordered deterministically.
    let mut runs_by_logical: BTreeMap<String, Vec<&crate::db::ToolCallRun>> = BTreeMap::new();
    for run in &store.runs {
        runs_by_logical
            .entry(run.external_session_id.clone())
            .or_default()
            .push(run);
    }
    for runs in runs_by_logical.values_mut() {
        // Order by wall-clock timestamp so a parent and its subagent sidecars —
        // which carry *independent* per-transcript ordinals — interleave
        // chronologically. Sorting on `start_ordinal` alone would place a
        // low-ordinal subagent event before a high-ordinal parent event that
        // actually happened earlier, corrupting the read-precedes-edit ordering
        // docs_steer and discoverability depend on. Runs without a timestamp fall
        // to the back; ordinal (chronological *within* one transcript) then
        // tool_use_id break ties deterministically.
        runs.sort_by(|left, right| {
            (left.started_at.is_none(), left.started_at)
                .cmp(&(right.started_at.is_none(), right.started_at))
                .then_with(|| {
                    left.start_ordinal
                        .unwrap_or(i64::MAX)
                        .cmp(&right.start_ordinal.unwrap_or(i64::MAX))
                })
                .then_with(|| left.tool_use_id.cmp(&right.tool_use_id))
        });
    }

    // Group lean messages by logical session id for turn usage.
    let mut messages_by_logical: BTreeMap<String, Vec<&LeanMessage>> = BTreeMap::new();
    for message in &store.messages {
        messages_by_logical
            .entry(message.external_session_id.clone())
            .or_default()
            .push(message);
    }

    let mut facts = Vec::new();
    for (external_session_id, runs) in &runs_by_logical {
        let mut edits: Vec<FileEvent> = Vec::new();
        let mut reads: Vec<FileEvent> = Vec::new();
        let mut events: Vec<SessionEvent> = Vec::new();
        let mut rigs: BTreeSet<String> = BTreeSet::new();
        let mut distinct_cwds: BTreeSet<String> = BTreeSet::new();
        // message_id -> canonical files edited within that turn.
        let mut turn_files: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for run in runs {
            let ordinal = run.start_ordinal;
            let message =
                ordinal.and_then(|ord| message_index.get(&(run.session_id.clone(), ord)).copied());
            let message_id = message.and_then(|message| message.message_id.clone());
            let raw_cwd = message
                .and_then(|message| message.cwd.clone())
                .or_else(|| session_cwd.get(&run.session_id).cloned().flatten())
                .or_else(|| run.canonical_cwd.clone());
            let Some(raw_cwd) = raw_cwd else {
                continue;
            };
            let canon_cwd = canon.canonical(&raw_cwd);
            if !canon.known(&canon_cwd) {
                continue;
            }
            distinct_cwds.insert(canon_cwd.clone());
            if let Some(rig) = canon.rig_for(&canon_cwd) {
                rigs.insert(rig.to_string());
            }

            let ts = run.started_at;
            let success = run.status != "error";
            let tool = ToolName::new(&run.tool_name);
            let kind = tool.classify();
            match kind {
                SessionEventKind::Read => {
                    // A failed Read (file-not-found, etc.) loaded nothing, so it
                    // must not enter the reads cohort the friction / read_clusters
                    // metrics consume. It still records a SessionEvent (with
                    // success=false) so discoverability sees the search effort.
                    if success {
                        for raw in &run.file_paths {
                            let path = canon.canonical(raw);
                            reads.push(FileEvent {
                                path: path.clone(),
                                ts,
                                message_id: message_id.clone(),
                            });
                        }
                    }
                    events.push(SessionEvent {
                        ts,
                        kind,
                        path: run.file_paths.first().map(|raw| canon.canonical(raw)),
                        success,
                        message_id: message_id.clone(),
                    });
                }
                SessionEventKind::Edit => {
                    let paths: Vec<String> = run
                        .file_paths
                        .iter()
                        .map(|raw| canon.canonical(raw))
                        .collect();
                    // A failed Edit (stale old_string, etc.) changed nothing, so it
                    // must not count toward rework / cochange_session / cost. The
                    // SessionEvent below still records the attempt for active time.
                    if success {
                        record_edits(
                            &paths,
                            &ts,
                            message_id.as_ref(),
                            &mut edits,
                            &mut turn_files,
                        );
                    }
                    events.push(SessionEvent {
                        ts,
                        kind,
                        path: paths.first().cloned(),
                        success,
                        message_id: message_id.clone(),
                    });
                }
                _ if tool.is_bash() => {
                    let mut paths: Vec<String> = Vec::new();
                    if let Some(command) = &run.command {
                        for raw in bash_write_targets(command) {
                            paths.push(canon.canonical(&raw));
                        }
                    }
                    let kind = if paths.is_empty() {
                        SessionEventKind::Other
                    } else {
                        SessionEventKind::Edit
                    };
                    // A failed Bash write changed nothing, so it must not be
                    // attributed as an edit; the SessionEvent still records it.
                    if success {
                        record_edits(
                            &paths,
                            &ts,
                            message_id.as_ref(),
                            &mut edits,
                            &mut turn_files,
                        );
                    }
                    events.push(SessionEvent {
                        ts,
                        kind,
                        path: paths.first().cloned(),
                        success,
                        message_id: message_id.clone(),
                    });
                }
                other => {
                    events.push(SessionEvent {
                        ts,
                        kind: other,
                        path: None,
                        success,
                        message_id: message_id.clone(),
                    });
                }
            }
        }

        // Turn usage: assistant messages with token usage, deduped by message.id.
        let mut turns_by_id: BTreeMap<String, TurnUsage> = BTreeMap::new();
        if let Some(messages) = messages_by_logical.get(external_session_id) {
            for message in messages {
                if message.role.as_deref() != Some("assistant") {
                    continue;
                }
                let Some(message_id) = &message.message_id else {
                    continue;
                };
                if message.input_tokens.is_none() {
                    continue;
                }
                let attributed = message.output_tokens + message.cache_creation_input_tokens;
                turns_by_id.entry(message_id.clone()).or_insert(TurnUsage {
                    message_id: message_id.clone(),
                    attributed_tokens: attributed,
                    files: turn_files
                        .get(message_id)
                        .map(|files| files.iter().cloned().collect())
                        .unwrap_or_default(),
                });
            }
        }
        let turns: Vec<TurnUsage> = turns_by_id.into_values().collect();

        if edits.is_empty() && reads.is_empty() && turns.is_empty() {
            continue;
        }

        let coordination = CoordinationClass::classify(&rigs, &distinct_cwds);
        facts.push(SessionFacts {
            external_session_id: external_session_id.clone(),
            coordination,
            rigs,
            edits,
            reads,
            turns,
            events,
        });
    }

    facts
}

fn record_edits(
    paths: &[String],
    ts: &Option<Timestamp>,
    message_id: Option<&String>,
    edits: &mut Vec<FileEvent>,
    turn_files: &mut BTreeMap<String, BTreeSet<String>>,
) {
    for path in paths {
        edits.push(FileEvent {
            path: path.clone(),
            ts: *ts,
            message_id: message_id.cloned(),
        });
        if let Some(message_id) = message_id {
            turn_files
                .entry(message_id.clone())
                .or_default()
                .insert(path.clone());
        }
    }
}

/// Restrict facts to file paths under a canonical root prefix.
///
/// Edits, reads, path-bearing events, and per-turn file lists are filtered to
/// paths under `under`; path-less events (Grep/Glob/Other) are kept for
/// active-time context. Sessions left with no edits, reads, or turns are dropped.
#[must_use]
pub fn filter_under(facts: Vec<SessionFacts>, root: &str) -> Vec<SessionFacts> {
    let keep = |path: &str| under(path, root);
    facts
        .into_iter()
        .filter_map(|session| {
            let edits: Vec<FileEvent> = session
                .edits
                .into_iter()
                .filter(|event| keep(&event.path))
                .collect();
            let reads: Vec<FileEvent> = session
                .reads
                .into_iter()
                .filter(|event| keep(&event.path))
                .collect();
            let events: Vec<SessionEvent> = session
                .events
                .into_iter()
                .filter(|event| event.path.as_deref().map_or(true, keep))
                .collect();
            let turns: Vec<TurnUsage> = session
                .turns
                .into_iter()
                .map(|mut turn| {
                    turn.files.retain(|path| keep(path));
                    turn
                })
                .collect();
            if edits.is_empty() && reads.is_empty() && turns.is_empty() {
                return None;
            }
            Some(SessionFacts {
                external_session_id: session.external_session_id,
                coordination: session.coordination,
                rigs: session.rigs,
                edits,
                reads,
                turns,
                events,
            })
        })
        .collect()
}

/// Build the provenance list: for each file touched (edited or read), the
/// most-recent (up to 20) logical session ids that touched it.
#[must_use]
pub fn build_provenance(facts: &[SessionFacts]) -> Vec<FileProvenance> {
    // path -> (session_id -> most-recent ts seen for that session/file).
    let mut per_file: BTreeMap<String, BTreeMap<String, Option<Timestamp>>> = BTreeMap::new();
    for session in facts {
        let recency = session.last_activity();
        let mut touch = |path: &str| {
            let entry = per_file
                .entry(path.to_string())
                .or_default()
                .entry(session.external_session_id.clone())
                .or_insert(None);
            if recency > *entry {
                *entry = recency;
            }
        };
        for event in session.edits.iter().chain(&session.reads) {
            touch(&event.path);
        }
    }

    let mut provenance: Vec<FileProvenance> = per_file
        .into_iter()
        .map(|(path, sessions)| {
            let mut ordered: Vec<(String, Option<Timestamp>)> = sessions.into_iter().collect();
            // Most recent first; ties broken by session id for determinism.
            ordered.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            let session_ids = ordered
                .into_iter()
                .map(|(id, _)| id)
                .take(20)
                .collect::<Vec<_>>();
            FileProvenance { path, session_ids }
        })
        .collect();
    provenance.sort_by(|left, right| left.path.cmp(&right.path));
    provenance
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon_with_rigs(roots: &[&str]) -> Canonicalizer {
        let rig_roots = roots.iter().map(|root| (*root).to_string()).collect();
        Canonicalizer::new(BTreeMap::new(), rig_roots)
    }

    #[test]
    fn canonical_strips_in_repo_worktree_segment() {
        let canon = canon_with_rigs(&[]);
        assert_eq!(
            canon.canonical("/srv/town/rig/.claude/worktrees/wt-1/src/a.rs"),
            "/srv/town/rig/src/a.rs"
        );
    }

    #[test]
    fn canonical_folds_sibling_worktree_via_map() {
        let mut map = BTreeMap::new();
        map.insert(
            "/srv/town/havi-content-wt".to_string(),
            "/srv/town/havi".to_string(),
        );
        let canon = Canonicalizer::new(map, BTreeSet::new());
        assert_eq!(
            canon.canonical("/srv/town/havi-content-wt/lib/page.ex"),
            "/srv/town/havi/lib/page.ex"
        );
    }

    #[test]
    fn known_degrades_to_true_without_rig_roots() {
        let canon = canon_with_rigs(&[]);
        assert!(canon.known("/anywhere/at/all"));
    }

    #[test]
    fn rig_for_matches_longest_root() {
        let canon = canon_with_rigs(&["/srv/town/rig", "/srv/town/rig/nested"]);
        assert_eq!(
            canon.rig_for("/srv/town/rig/nested/a.rs"),
            Some("/srv/town/rig/nested")
        );
        assert_eq!(canon.rig_for("/srv/town/rig/a.rs"), Some("/srv/town/rig"));
        assert_eq!(canon.rig_for("/elsewhere/a.rs"), None);
    }

    #[test]
    fn bash_write_targets_detects_redirect_and_tee() {
        assert_eq!(
            bash_write_targets("echo hi > out/log.txt"),
            vec!["out/log.txt"]
        );
        assert_eq!(bash_write_targets("cat foo >>notes.md"), vec!["notes.md"]);
        assert_eq!(
            bash_write_targets("printf x | tee -a config/app.toml"),
            vec!["config/app.toml"]
        );
    }

    #[test]
    fn bash_write_targets_detects_sed_and_mv() {
        assert_eq!(
            bash_write_targets("sed -i s/a/b/ src/main.rs"),
            vec!["src/main.rs"]
        );
        let moved = bash_write_targets("git mv old/name.rs new/name.rs");
        assert!(moved.contains(&"old/name.rs".to_string()));
        assert!(moved.contains(&"new/name.rs".to_string()));
    }

    #[test]
    fn bash_write_targets_ignores_plain_commands() {
        assert!(bash_write_targets("cargo test --all").is_empty());
        assert!(bash_write_targets("grep -rn foo src").is_empty());
    }

    /// Build a minimal [`ToolCallRun`], varying only the fields the fold reads.
    /// The `canonical_cwd` fallback stands in for a per-message cwd, and an empty
    /// rig set makes `Canonicalizer::known` degrade to accepting it.
    #[allow(clippy::too_many_arguments)]
    fn mk_run(
        external: &str,
        session: &str,
        tool: &str,
        file: &str,
        status: &str,
        started_at: &str,
        ordinal: i64,
    ) -> crate::db::ToolCallRun {
        crate::db::ToolCallRun {
            tool_use_id: format!("{session}-{ordinal}"),
            session_id: session.to_string(),
            external_session_id: external.to_string(),
            parent_session_id: None,
            is_subagent: false,
            agent_id: None,
            tool_name: tool.to_string(),
            command: None,
            command_program: None,
            command_args: Vec::new(),
            command_fingerprint: None,
            input_summary: None,
            input_size: None,
            output_size: None,
            file_paths: vec![file.to_string()],
            status: status.to_string(),
            started_at: Timestamp::parse(started_at),
            finished_at: None,
            duration_ms: None,
            start_ordinal: Some(ordinal),
            end_ordinal: None,
            source_scope: None,
            error_content: None,
            project_alias: String::new(),
            worktree_name: None,
            canonical_cwd: Some("/srv/town/rig".to_string()),
            read_total_lines: None,
            read_lines: None,
            read_truncated: None,
        }
    }

    fn facts_from_runs(runs: Vec<crate::db::ToolCallRun>) -> Vec<SessionFacts> {
        let store = crate::scan::LeanStore {
            runs,
            ..Default::default()
        };
        let opts = RelationsOptions {
            now: Utc::now(),
            since_days: 30,
            fanout_cap: 100,
        };
        build_session_facts(&store, &canon_with_rigs(&[]), &opts)
    }

    #[test]
    fn failed_read_is_excluded_from_reads_but_kept_as_event() {
        let facts = facts_from_runs(vec![
            mk_run(
                "s",
                "s",
                "Read",
                "/srv/town/rig/a.rs",
                "error",
                "2026-06-01T10:00:00+00:00",
                0,
            ),
            mk_run(
                "s",
                "s",
                "Read",
                "/srv/town/rig/b.rs",
                "ok",
                "2026-06-01T10:01:00+00:00",
                1,
            ),
        ]);
        assert_eq!(facts.len(), 1);
        let session = &facts[0];
        // Only the successful read enters the reads cohort.
        assert_eq!(session.reads.len(), 1);
        assert_eq!(session.reads[0].path, "/srv/town/rig/b.rs");
        // Both reads still record an ordered event; the failed one is flagged.
        assert_eq!(session.events.len(), 2);
        assert!(!session.events[0].success);
        assert!(session.events[1].success);
    }

    #[test]
    fn failed_edit_is_excluded_from_edits() {
        let facts = facts_from_runs(vec![
            mk_run(
                "s",
                "s",
                "Edit",
                "/srv/town/rig/a.rs",
                "error",
                "2026-06-01T10:00:00+00:00",
                0,
            ),
            // A successful read keeps the session alive so we can inspect it.
            mk_run(
                "s",
                "s",
                "Read",
                "/srv/town/rig/b.rs",
                "ok",
                "2026-06-01T10:01:00+00:00",
                1,
            ),
        ]);
        assert_eq!(facts.len(), 1);
        let session = &facts[0];
        assert!(session.edits.is_empty(), "failed edit must not enter edits");
        assert_eq!(session.reads.len(), 1);
        assert_eq!(session.events.len(), 2);
    }

    #[test]
    fn folded_subagent_runs_order_by_timestamp_not_ordinal() {
        // Parent reads a doc at parent-ordinal 8; a subagent (same external id)
        // edits code at subagent-ordinal 2 but a LATER timestamp. Folded and
        // sorted by timestamp, the parent read must precede the subagent edit
        // despite the lower ordinal — the ordering docs_steer depends on.
        let parent = mk_run(
            "ext",
            "ext",
            "Read",
            "/srv/town/rig/docs/arch.md",
            "ok",
            "2026-06-01T10:00:00+00:00",
            8,
        );
        let mut sub = mk_run(
            "ext",
            "ext:agent:a1",
            "Edit",
            "/srv/town/rig/src/app.rs",
            "ok",
            "2026-06-01T10:05:00+00:00",
            2,
        );
        sub.is_subagent = true;

        // Input order reversed to prove the sort, not insertion order, decides.
        let facts = facts_from_runs(vec![sub, parent]);
        assert_eq!(facts.len(), 1, "parent + subagent fold into one session");
        let session = &facts[0];
        assert_eq!(session.events.len(), 2);
        assert_eq!(
            session.events[0].kind,
            SessionEventKind::Read,
            "earlier-timestamp read must come first"
        );
        assert_eq!(
            session.events[1].kind,
            SessionEventKind::Edit,
            "later-timestamp edit must come second"
        );
        assert_eq!(session.reads.len(), 1);
        assert_eq!(session.edits.len(), 1);
    }

    #[test]
    fn tool_name_new_and_as_str_are_const_fn() {
        const NAME: ToolName<'static> = ToolName::new("const-marker");
        const AS_STR: &str = NAME.as_str();
        assert_eq!(AS_STR, "const-marker");
    }
}
