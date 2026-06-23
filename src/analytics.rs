//! Analytics over stored transcript sessions and derived tool call runs.

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use chrono::{DateTime, Utc};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::db::{self, MessageHit, SessionRecord, ToolCallRun};
use crate::jsonl::TranscriptMessage;

/// Common tool-call filters.
#[derive(Debug, Default)]
pub struct RunFilters {
    /// Project alias filter.
    pub project: Option<String>,
    /// Worktree name filter.
    pub worktree: Option<String>,
    /// Internal or external session id filter.
    pub session: Option<String>,
    /// Tool name filter.
    pub tool: Option<String>,
    /// Bash command substring filter.
    pub command_contains: Option<String>,
    /// Error content substring filter.
    pub error_contains: Option<String>,
    /// File path substring filter.
    pub file_path: Option<String>,
    /// Minimum duration.
    pub min_duration: Option<i64>,
    /// Maximum duration.
    pub max_duration: Option<i64>,
    /// Keep only `Read` runs whose file has at least this many total lines.
    pub min_read_lines: Option<i64>,
    /// Status filter.
    pub status: Option<String>,
    /// Lower bound timestamp or date.
    pub since: Option<String>,
    /// Maximum results.
    pub limit: Option<usize>,
}

/// Compare output for one cohort group.
#[derive(Debug, Clone, Serialize)]
pub struct CompareGroup {
    /// Group key.
    pub key: String,
    /// Tool call count.
    pub count: usize,
    /// Average duration in milliseconds.
    pub avg_duration_ms: Option<i64>,
}

/// Compare output.
#[derive(Debug, Clone, Serialize)]
pub struct CompareResult {
    /// Left cohort groups.
    pub left: Vec<CompareGroup>,
    /// Right cohort groups.
    pub right: Vec<CompareGroup>,
}

/// Aggregate output.
#[derive(Debug, Clone, Serialize)]
pub struct AggregateResult {
    /// Number of sessions represented.
    pub session_count: usize,
    /// Total run count.
    pub total_runs: usize,
    /// Grouped rows.
    pub groups: Vec<AggregateGroup>,
    /// Most common errors.
    pub top_errors: Vec<TopError>,
}

/// Aggregate row.
#[derive(Debug, Clone, Serialize)]
pub struct AggregateGroup {
    /// Group key values.
    pub key: BTreeMap<String, String>,
    /// Tool call count.
    pub count: usize,
    /// Error count.
    pub errors: usize,
    /// Error percentage.
    pub error_pct: f64,
    /// Median duration.
    pub avg_duration_ms: Option<i64>,
    /// P95 duration.
    pub p95_duration_ms: Option<i64>,
}

/// Top error row.
#[derive(Debug, Clone, Serialize)]
pub struct TopError {
    /// Normalized fingerprint.
    pub fingerprint: String,
    /// Occurrence count.
    pub count: usize,
    /// Representative tool name.
    pub tool_name: String,
    /// Representative sample.
    pub sample: String,
}

/// Error analysis output.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorAnalysisResult {
    /// Total matching tool calls.
    pub total_tool_calls: usize,
    /// Total matching failed tool calls.
    pub total_errors: usize,
    /// Number of distinct error patterns before top truncation.
    pub pattern_count: usize,
    /// Most common error patterns.
    pub patterns: Vec<ErrorPattern>,
}

/// Error analysis row.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorPattern {
    /// Tool name.
    pub tool_name: String,
    /// Normalized fingerprint.
    pub fingerprint: String,
    /// Occurrence count.
    pub count: usize,
    /// First timestamp.
    pub first_seen: Option<String>,
    /// Last timestamp.
    pub last_seen: Option<String>,
    /// Sample error content.
    pub sample_error: String,
    /// Sample session ids.
    pub sample_sessions: Vec<String>,
    /// Category when classification is requested.
    pub category: Option<String>,
    /// Preventability when classification is requested.
    pub preventability: Option<String>,
    /// Total calls for this tool when classification is requested.
    pub total_tool_calls: Option<usize>,
    /// Error rate when classification is requested.
    pub error_rate: Option<f64>,
}

/// Health analysis for one session.
#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    /// Number of usage-bearing messages.
    pub message_count: usize,
    /// Cache misses.
    pub cache_misses: Vec<CacheMiss>,
    /// Token jumps.
    pub jumps: Vec<TokenJump>,
    /// Detected cache window.
    pub cache_window: CacheWindow,
    /// Summary metrics.
    pub summary: HealthSummary,
}

/// Project health aggregate.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectHealth {
    /// Number of sessions analyzed.
    pub session_count: usize,
    /// Total cache misses.
    pub total_cache_misses: usize,
    /// Total token jumps.
    pub total_jumps: usize,
    /// Total waste tokens.
    pub total_waste_tokens: i64,
    /// Peak context.
    pub peak_context: i64,
    /// Total cache read tokens.
    pub total_cache_read_tokens: i64,
    /// Total cache creation tokens.
    pub total_cache_creation_tokens: i64,
    /// Peak cache read tokens in one message.
    pub peak_cache_read_tokens: i64,
    /// Peak cache creation tokens in one message.
    pub peak_cache_creation_tokens: i64,
    /// Per-session rows.
    pub sessions: Vec<ProjectHealthSession>,
}

/// Per-session project health row.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectHealthSession {
    /// Claude Code session id.
    pub session_id: String,
    /// Project alias.
    pub project_alias: String,
    /// Message count.
    pub message_count: usize,
    /// Cache miss count.
    pub cache_misses: usize,
    /// Jump count.
    pub jumps: usize,
    /// Cache window.
    pub cache_window: CacheWindow,
    /// Summary.
    pub summary: HealthSummary,
}

/// Cache window descriptor.
#[derive(Debug, Clone, Serialize)]
pub struct CacheWindow {
    /// Cache tier.
    pub tier: String,
    /// Idle window in seconds.
    pub idle_window_seconds: i64,
}

/// Cache miss row.
#[derive(Debug, Clone, Serialize)]
pub struct CacheMiss {
    /// Timestamp.
    pub timestamp: Option<String>,
    /// Gap in seconds.
    pub gap_seconds: i64,
    /// Context size.
    pub context_size: i64,
    /// Previous cache read.
    pub cache_read_before: i64,
    /// Current cache read.
    pub cache_read_after: i64,
    /// Message ordinal.
    pub ordinal: i64,
}

/// Token jump row.
#[derive(Debug, Clone, Serialize)]
pub struct TokenJump {
    /// Timestamp.
    pub timestamp: Option<String>,
    /// Token delta.
    pub delta: i64,
    /// Previous context.
    pub context_before: i64,
    /// Current context.
    pub context_after: i64,
    /// Whether this follows a compaction.
    pub is_post_compaction: bool,
    /// Model name.
    pub model: Option<String>,
    /// Message ordinal.
    pub ordinal: i64,
}

/// Health summary.
#[derive(Debug, Clone, Serialize)]
pub struct HealthSummary {
    /// Total input context.
    pub total_input: i64,
    /// Total output tokens.
    pub total_output: i64,
    /// Total cache read tokens.
    pub total_cache_read: i64,
    /// Total cache creation tokens.
    pub total_cache_creation: i64,
    /// Peak context.
    pub peak_context: i64,
    /// Peak cache read tokens in one message.
    pub peak_cache_read: i64,
    /// Peak cache creation tokens in one message.
    pub peak_cache_creation: i64,
    /// Startup context.
    pub startup_context: i64,
    /// Whether session appears continued.
    pub is_continued: bool,
    /// Token waste from jumps.
    pub total_waste: i64,
}

/// Sequence analysis output.
#[derive(Debug, Clone, Serialize)]
pub struct SequenceResult {
    /// Number of sessions analyzed.
    pub session_count: usize,
    /// Frequent ngrams.
    pub frequent_sequences: Vec<SequenceRow>,
    /// Retry patterns.
    pub retry_patterns: Vec<RetryRow>,
    /// Recovery stats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_stats: Option<Vec<RecoveryRow>>,
}

/// Frequent sequence row.
#[derive(Debug, Clone, Serialize)]
pub struct SequenceRow {
    /// Tool sequence.
    pub pattern: Vec<String>,
    /// Occurrence count.
    pub count: usize,
}

/// Retry row.
#[derive(Debug, Clone, Serialize)]
pub struct RetryRow {
    /// Retry pattern.
    pub pattern: String,
    /// Occurrence count.
    pub count: usize,
}

/// Recovery row.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryRow {
    /// Error category.
    pub category: String,
    /// Total errors.
    pub total_errors: usize,
    /// Retry percentage.
    pub retry_rate: f64,
    /// Recovery percentage.
    pub recovery_rate: f64,
    /// Average retry count.
    pub avg_retries: f64,
}

/// Search tool-call runs.
pub fn search_runs(conn: &Connection, filters: &RunFilters) -> Result<Vec<ToolCallRun>> {
    Ok(search_runs_in(db::list_runs(conn)?, filters))
}

/// Search tool-call runs against an already-loaded set.
pub fn search_runs_in(runs: Vec<ToolCallRun>, filters: &RunFilters) -> Vec<ToolCallRun> {
    let mut runs = filter_runs(runs, filters);
    if let Some(limit) = filters.limit {
        runs.truncate(limit);
    }
    runs
}

/// Options controlling per-file read-score computation.
#[derive(Debug, Clone)]
pub struct ReadScoreOptions {
    /// Recency half-life in days: a read this many days old counts half as much.
    pub half_life_days: f64,
    /// Restrict to paths under this prefix, applied after worktree normalization.
    pub under: Option<String>,
    /// Restrict to files with this extension (without the dot, e.g. `md`).
    pub ext: Option<String>,
    /// Reference time used for recency decay.
    pub now: DateTime<Utc>,
    /// Maximum files to emit, highest score first.
    pub limit: Option<usize>,
}

/// A single file's read score.
#[derive(Debug, Clone, Serialize)]
pub struct ReadScore {
    /// Canonical file path, with any worktree segment stripped.
    pub path: String,
    /// Recency-weighted read score.
    pub score: f64,
    /// Raw count of Read tool calls targeting the file.
    pub reads: usize,
    /// Most recent read timestamp, when known.
    pub last_read: Option<String>,
}

/// Aggregate read-score output.
#[derive(Debug, Clone, Serialize)]
pub struct ReadScoreResult {
    /// Reference time the scores were computed against (RFC3339).
    pub generated_at: String,
    /// Recency half-life in days used for decay.
    pub half_life_days: f64,
    /// Total Read tool calls counted after filtering.
    pub total_reads: usize,
    /// Number of distinct files scored.
    pub file_count: usize,
    /// Per-file scores, highest first.
    pub files: Vec<ReadScore>,
}

/// Fold a `/.claude/worktrees/<name>/` segment out of a path so reads taken in
/// a worktree count toward the canonical main-checkout file.
pub fn canonical_read_path(worktree_re: &Regex, path: &str) -> String {
    worktree_re.replace_all(path, "/").into_owned()
}

/// Score how often each file is opened via the `Read` tool, weighting recent
/// reads more heavily and folding worktree reads onto the canonical path.
pub fn read_scores_in(runs: Vec<ToolCallRun>, opts: &ReadScoreOptions) -> ReadScoreResult {
    let worktree_re = Regex::new(r"/\.claude/worktrees/[^/]+/").expect("valid worktree regex");
    let ext_suffix = opts
        .ext
        .as_ref()
        .map(|ext| format!(".{}", ext.trim_start_matches('.')));
    let half_life = opts.half_life_days.max(f64::MIN_POSITIVE);

    let mut acc: HashMap<String, (f64, usize, Option<String>)> = HashMap::new();
    let mut total_reads = 0usize;

    for run in runs.iter().filter(|run| run.tool_name == "Read") {
        let weight = match run.started_at.as_deref().and_then(parse_timestamp) {
            Some(timestamp) => {
                let age_days =
                    (opts.now - timestamp).num_seconds().max(0) as f64 / 86_400.0;
                0.5f64.powf(age_days / half_life)
            }
            // No timestamp: still counts as a raw read, but adds nothing to the
            // recency-weighted score rather than guessing an age.
            None => 0.0,
        };

        for raw in &run.file_paths {
            let path = canonical_read_path(&worktree_re, raw);
            if let Some(under) = &opts.under {
                if !path.starts_with(under) {
                    continue;
                }
            }
            if let Some(suffix) = &ext_suffix {
                if !path.ends_with(suffix.as_str()) {
                    continue;
                }
            }

            total_reads += 1;
            let entry = acc.entry(path).or_insert((0.0, 0, None));
            entry.0 += weight;
            entry.1 += 1;
            if let Some(ts) = run.started_at.as_deref() {
                let newer = match entry.2.as_deref() {
                    Some(existing) => ts > existing,
                    None => true,
                };
                if newer {
                    entry.2 = Some(ts.to_string());
                }
            }
        }
    }

    let mut files: Vec<ReadScore> = acc
        .into_iter()
        .map(|(path, (score, reads, last_read))| ReadScore {
            path,
            score: round2(score),
            reads,
            last_read,
        })
        .collect();
    files.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.reads.cmp(&left.reads))
            .then_with(|| left.path.cmp(&right.path))
    });
    let file_count = files.len();
    if let Some(limit) = opts.limit {
        files.truncate(limit);
    }

    ReadScoreResult {
        generated_at: opts.now.to_rfc3339(),
        half_life_days: opts.half_life_days,
        total_reads,
        file_count,
        files,
    }
}

/// Search transcript message content.
pub fn search_content(conn: &Connection, text: &str, limit: usize) -> Result<Vec<MessageHit>> {
    db::search_messages(conn, text, limit)
}

/// Search transcript message content against an in-memory store.
///
/// Mirrors the DB FTS path's contract (case-insensitive substring, snippets
/// capped at 180 chars) so callers can swap between the two and get
/// comparable hits.
pub fn search_content_in(messages: &[StoredMessage], text: &str, limit: usize) -> Vec<MessageHit> {
    let needle = text.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for message in messages {
        if message.search_text.to_lowercase().contains(&needle) {
            hits.push(MessageHit {
                session_id: message.session_id.clone(),
                external_session_id: message.external_session_id.clone(),
                project_alias: message.project_alias.clone(),
                ordinal: message.ordinal,
                record_type: message.record_type.clone(),
                role: message.role.clone(),
                timestamp: message.timestamp.clone(),
                snippet: truncate_chars(&message.search_text, 180),
            });
            if hits.len() >= limit {
                break;
            }
        }
    }
    hits
}

/// A normalized message kept in-memory by the scan path.
///
/// Mirrors what the SQLite messages table stores so the same analytics cores
/// can run against either backing store.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    /// Internal session id.
    pub session_id: String,
    /// Claude Code session id.
    pub external_session_id: String,
    /// Project alias.
    pub project_alias: String,
    /// Message ordinal.
    pub ordinal: i64,
    /// Normalized record type.
    pub record_type: String,
    /// Message role.
    pub role: Option<String>,
    /// Timestamp.
    pub timestamp: Option<String>,
    /// Flattened text used by both substring search and inspection snippets.
    pub search_text: String,
}

/// Inspect runs for a session.
pub fn inspect_runs(
    conn: &Connection,
    session_id: &str,
    tool_use_id: Option<&str>,
    status: Option<&str>,
    context: Option<usize>,
) -> Result<Vec<ToolCallRun>> {
    let session = db::find_session(conn, session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
    Ok(inspect_runs_in(
        &session,
        db::list_runs(conn)?,
        tool_use_id,
        status,
        context,
    ))
}

/// Inspect runs for a session given pre-loaded runs.
pub fn inspect_runs_in(
    session: &SessionRecord,
    all_runs: Vec<ToolCallRun>,
    tool_use_id: Option<&str>,
    status: Option<&str>,
    context: Option<usize>,
) -> Vec<ToolCallRun> {
    let mut runs = all_runs
        .into_iter()
        .filter(|run| {
            run.session_id == session.id
                || (!session.is_subagent && run.parent_session_id.as_ref() == Some(&session.id))
        })
        .collect::<Vec<_>>();
    runs.sort_by(|left, right| {
        left.start_ordinal
            .unwrap_or(i64::MAX)
            .cmp(&right.start_ordinal.unwrap_or(i64::MAX))
            .then_with(|| left.tool_use_id.cmp(&right.tool_use_id))
    });

    if let Some(tool_use_id) = tool_use_id {
        context_window(
            &runs,
            |run| run.tool_use_id == tool_use_id,
            context.unwrap_or(0),
        )
    } else if let Some(status) = status {
        if let Some(context) = context {
            context_window(&runs, |run| run.status == status, context)
        } else {
            runs.into_iter()
                .filter(|run| run.status == status)
                .collect()
        }
    } else {
        runs
    }
}

/// Load message context around inspected runs.
pub fn message_context(conn: &Connection, runs: &[ToolCallRun]) -> Result<Vec<MessageHit>> {
    let ranges = context_ranges(runs);
    let mut output = Vec::new();
    for (session_id, min, max) in ranges {
        output.extend(db::messages_between(conn, &session_id, min, max)?);
    }
    output.sort_by(|left, right| {
        left.session_id
            .cmp(&right.session_id)
            .then_with(|| left.ordinal.cmp(&right.ordinal))
    });
    Ok(output)
}

/// In-memory equivalent of [`message_context`]: pull messages that fall
/// within ordinal windows around the given runs.
pub fn message_context_in(messages: &[StoredMessage], runs: &[ToolCallRun]) -> Vec<MessageHit> {
    let ranges = context_ranges(runs);
    let mut output = Vec::new();
    for (session_id, min, max) in &ranges {
        for message in messages {
            if &message.session_id == session_id
                && message.ordinal >= *min
                && message.ordinal <= *max
            {
                output.push(MessageHit {
                    session_id: message.session_id.clone(),
                    external_session_id: message.external_session_id.clone(),
                    project_alias: message.project_alias.clone(),
                    ordinal: message.ordinal,
                    record_type: message.record_type.clone(),
                    role: message.role.clone(),
                    timestamp: message.timestamp.clone(),
                    snippet: truncate_chars(&message.search_text, 240),
                });
            }
        }
    }
    output.sort_by(|left, right| {
        left.session_id
            .cmp(&right.session_id)
            .then_with(|| left.ordinal.cmp(&right.ordinal))
    });
    output
}

fn context_ranges(runs: &[ToolCallRun]) -> Vec<(String, i64, i64)> {
    let mut by_session = BTreeMap::<String, Vec<&ToolCallRun>>::new();
    for run in runs {
        by_session
            .entry(run.session_id.clone())
            .or_default()
            .push(run);
    }
    by_session
        .into_iter()
        .map(|(session_id, session_runs)| {
            let min = session_runs
                .iter()
                .filter_map(|run| run.start_ordinal)
                .map(|ordinal| ordinal.saturating_sub(1))
                .min()
                .unwrap_or(0);
            let max = session_runs
                .iter()
                .flat_map(|run| [run.start_ordinal, run.end_ordinal])
                .flatten()
                .max()
                .unwrap_or(min);
            (session_id, min, max)
        })
        .collect()
}

/// Compare two session cohorts.
pub fn compare(
    conn: &Connection,
    left_sessions: &[String],
    right_sessions: &[String],
    filters: &RunFilters,
    group_by: &str,
) -> Result<CompareResult> {
    Ok(compare_in(
        db::list_runs(conn)?,
        left_sessions,
        right_sessions,
        filters,
        group_by,
    ))
}

/// Compare cohorts against an already-loaded set of runs.
pub fn compare_in(
    runs: Vec<ToolCallRun>,
    left_sessions: &[String],
    right_sessions: &[String],
    filters: &RunFilters,
    group_by: &str,
) -> CompareResult {
    let runs = filter_runs(runs, filters);
    let left = cohort_runs(&runs, left_sessions);
    let right = cohort_runs(&runs, right_sessions);
    CompareResult {
        left: compare_groups(&left, group_by),
        right: compare_groups(&right, group_by),
    }
}

/// Aggregate runs.
pub fn aggregate(
    conn: &Connection,
    filters: &RunFilters,
    group_by: &[String],
) -> Result<AggregateResult> {
    Ok(aggregate_in(db::list_runs(conn)?, filters, group_by))
}

/// Aggregate against an already-loaded set of runs.
pub fn aggregate_in(
    runs: Vec<ToolCallRun>,
    filters: &RunFilters,
    group_by: &[String],
) -> AggregateResult {
    let runs = filter_runs(runs, filters);
    let groups = aggregate_groups(&runs, group_by);
    let top_errors = top_errors(&runs, 10);
    let session_count = runs
        .iter()
        .map(|run| run.session_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    AggregateResult {
        session_count,
        total_runs: runs.len(),
        groups,
        top_errors,
    }
}

/// Analyze error patterns.
pub fn error_analysis(
    conn: &Connection,
    filters: &RunFilters,
    top: usize,
    classify: bool,
) -> Result<ErrorAnalysisResult> {
    Ok(error_analysis_in(db::list_runs(conn)?, filters, top, classify))
}

/// Analyze error patterns against an already-loaded set of runs.
pub fn error_analysis_in(
    all_runs: Vec<ToolCallRun>,
    filters: &RunFilters,
    top: usize,
    classify: bool,
) -> ErrorAnalysisResult {
    let total_tool_calls = filter_runs(all_runs.clone(), filters).len();
    let mut effective = RunFilters {
        status: Some("error".to_string()),
        ..filters.clone_without_limit()
    };
    effective.limit = None;
    let runs = filter_runs(all_runs.clone(), &effective);
    let total_errors = runs.len();
    let totals = if classify {
        filter_runs(all_runs, filters).into_iter().fold(
            HashMap::<String, usize>::new(),
            |mut acc, run| {
                *acc.entry(run.tool_name).or_default() += 1;
                acc
            },
        )
    } else {
        HashMap::new()
    };

    let mut grouped = BTreeMap::<(String, String), Vec<ToolCallRun>>::new();
    for run in runs {
        let fingerprint = normalize_error(run.error_content.as_deref().unwrap_or(""));
        grouped
            .entry((run.tool_name.clone(), fingerprint))
            .or_default()
            .push(run);
    }

    let mut patterns = grouped
        .into_iter()
        .map(|((tool_name, fingerprint), mut group)| {
            group.sort_by(|left, right| {
                left.started_at
                    .cmp(&right.started_at)
                    .then_with(|| left.tool_use_id.cmp(&right.tool_use_id))
            });
            let sample = group
                .last()
                .and_then(|run| run.error_content.clone())
                .unwrap_or_default();
            let category = classify.then(|| classify_error(&sample));
            let total = totals.get(&tool_name).copied();
            ErrorPattern {
                tool_name,
                fingerprint,
                count: group.len(),
                first_seen: group.first().and_then(|run| run.started_at.clone()),
                last_seen: group.last().and_then(|run| run.started_at.clone()),
                sample_error: truncate_chars(&sample, 300),
                sample_sessions: group
                    .iter()
                    .map(|run| run.session_id.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .take(3)
                    .collect(),
                category: category.clone(),
                preventability: category.as_deref().map(error_preventability),
                total_tool_calls: total,
                error_rate: total.map(|total| {
                    if total == 0 {
                        0.0
                    } else {
                        round2(group.len() as f64 / total as f64 * 100.0)
                    }
                }),
            }
        })
        .collect::<Vec<_>>();
    patterns.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.tool_name.cmp(&right.tool_name))
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    let pattern_count = patterns.len();
    patterns.truncate(top);
    ErrorAnalysisResult {
        total_tool_calls,
        total_errors,
        pattern_count,
        patterns,
    }
}

/// Analyze one session's token health.
pub fn health_session(conn: &Connection, session_id: &str) -> Result<HealthReport> {
    let session = db::find_session(conn, session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
    let messages = usage_messages(conn, &session.id)?;
    Ok(health_session_in(&messages))
}

/// Analyze one session's token health from pre-loaded usage messages.
pub fn health_session_in(messages: &[UsageMessage]) -> HealthReport {
    analyze_usage_messages(messages)
}

/// Analyze project token health.
pub fn health_project(
    conn: &Connection,
    project: Option<&str>,
    since: Option<&str>,
    limit: usize,
) -> Result<ProjectHealth> {
    let sessions = db::list_sessions(conn)?;
    let mut prepared = Vec::with_capacity(sessions.len());
    for session in sessions {
        let messages = usage_messages(conn, &session.id)?;
        prepared.push((session, messages));
    }
    Ok(health_project_in(prepared, project, since, limit))
}

/// Analyze project token health from pre-loaded sessions and their usage messages.
pub fn health_project_in(
    sessions: Vec<(SessionRecord, Vec<UsageMessage>)>,
    project: Option<&str>,
    since: Option<&str>,
    limit: usize,
) -> ProjectHealth {
    let sessions = sessions
        .into_iter()
        .filter(|(session, _)| project.map_or(true, |value| session.project_alias == value))
        .filter(|(session, _)| {
            since.map_or(true, |since| {
                session.started_at.as_deref().unwrap_or("") >= since
            })
        })
        .collect::<Vec<_>>();
    let session_count = sessions.len();

    let mut rows = Vec::new();
    for (session, messages) in sessions {
        let report = analyze_usage_messages(&messages);
        rows.push(ProjectHealthSession {
            session_id: session.external_session_id,
            project_alias: session.project_alias,
            message_count: report.message_count,
            cache_misses: report.cache_misses.len(),
            jumps: report.jumps.len(),
            cache_window: report.cache_window,
            summary: report.summary,
        });
    }

    let total_cache_misses = rows.iter().map(|row| row.cache_misses).sum();
    let total_jumps = rows.iter().map(|row| row.jumps).sum();
    let total_waste_tokens = rows.iter().map(|row| row.summary.total_waste).sum();
    let peak_context = rows
        .iter()
        .map(|row| row.summary.peak_context)
        .max()
        .unwrap_or(0);
    let total_cache_read_tokens = rows.iter().map(|row| row.summary.total_cache_read).sum();
    let total_cache_creation_tokens = rows
        .iter()
        .map(|row| row.summary.total_cache_creation)
        .sum();
    let peak_cache_read_tokens = rows
        .iter()
        .map(|row| row.summary.peak_cache_read)
        .max()
        .unwrap_or(0);
    let peak_cache_creation_tokens = rows
        .iter()
        .map(|row| row.summary.peak_cache_creation)
        .max()
        .unwrap_or(0);
    rows.truncate(limit);
    ProjectHealth {
        session_count,
        total_cache_misses,
        total_jumps,
        total_waste_tokens,
        peak_context,
        total_cache_read_tokens,
        total_cache_creation_tokens,
        peak_cache_read_tokens,
        peak_cache_creation_tokens,
        sessions: rows,
    }
}

/// Detect recurring sequences and retries.
pub fn sequence_analysis(
    conn: &Connection,
    filters: &RunFilters,
    min_length: usize,
    max_length: usize,
    min_occurrences: usize,
    recovery: bool,
) -> Result<SequenceResult> {
    Ok(sequence_analysis_in(
        db::list_runs(conn)?,
        filters,
        min_length,
        max_length,
        min_occurrences,
        recovery,
    ))
}

/// Detect recurring sequences and retries against pre-loaded runs.
pub fn sequence_analysis_in(
    all_runs: Vec<ToolCallRun>,
    filters: &RunFilters,
    min_length: usize,
    max_length: usize,
    min_occurrences: usize,
    recovery: bool,
) -> SequenceResult {
    let runs = filter_runs(all_runs, filters);
    let mut sessions = BTreeMap::<String, Vec<ToolCallRun>>::new();
    for run in runs {
        sessions
            .entry(run.session_id.clone())
            .or_default()
            .push(run);
    }
    for session_runs in sessions.values_mut() {
        session_runs.sort_by(|left, right| {
            left.start_ordinal
                .unwrap_or(i64::MAX)
                .cmp(&right.start_ordinal.unwrap_or(i64::MAX))
                .then_with(|| left.tool_use_id.cmp(&right.tool_use_id))
        });
    }

    let mut ngrams = HashMap::<Vec<String>, usize>::new();
    for session_runs in sessions.values() {
        let tools = session_runs
            .iter()
            .map(|run| run.tool_name.clone())
            .collect::<Vec<_>>();
        for n in min_length..=max_length {
            if tools.len() < n {
                continue;
            }
            for index in 0..=tools.len() - n {
                *ngrams.entry(tools[index..index + n].to_vec()).or_default() += 1;
            }
        }
    }

    let mut frequent_sequences = ngrams
        .into_iter()
        .filter(|(_, count)| *count >= min_occurrences)
        .map(|(pattern, count)| SequenceRow { pattern, count })
        .collect::<Vec<_>>();
    frequent_sequences.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.pattern.cmp(&right.pattern))
    });
    frequent_sequences.truncate(20);

    let mut retries = HashMap::<String, usize>::new();
    for session_runs in sessions.values() {
        for pair in session_runs.windows(2) {
            let [left, right] = pair else {
                continue;
            };
            if left.status == "error" && left.tool_name == right.tool_name {
                let hint = truncate_chars(
                    &normalize_error(left.error_content.as_deref().unwrap_or("")),
                    60,
                );
                *retries
                    .entry(format!(
                        "{} ({hint}) -> {}",
                        left.tool_name, right.tool_name
                    ))
                    .or_default() += 1;
            }
        }
    }
    let mut retry_patterns = retries
        .into_iter()
        .filter(|(_, count)| *count >= min_occurrences)
        .map(|(pattern, count)| RetryRow { pattern, count })
        .collect::<Vec<_>>();
    retry_patterns.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.pattern.cmp(&right.pattern))
    });
    retry_patterns.truncate(20);

    let recovery_stats = recovery.then(|| recovery_analysis(&sessions));
    SequenceResult {
        session_count: sessions.len(),
        frequent_sequences,
        retry_patterns,
        recovery_stats,
    }
}

impl RunFilters {
    fn clone_without_limit(&self) -> Self {
        Self {
            project: self.project.clone(),
            worktree: self.worktree.clone(),
            session: self.session.clone(),
            tool: self.tool.clone(),
            command_contains: self.command_contains.clone(),
            error_contains: self.error_contains.clone(),
            file_path: self.file_path.clone(),
            min_duration: self.min_duration,
            max_duration: self.max_duration,
            min_read_lines: self.min_read_lines,
            status: self.status.clone(),
            since: self.since.clone(),
            limit: None,
        }
    }
}

/// Apply the standard run-filter set in-place.
///
/// Exposed so the DB-less `scan` path can reuse the same predicate the
/// DB-backed `search` path uses.
pub fn filter_runs(runs: Vec<ToolCallRun>, filters: &RunFilters) -> Vec<ToolCallRun> {
    runs.into_iter()
        .filter(|run| {
            filters
                .project
                .as_ref()
                .map_or(true, |value| &run.project_alias == value)
        })
        .filter(|run| {
            filters
                .worktree
                .as_ref()
                .map_or(true, |value| run.worktree_name.as_ref() == Some(value))
        })
        .filter(|run| {
            filters.session.as_ref().map_or(true, |value| {
                &run.session_id == value
                    || &run.external_session_id == value
                    || run.parent_session_id.as_ref() == Some(value)
            })
        })
        .filter(|run| {
            filters
                .tool
                .as_ref()
                .map_or(true, |value| &run.tool_name == value)
        })
        .filter(|run| contains_opt(run.command.as_deref(), filters.command_contains.as_deref()))
        .filter(|run| {
            contains_opt(
                run.error_content.as_deref(),
                filters.error_contains.as_deref(),
            )
        })
        .filter(|run| {
            filters.file_path.as_ref().map_or(true, |path| {
                run.file_paths.iter().any(|stored| stored.contains(path))
                    || run
                        .command
                        .as_ref()
                        .map_or(false, |command| command.contains(path))
                    || run
                        .input_summary
                        .as_ref()
                        .map_or(false, |summary| summary.contains(path))
            })
        })
        .filter(|run| {
            filters
                .min_duration
                .map_or(true, |min| run.duration_ms.unwrap_or(0) >= min)
        })
        .filter(|run| {
            filters
                .max_duration
                .map_or(true, |max| run.duration_ms.unwrap_or(0) <= max)
        })
        .filter(|run| {
            filters.min_read_lines.map_or(true, |min| {
                run.read_total_lines.map_or(false, |lines| lines >= min)
            })
        })
        .filter(|run| {
            filters
                .status
                .as_ref()
                .map_or(true, |value| &run.status == value)
        })
        .filter(|run| {
            filters.since.as_ref().map_or(true, |since| {
                run.started_at.as_deref().unwrap_or("") >= since.as_str()
            })
        })
        .collect()
}

fn contains_opt(haystack: Option<&str>, needle: Option<&str>) -> bool {
    needle.map_or(true, |needle| {
        haystack.map_or(false, |haystack| haystack.contains(needle))
    })
}

fn context_window<F>(runs: &[ToolCallRun], predicate: F, context: usize) -> Vec<ToolCallRun>
where
    F: Fn(&ToolCallRun) -> bool,
{
    let mut indices = runs
        .iter()
        .enumerate()
        .filter_map(|(index, run)| predicate(run).then_some(index))
        .flat_map(|index| {
            index.saturating_sub(context)..=usize::min(runs.len() - 1, index + context)
        })
        .collect::<Vec<_>>();
    indices.sort_unstable();
    indices.dedup();
    indices
        .into_iter()
        .map(|index| runs[index].clone())
        .collect()
}

fn cohort_runs(runs: &[ToolCallRun], sessions: &[String]) -> Vec<ToolCallRun> {
    runs.iter()
        .filter(|run| {
            sessions.iter().any(|session| {
                &run.session_id == session
                    || &run.external_session_id == session
                    || run.parent_session_id.as_ref() == Some(session)
            })
        })
        .cloned()
        .collect()
}

fn compare_groups(runs: &[ToolCallRun], group_by: &str) -> Vec<CompareGroup> {
    let mut grouped = BTreeMap::<String, Vec<&ToolCallRun>>::new();
    for run in runs {
        grouped
            .entry(group_value(run, group_by))
            .or_default()
            .push(run);
    }
    grouped
        .into_iter()
        .map(|(key, group)| CompareGroup {
            key,
            count: group.len(),
            avg_duration_ms: average_duration(group.iter().copied()),
        })
        .collect()
}

fn aggregate_groups(runs: &[ToolCallRun], group_by: &[String]) -> Vec<AggregateGroup> {
    let keys = if group_by.is_empty() {
        vec!["tool_name".to_string()]
    } else {
        group_by.to_vec()
    };
    let mut grouped = BTreeMap::<Vec<String>, Vec<&ToolCallRun>>::new();
    for run in runs {
        grouped
            .entry(keys.iter().map(|key| group_value(run, key)).collect())
            .or_default()
            .push(run);
    }

    let mut rows = grouped
        .into_iter()
        .map(|(values, group)| {
            let count = group.len();
            let errors = group.iter().filter(|run| run.status == "error").count();
            let mut durations = group
                .iter()
                .filter_map(|run| run.duration_ms)
                .collect::<Vec<_>>();
            durations.sort_unstable();
            AggregateGroup {
                key: keys.iter().cloned().zip(values).collect(),
                count,
                errors,
                error_pct: if count == 0 {
                    0.0
                } else {
                    round1(errors as f64 / count as f64 * 100.0)
                },
                avg_duration_ms: percentile(&durations, 50),
                p95_duration_ms: percentile(&durations, 95),
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.key.cmp(&right.key))
    });
    rows
}

fn group_value(run: &ToolCallRun, key: &str) -> String {
    match key {
        "tool_name" => run.tool_name.clone(),
        "status" => run.status.clone(),
        "project" | "project_alias" => run.project_alias.clone(),
        "worktree" | "worktree_name" => run.worktree_name.clone().unwrap_or_default(),
        "agent_id" => run.agent_id.clone().unwrap_or_default(),
        other => format!("unsupported:{other}"),
    }
}

fn average_duration<'a>(runs: impl Iterator<Item = &'a ToolCallRun>) -> Option<i64> {
    let durations = runs.filter_map(|run| run.duration_ms).collect::<Vec<_>>();
    (!durations.is_empty()).then(|| durations.iter().sum::<i64>() / durations.len() as i64)
}

fn percentile(sorted: &[i64], pct: usize) -> Option<i64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() * pct).div_ceil(100)).saturating_sub(1);
    sorted.get(index).copied()
}

fn top_errors(runs: &[ToolCallRun], top: usize) -> Vec<TopError> {
    let mut grouped = BTreeMap::<String, Vec<&ToolCallRun>>::new();
    for run in runs.iter().filter(|run| run.status == "error") {
        grouped
            .entry(normalize_error(run.error_content.as_deref().unwrap_or("")))
            .or_default()
            .push(run);
    }
    let mut rows = grouped
        .into_iter()
        .map(|(fingerprint, group)| {
            let first = group[0];
            TopError {
                fingerprint,
                count: group.len(),
                tool_name: first.tool_name.clone(),
                sample: truncate_chars(first.error_content.as_deref().unwrap_or(""), 200),
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.tool_name.cmp(&right.tool_name))
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    rows.truncate(top);
    rows
}

fn normalize_error(content: &str) -> String {
    let path_normalized = Regex::new(r"/[^\s:]+/[^\s:]+").map_or_else(
        |_| content.to_string(),
        |regex| regex.replace_all(content, "<path>").to_string(),
    );
    let number_normalized = Regex::new(r"\d+")
        .map(|regex| regex.replace_all(&path_normalized, "N").to_string())
        .unwrap_or(path_normalized);
    truncate_chars(&number_normalized, 120)
}

fn classify_error(content: &str) -> String {
    let lower = content.to_ascii_lowercase();
    if lower.contains("doesn't want to proceed") || lower.contains("was rejected") {
        "user_rejected"
    } else if lower.contains("sibling tool call errored") {
        "sibling_errored"
    } else if lower.contains("hook") && (lower.contains("blocked") || lower.contains("denied")) {
        "hook_blocked"
    } else if lower.contains("has not been read yet") {
        "file_not_read_first"
    } else if lower.contains("modified since read") {
        "file_modified_since_read"
    } else if lower.contains("does not exist")
        || lower.contains("no such file")
        || lower.contains("enoent")
    {
        "file_not_found"
    } else if lower.contains("eisdir") || lower.contains("path does not exist") {
        "path_error"
    } else if lower.contains("exceeds maximum allowed tokens") {
        "token_limit_exceeded"
    } else if lower.contains("mcp error") {
        "mcp_error"
    } else if lower.contains("precommit failed")
        || lower.contains("pre-commit")
        || lower.contains("lefthook")
    {
        "pre_commit_failed"
    } else if lower.contains("exit code") {
        "exit_code"
    } else {
        "other"
    }
    .to_string()
}

fn error_preventability(category: &str) -> String {
    match category {
        "file_not_read_first"
        | "file_modified_since_read"
        | "file_not_found"
        | "path_error"
        | "token_limit_exceeded" => "preventable",
        "user_rejected" | "hook_blocked" => "user_driven",
        "sibling_errored" => "cascading",
        "exit_code" | "mcp_error" | "pre_commit_failed" => "systemic",
        _ => "other",
    }
    .to_string()
}

/// Per-message token usage row, shared by the DB-backed and in-memory health paths.
#[derive(Debug, Clone)]
pub struct UsageMessage {
    /// Message ordinal.
    pub ordinal: i64,
    /// Timestamp.
    pub timestamp: Option<String>,
    /// Input tokens.
    pub input_tokens: i64,
    /// Output tokens.
    pub output_tokens: i64,
    /// Cache read input tokens.
    pub cache_read_input_tokens: i64,
    /// Cache creation input tokens.
    pub cache_creation_input_tokens: i64,
    /// Model name when present.
    pub model: Option<String>,
    /// Raw JSON payload as a string — used to sniff cache tier strings.
    pub raw_payload: String,
}

impl UsageMessage {
    /// Build a usage row from an in-memory transcript message, if it carries
    /// token usage. Returns `None` for messages without `input_tokens`.
    pub fn from_transcript_message(message: &TranscriptMessage) -> Option<Self> {
        let input_tokens = message.input_tokens?;
        Some(Self {
            ordinal: message.ordinal,
            timestamp: message.timestamp.map(|timestamp| timestamp.to_rfc3339()),
            input_tokens,
            output_tokens: message.output_tokens.unwrap_or(0),
            cache_read_input_tokens: message.cache_read_input_tokens.unwrap_or(0),
            cache_creation_input_tokens: message.cache_creation_input_tokens.unwrap_or(0),
            model: message.model.clone(),
            raw_payload: message.raw_payload.to_string(),
        })
    }
}

fn usage_messages(conn: &Connection, session_id: &str) -> Result<Vec<UsageMessage>> {
    let mut stmt = conn.prepare(
        "SELECT ordinal, timestamp, COALESCE(input_tokens, 0), COALESCE(output_tokens, 0), COALESCE(cache_read_input_tokens, 0), COALESCE(cache_creation_input_tokens, 0), model, raw_payload \
         FROM messages WHERE session_id = ?1 AND input_tokens IS NOT NULL ORDER BY ordinal",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok(UsageMessage {
            ordinal: row.get(0)?,
            timestamp: row.get(1)?,
            input_tokens: row.get(2)?,
            output_tokens: row.get(3)?,
            cache_read_input_tokens: row.get(4)?,
            cache_creation_input_tokens: row.get(5)?,
            model: row.get(6)?,
            raw_payload: row.get(7)?,
        })
    })?;
    let mut output = Vec::new();
    for row in rows {
        output.push(row?);
    }
    Ok(output)
}

fn analyze_usage_messages(messages: &[UsageMessage]) -> HealthReport {
    if messages.is_empty() {
        return HealthReport {
            message_count: 0,
            cache_misses: Vec::new(),
            jumps: Vec::new(),
            cache_window: default_cache_window(),
            summary: HealthSummary {
                total_input: 0,
                total_output: 0,
                total_cache_read: 0,
                total_cache_creation: 0,
                peak_context: 0,
                peak_cache_read: 0,
                peak_cache_creation: 0,
                startup_context: 0,
                is_continued: false,
                total_waste: 0,
            },
        };
    }

    let cache_window = detect_cache_window(messages);
    let jumps = detect_jumps(messages);
    let cache_misses = detect_cache_misses(messages);
    let peak_context = messages.iter().map(context_size).max().unwrap_or(0);
    let total_output = messages.iter().map(|message| message.output_tokens).sum();
    let total_cache_read = messages
        .iter()
        .map(|message| message.cache_read_input_tokens)
        .sum();
    let total_cache_creation = messages
        .iter()
        .map(|message| message.cache_creation_input_tokens)
        .sum();
    let peak_cache_read = messages
        .iter()
        .map(|message| message.cache_read_input_tokens)
        .max()
        .unwrap_or(0);
    let peak_cache_creation = messages
        .iter()
        .map(|message| message.cache_creation_input_tokens)
        .max()
        .unwrap_or(0);
    let startup_context = messages.first().map_or(0, context_size);
    let first_cache_ratio = messages.first().map_or(0.0, |message| {
        let context = context_size(message);
        if context == 0 {
            0.0
        } else {
            message.cache_read_input_tokens as f64 / context as f64
        }
    });
    let total_waste = jumps
        .iter()
        .filter(|jump| !jump.is_post_compaction)
        .map(|jump| jump.delta)
        .sum();

    HealthReport {
        message_count: messages.len(),
        cache_misses,
        jumps,
        cache_window,
        summary: HealthSummary {
            total_input: messages.last().map_or(0, context_size),
            total_output,
            total_cache_read,
            total_cache_creation,
            peak_context,
            peak_cache_read,
            peak_cache_creation,
            startup_context,
            is_continued: first_cache_ratio >= 0.80,
            total_waste,
        },
    }
}

fn detect_cache_window(messages: &[UsageMessage]) -> CacheWindow {
    for message in messages.iter().rev() {
        if message.raw_payload.contains("ephemeral_5m_input_tokens") {
            return CacheWindow {
                tier: "5m cache".to_string(),
                idle_window_seconds: 300,
            };
        }
        if message.raw_payload.contains("ephemeral_1h_input_tokens") {
            return CacheWindow {
                tier: "1h cache".to_string(),
                idle_window_seconds: 3600,
            };
        }
    }
    default_cache_window()
}

fn default_cache_window() -> CacheWindow {
    CacheWindow {
        tier: "legacy 5m fallback".to_string(),
        idle_window_seconds: 300,
    }
}

fn detect_cache_misses(messages: &[UsageMessage]) -> Vec<CacheMiss> {
    messages
        .windows(2)
        .filter_map(|pair| {
            let [prev, curr] = pair else {
                return None;
            };
            let prev_cache = prev.cache_read_input_tokens;
            let curr_cache = curr.cache_read_input_tokens;
            let curr_context = context_size(curr);
            let prev_context = context_size(prev);
            let gap = timestamp_gap_seconds(prev.timestamp.as_deref(), curr.timestamp.as_deref());
            let drop = if prev_cache > 0 {
                (prev_cache - curr_cache) as f64 / prev_cache as f64
            } else {
                0.0
            };
            (curr_context >= 200_000
                && prev_cache > 0
                && prev_context != 0
                && drop > 0.80
                && gap >= 300)
                .then(|| CacheMiss {
                    timestamp: curr.timestamp.clone(),
                    gap_seconds: gap,
                    context_size: curr_context,
                    cache_read_before: prev_cache,
                    cache_read_after: curr_cache,
                    ordinal: curr.ordinal,
                })
        })
        .collect()
}

fn detect_jumps(messages: &[UsageMessage]) -> Vec<TokenJump> {
    let mut previous = 0;
    let mut jumps = Vec::new();
    for message in messages {
        let context = context_size(message);
        let delta = context - previous;
        if delta >= 5_000 {
            jumps.push(TokenJump {
                timestamp: message.timestamp.clone(),
                delta,
                context_before: previous,
                context_after: context,
                is_post_compaction: previous == 0,
                model: message.model.clone(),
                ordinal: message.ordinal,
            });
        }
        previous = context;
    }
    jumps
}

const fn context_size(message: &UsageMessage) -> i64 {
    message.input_tokens + message.cache_creation_input_tokens + message.cache_read_input_tokens
}

fn timestamp_gap_seconds(previous: Option<&str>, current: Option<&str>) -> i64 {
    let Some(previous) = previous.and_then(parse_timestamp) else {
        return 0;
    };
    let Some(current) = current.and_then(parse_timestamp) else {
        return 0;
    };
    current.signed_duration_since(previous).num_seconds().max(0)
}

fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .ok()
}

fn recovery_analysis(sessions: &BTreeMap<String, Vec<ToolCallRun>>) -> Vec<RecoveryRow> {
    #[derive(Default)]
    struct Stats {
        total: usize,
        retried: usize,
        recovered: usize,
        retry_counts: Vec<usize>,
    }

    let mut stats = BTreeMap::<String, Stats>::new();
    for runs in sessions.values() {
        for (index, run) in runs
            .iter()
            .enumerate()
            .filter(|(_, run)| run.status == "error")
        {
            let category = classify_error(run.error_content.as_deref().unwrap_or(""));
            let following = &runs[index + 1..usize::min(runs.len(), index + 6)];
            let retries = following
                .iter()
                .take_while(|next| next.tool_name == run.tool_name)
                .collect::<Vec<_>>();
            let entry = stats.entry(category).or_default();
            entry.total += 1;
            if !retries.is_empty() {
                entry.retried += 1;
                entry.retry_counts.push(retries.len());
            }
            if retries.iter().any(|retry| retry.status == "completed") {
                entry.recovered += 1;
            }
        }
    }

    stats
        .into_iter()
        .map(|(category, stats)| RecoveryRow {
            category,
            total_errors: stats.total,
            retry_rate: if stats.total == 0 {
                0.0
            } else {
                round1(stats.retried as f64 / stats.total as f64 * 100.0)
            },
            recovery_rate: if stats.retried == 0 {
                0.0
            } else {
                round1(stats.recovered as f64 / stats.retried as f64 * 100.0)
            },
            avg_retries: if stats.retry_counts.is_empty() {
                0.0
            } else {
                round1(
                    stats.retry_counts.iter().sum::<usize>() as f64
                        / stats.retry_counts.len() as f64,
                )
            },
        })
        .collect()
}

fn truncate_chars(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let truncated = chars.by_ref().take(max).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: &str, tool: &str, status: &str, ordinal: i64) -> ToolCallRun {
        ToolCallRun {
            tool_use_id: id.to_string(),
            session_id: "session-a".to_string(),
            external_session_id: "external-a".to_string(),
            parent_session_id: Some("parent-a".to_string()),
            is_subagent: false,
            agent_id: Some("agent-a".to_string()),
            tool_name: tool.to_string(),
            command: Some("cat src/main.rs".to_string()),
            command_program: Some("cat".to_string()),
            command_args: vec!["src/main.rs".to_string()],
            command_fingerprint: Some("cat <path>".to_string()),
            input_summary: Some("file_path=src/lib.rs".to_string()),
            input_size: Some(10),
            output_size: Some(20),
            file_paths: vec!["src/main.rs".to_string()],
            status: status.to_string(),
            started_at: Some(format!("2026-01-01T00:00:{ordinal:02}Z")),
            finished_at: Some(format!("2026-01-01T00:00:{:02}Z", ordinal + 1)),
            duration_ms: Some(100 + ordinal),
            start_ordinal: Some(ordinal),
            end_ordinal: Some(ordinal + 1),
            source_scope: Some("main".to_string()),
            error_content: (status == "error")
                .then(|| "exit code 1 reading /tmp/project/src/main.rs at line 42".to_string()),
            project_alias: "project-a".to_string(),
            worktree_name: Some("worktree-a".to_string()),
            canonical_cwd: Some("/tmp/project".to_string()),
            read_total_lines: None,
            read_lines: None,
            read_truncated: None,
        }
    }

    #[test]
    fn run_filtering_grouping_and_context_branches() {
        let mut first = run("toolu_a", "Bash", "error", 1);
        let mut second = run("toolu_b", "Read", "completed", 3);
        second.project_alias = "project-b".to_string();
        second.worktree_name = None;
        second.parent_session_id = None;
        second.command = None;
        second.input_summary = None;
        second.file_paths.clear();
        second.read_total_lines = Some(1500);
        second.read_lines = Some(200);
        second.read_truncated = Some(true);

        let runs = vec![first.clone(), second.clone()];
        let filters = RunFilters {
            project: Some("project-a".to_string()),
            worktree: Some("worktree-a".to_string()),
            session: Some("parent-a".to_string()),
            tool: Some("Bash".to_string()),
            command_contains: Some("src/main".to_string()),
            error_contains: Some("exit code".to_string()),
            file_path: Some("src/lib".to_string()),
            min_duration: Some(90),
            max_duration: Some(200),
            min_read_lines: None,
            status: Some("error".to_string()),
            since: Some("2026-01-01".to_string()),
            limit: None,
        };
        assert_eq!(filter_runs(runs.clone(), &filters).len(), 1);

        // `min_read_lines` matches on the file's total size, so the partially
        // read 1500-line file is kept even though only 200 lines were returned,
        // and the non-Read run (no line metadata) is dropped.
        let big_reads = filter_runs(
            runs.clone(),
            &RunFilters {
                min_read_lines: Some(1000),
                ..RunFilters::default()
            },
        );
        assert_eq!(big_reads.len(), 1);
        assert_eq!(big_reads[0].tool_use_id, "toolu_b");
        assert!(filter_runs(
            runs.clone(),
            &RunFilters {
                min_read_lines: Some(2000),
                ..RunFilters::default()
            },
        )
        .is_empty());
        assert!(filter_runs(
            runs.clone(),
            &RunFilters {
                project: Some("missing".to_string()),
                ..RunFilters::default()
            },
        )
        .is_empty());
        assert!(!contains_opt(None, Some("needle")));
        assert!(contains_opt(None, None));

        assert_eq!(group_value(&first, "tool_name"), "Bash");
        assert_eq!(group_value(&first, "status"), "error");
        assert_eq!(group_value(&first, "project"), "project-a");
        assert_eq!(group_value(&first, "worktree"), "worktree-a");
        assert_eq!(group_value(&first, "agent_id"), "agent-a");
        assert_eq!(group_value(&first, "unknown"), "unsupported:unknown");

        let compare = compare_groups(&runs, "tool_name");
        assert_eq!(compare.len(), 2);
        let aggregate = aggregate_groups(&runs, &["status".to_string(), "tool_name".to_string()]);
        assert_eq!(aggregate.len(), 2);
        assert_eq!(average_duration([&first, &second].into_iter()), Some(102));
        assert_eq!(average_duration(std::iter::empty()), None);
        assert_eq!(percentile(&[], 95), None);
        assert_eq!(percentile(&[10, 20, 30], 95), Some(30));
        assert_eq!(cohort_runs(&runs, &["external-a".to_string()]).len(), 2);
        assert_eq!(
            context_window(&runs, |run| run.tool_use_id == "toolu_b", 1).len(),
            2
        );

        let top = top_errors(&[first.clone()], 10);
        assert_eq!(top[0].tool_name, "Bash");
        first.error_content = None;
        assert_eq!(top_errors(&[first], 10)[0].sample, "");
    }

    #[test]
    fn error_classification_and_recovery_branches() {
        let cases = [
            ("doesn't want to proceed", "user_rejected", "user_driven"),
            ("sibling tool call errored", "sibling_errored", "cascading"),
            ("hook blocked command", "hook_blocked", "user_driven"),
            (
                "has not been read yet",
                "file_not_read_first",
                "preventable",
            ),
            (
                "modified since read",
                "file_modified_since_read",
                "preventable",
            ),
            ("ENOENT no such file", "file_not_found", "preventable"),
            ("EISDIR directory target", "path_error", "preventable"),
            (
                "exceeds maximum allowed tokens",
                "token_limit_exceeded",
                "preventable",
            ),
            ("mcp error", "mcp_error", "systemic"),
            ("pre-commit hook failed", "pre_commit_failed", "systemic"),
            ("exit code 2", "exit_code", "systemic"),
            ("unclassified", "other", "other"),
        ];
        for (message, category, preventability) in cases {
            assert_eq!(classify_error(message), category);
            assert_eq!(error_preventability(category), preventability);
        }

        let mut first = run("toolu_error", "Bash", "error", 1);
        first.error_content = Some("has not been read yet".to_string());
        let retry = run("toolu_retry", "Bash", "completed", 2);
        let mut second = run("toolu_other", "Read", "error", 4);
        second.error_content = Some("mcp error".to_string());
        let sessions = BTreeMap::from([("session-a".to_string(), vec![first, retry, second])]);
        let recovery = recovery_analysis(&sessions);
        assert!(recovery.iter().any(|row| {
            row.category == "file_not_read_first"
                && approx_eq(row.retry_rate, 100.0)
                && approx_eq(row.recovery_rate, 100.0)
        }));
        assert!(recovery
            .iter()
            .any(|row| row.category == "mcp_error" && approx_eq(row.retry_rate, 0.0)));
    }

    #[test]
    fn usage_health_branch_detection() {
        let empty = analyze_usage_messages(&[]);
        assert_eq!(empty.summary.peak_context, 0);
        assert_eq!(detect_cache_window(&[]).tier, "legacy 5m fallback");

        let messages = vec![
            UsageMessage {
                ordinal: 1,
                timestamp: Some("2026-01-01T00:00:00Z".to_string()),
                input_tokens: 6_000,
                output_tokens: 1,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                model: Some("model-a".to_string()),
                raw_payload: "{}".to_string(),
            },
            UsageMessage {
                ordinal: 2,
                timestamp: Some("2026-01-01T00:00:01Z".to_string()),
                input_tokens: 10_000,
                output_tokens: 2,
                cache_read_input_tokens: 220_000,
                cache_creation_input_tokens: 0,
                model: Some("model-a".to_string()),
                raw_payload: r#"{"ephemeral_1h_input_tokens":1}"#.to_string(),
            },
            UsageMessage {
                ordinal: 3,
                timestamp: Some("2026-01-01T00:07:00Z".to_string()),
                input_tokens: 250_000,
                output_tokens: 3,
                cache_read_input_tokens: 1_000,
                cache_creation_input_tokens: 0,
                model: Some("model-b".to_string()),
                raw_payload: r#"{"ephemeral_5m_input_tokens":1}"#.to_string(),
            },
        ];
        let report = analyze_usage_messages(&messages);
        assert_eq!(report.cache_window.tier, "5m cache");
        assert_eq!(report.cache_misses.len(), 1);
        assert_eq!(report.summary.total_cache_read, 221_000);
        assert_eq!(report.summary.peak_cache_read, 220_000);
        assert!(report.jumps.iter().any(|jump| jump.is_post_compaction));
        assert!(report.jumps.iter().any(|jump| !jump.is_post_compaction));
        assert_eq!(
            timestamp_gap_seconds(Some("not-a-date"), Some("2026-01-01T00:00:00Z")),
            0
        );
        assert_eq!(
            timestamp_gap_seconds(Some("2026-01-01T00:00:10Z"), Some("2026-01-01T00:00:00Z")),
            0
        );
    }

    #[test]
    fn string_helpers_cover_truncation_and_normalization() {
        assert_eq!(truncate_chars("abc", 10), "abc");
        assert_eq!(truncate_chars("abcdef", 3), "abc...");
        assert!(normalize_error("/tmp/project/file 123").contains("<path>"));
        assert!(approx_eq(round1(1.24), 1.2));
        assert!(approx_eq(round2(1.235), 1.24));
    }

    #[test]
    fn read_scores_fold_worktrees_and_weight_recency() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2026, 6, 23, 0, 0, 0).unwrap();

        let mut recent = run("toolu_recent", "Read", "completed", 1);
        recent.file_paths = vec!["/srv/town/levio/.claude/worktrees/wt-a/README.md".to_string()];
        recent.started_at = Some("2026-06-22T00:00:00Z".to_string()); // ~1 day old

        let mut also_recent = run("toolu_recent2", "Read", "completed", 2);
        also_recent.file_paths = vec!["/srv/town/levio/README.md".to_string()];
        also_recent.started_at = Some("2026-06-21T00:00:00Z".to_string()); // ~2 days old

        let mut old = run("toolu_old", "Read", "completed", 3);
        old.file_paths = vec!["/srv/town/levio/OLD.md".to_string()];
        old.started_at = Some("2026-01-01T00:00:00Z".to_string()); // months old

        let mut non_md = run("toolu_code", "Read", "completed", 4);
        non_md.file_paths = vec!["/srv/town/levio/src/main.rs".to_string()];

        let mut a_write = run("toolu_write", "Edit", "completed", 5);
        a_write.file_paths = vec!["/srv/town/levio/README.md".to_string()];

        let opts = ReadScoreOptions {
            half_life_days: 30.0,
            under: Some("/srv/town/".to_string()),
            ext: Some("md".to_string()),
            now,
            limit: None,
        };
        let result = read_scores_in(vec![recent, also_recent, old, non_md, a_write], &opts);

        // Only .md files under the prefix: src/main.rs filtered out, the Edit ignored.
        assert_eq!(result.file_count, 2);
        assert_eq!(result.total_reads, 3);
        // README read from a worktree and from the main checkout fold into one file.
        let readme = result
            .files
            .iter()
            .find(|file| file.path == "/srv/town/levio/README.md")
            .expect("readme scored");
        assert_eq!(readme.reads, 2);
        // The recently-read README outranks the months-old doc.
        assert_eq!(result.files[0].path, "/srv/town/levio/README.md");
        assert!(result.files[0].score > result.files[1].score);
    }

    fn approx_eq(left: f64, right: f64) -> bool {
        (left - right).abs() < f64::EPSILON
    }
}
