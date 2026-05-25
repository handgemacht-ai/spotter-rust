//! In-memory transcript store and JSONL loader for the DB-less `scan` path.
//!
//! Walks Claude Code JSONL transcripts, parses each via [`crate::jsonl`], and
//! materializes the same data the SQLite-backed path stores: a list of
//! [`SessionRecord`], the derived [`ToolCallRun`]s, normalized
//! [`StoredMessage`]s for content search, and per-session [`UsageMessage`]s
//! for token-health analysis.
//!
//! The analytics functions in [`crate::analytics`] take this store's slices
//! and produce the same results the DB path produces, so `scan <verb>` and
//! `transcripts <verb>` stay in lock-step.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::analytics::{StoredMessage, UsageMessage};
use crate::config::Config;
use crate::db::{
    self, all_transcript_files_under, content_text, derive_runs, is_subagent_transcript,
    session_record_from_parsed, transcript_files_under, SessionRecord, ToolCallRun,
};
use crate::jsonl::{self, ParsedSession, TranscriptMessage};

/// In-memory transcript dataset used by every `scan <verb>`.
#[derive(Debug, Default)]
pub struct Store {
    /// Every parsed session.
    pub sessions: Vec<SessionRecord>,
    /// Every derived tool-call run.
    pub runs: Vec<ToolCallRun>,
    /// Every normalized message, in walk order.
    pub messages: Vec<StoredMessage>,
    /// Usage-bearing messages, keyed by internal session id.
    pub usage_by_session: Vec<(SessionRecord, Vec<UsageMessage>)>,
    /// Files that failed to parse, with the underlying error.
    pub errors: Vec<(PathBuf, anyhow::Error)>,
}

impl Store {
    /// Find a session by internal id or external id, preferring main sessions over subagents.
    pub fn find_session(&self, id: &str) -> Option<&SessionRecord> {
        let mut matches = self
            .sessions
            .iter()
            .filter(|session| session.id == id || session.external_session_id == id)
            .collect::<Vec<_>>();
        matches.sort_by_key(|session| i32::from(session.is_subagent));
        matches.first().copied()
    }
}

/// Resolve the JSONL files to scan from CLI args and config.
///
/// The returned list is deduplicated and sorted. When neither `files` nor
/// `roots` is provided and the config has no transcript roots, the default
/// is `~/.claude/projects` and `~/.claude_agents/projects` (both, when they
/// exist).
pub fn collect_targets(
    files: &[PathBuf],
    roots: &[PathBuf],
    no_subagents: bool,
    config: &Config,
) -> Vec<PathBuf> {
    let mut targets = files.to_vec();
    let mut all_roots = roots.to_vec();
    if files.is_empty() && roots.is_empty() {
        all_roots.extend(default_roots(config));
    }
    for root in all_roots {
        let resolved = if no_subagents {
            transcript_files_under(&root)
        } else {
            all_transcript_files_under(&root)
        };
        targets.extend(resolved);
    }
    targets.sort();
    targets.dedup();
    targets
}

fn default_roots(config: &Config) -> Vec<PathBuf> {
    if !config.transcript_roots.is_empty() {
        return config.transcript_roots.clone();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut roots = Vec::new();
    if let Some(home) = home {
        for candidate in [
            home.join(".claude").join("projects"),
            home.join(".claude_agents").join("projects"),
        ] {
            if candidate.exists() {
                roots.push(candidate);
            }
        }
    }
    roots
}

/// Walk the targets and populate a [`Store`].
///
/// Parsing failures are collected into `store.errors` rather than aborted on,
/// matching the behavior of the existing scan CLI.
pub fn load(targets: &[PathBuf], config: &Config, cancel: &AtomicBool) -> Result<Store> {
    let mut store = Store::default();
    for path in targets {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("operation cancelled by SIGINT");
        }
        let subagent = is_subagent_transcript(path);
        let parsed = if subagent {
            jsonl::parse_subagent_file(path)
        } else {
            jsonl::parse_session_file(path)
        };
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                store.errors.push((path.clone(), anyhow::Error::new(error)));
                continue;
            }
        };
        let project_alias = config.alias_for_cwd(parsed.cwd.as_deref());
        let session = match session_record_from_parsed(&parsed, path, &project_alias, None) {
            Ok(session) => session,
            Err(error) => {
                store.errors.push((path.clone(), error));
                continue;
            }
        };
        append_to_store(&mut store, &parsed, &session);
        store.sessions.push(session);
    }
    Ok(store)
}

fn append_to_store(store: &mut Store, parsed: &ParsedSession, session: &SessionRecord) {
    store.runs.extend(derive_runs(parsed, session));
    let usage = parsed
        .messages
        .iter()
        .filter_map(UsageMessage::from_transcript_message)
        .collect::<Vec<_>>();
    store
        .usage_by_session
        .push((session.clone(), usage));
    for message in &parsed.messages {
        store.messages.push(stored_message_from(session, message));
    }
}

fn stored_message_from(session: &SessionRecord, message: &TranscriptMessage) -> StoredMessage {
    StoredMessage {
        session_id: session.id.clone(),
        external_session_id: session.external_session_id.clone(),
        project_alias: session.project_alias.clone(),
        ordinal: message.ordinal,
        record_type: message.normalized_type.clone(),
        role: message.role.clone(),
        timestamp: message.timestamp.map(|timestamp| timestamp.to_rfc3339()),
        search_text: content_text(&message.content),
    }
}

/// Audit-style line counts and type histograms for one JSONL file.
///
/// Mirrors what `transcripts audit --file` reports, minus the
/// `imported_messages` comparison (the scan path has no DB to compare
/// against, so it reports the file's own JSONL line count as "imported").
pub fn audit_file(path: &Path) -> Result<AuditFileReport> {
    let parsed = jsonl::parse_session_file(path)
        .with_context(|| format!("failed to parse transcript {}", path.display()))?;
    let session_id = parsed
        .session_id
        .clone()
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToString::to_string)
        })
        .context("could not determine session id")?;
    let jsonl_lines = db::count_jsonl_lines(path)?;
    let mut types = std::collections::BTreeMap::new();
    for message in &parsed.messages {
        *types.entry(message.normalized_type.clone()).or_insert(0) += 1;
    }
    Ok(AuditFileReport {
        session_id,
        file: path.display().to_string(),
        jsonl_lines,
        parsed_messages: parsed.messages.len() as i64,
        jsonl_types: types,
    })
}

/// Per-file audit row for `spotter scan audit`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditFileReport {
    /// External session id (from JSONL or filename stem).
    pub session_id: String,
    /// Transcript path.
    pub file: String,
    /// Total non-empty JSONL lines.
    pub jsonl_lines: i64,
    /// Successfully-parsed messages.
    pub parsed_messages: i64,
    /// Histogram of normalized message types.
    pub jsonl_types: std::collections::BTreeMap<String, i64>,
}

/// Heuristic plain-text helper used by older callers — exposed so tests can
/// validate the same flattening logic used inside the store.
#[doc(hidden)]
pub fn plain_text_of(value: &Value) -> String {
    content_text(value)
}
