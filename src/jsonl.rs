//! Claude Code JSONL transcript parsing.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use thiserror::Error;

/// A parsed transcript session.
#[derive(Debug, Clone)]
pub struct ParsedSession {
    /// Claude Code session id.
    pub session_id: Option<String>,
    /// Session slug when present.
    pub slug: Option<String>,
    /// Working directory recorded by Claude Code.
    pub cwd: Option<String>,
    /// Git branch recorded by Claude Code.
    pub git_branch: Option<String>,
    /// Claude Code version recorded by the transcript.
    pub version: Option<String>,
    /// First message timestamp.
    pub started_at: Option<DateTime<Utc>>,
    /// Last message timestamp.
    pub ended_at: Option<DateTime<Utc>>,
    /// Agent id for subagent transcripts.
    pub agent_id: Option<String>,
    /// Number of non-empty JSONL records parsed or scanned.
    pub message_count: usize,
    /// Normalized transcript messages.
    pub messages: Vec<TranscriptMessage>,
}

/// A normalized transcript message.
#[derive(Debug, Clone)]
pub struct TranscriptMessage {
    /// Zero-based JSONL ordinal.
    pub ordinal: i64,
    /// Transcript source scope, such as `main` or `subagent:<id>`.
    pub source_scope: String,
    /// Raw Claude Code type.
    pub record_type: Option<String>,
    /// Normalized type label.
    pub normalized_type: String,
    /// Message uuid.
    pub uuid: Option<String>,
    /// Parent message uuid.
    pub parent_uuid: Option<String>,
    /// Nested Anthropic message id.
    pub message_id: Option<String>,
    /// Nested message role.
    pub role: Option<String>,
    /// Normalized content object.
    pub content: Value,
    /// Raw JSONL payload.
    pub raw_payload: Value,
    /// Parsed timestamp.
    pub timestamp: Option<DateTime<Utc>>,
    /// Whether this row is from a sidechain/subagent.
    pub is_sidechain: bool,
    /// Agent id when present.
    pub agent_id: Option<String>,
    /// Tool use id when present at the top level.
    pub tool_use_id: Option<String>,
    /// Parent tool use id when present.
    pub parent_tool_use_id: Option<String>,
    /// Session id on the row.
    pub session_id: Option<String>,
    /// Slug on the row.
    pub slug: Option<String>,
    /// Working directory on the row.
    pub cwd: Option<String>,
    /// Git branch on the row.
    pub git_branch: Option<String>,
    /// Claude Code version on the row.
    pub version: Option<String>,
    /// Team name on the row.
    pub team_name: Option<String>,
    /// Agent display name on the row.
    pub agent_name: Option<String>,
    /// Input token count.
    pub input_tokens: Option<i64>,
    /// Output token count.
    pub output_tokens: Option<i64>,
    /// Cache read token count.
    pub cache_read_input_tokens: Option<i64>,
    /// Cache creation token count.
    pub cache_creation_input_tokens: Option<i64>,
    /// Model name.
    pub model: Option<String>,
}

/// Parser failure.
#[derive(Debug, Error)]
pub enum JsonlError {
    /// I/O failed.
    #[error("failed to read {path}: {source}")]
    Io {
        /// Transcript path.
        path: String,
        /// Underlying error.
        source: std::io::Error,
    },

    /// JSON decoding failed.
    #[error("invalid JSON on line {line}: {source}")]
    Decode {
        /// One-based line number.
        line: usize,
        /// Underlying decoder error.
        source: serde_json::Error,
    },

    /// A known exhaustiveness boundary saw an unknown key.
    #[error("unconsumed JSONL field at {level} on line {line}: {field}")]
    UnknownField {
        /// One-based line number.
        line: usize,
        /// Boundary where the unknown field appeared.
        level: &'static str,
        /// Unknown field name.
        field: String,
    },
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTopLevel {
    r#type: Option<Value>,
    uuid: Option<Value>,
    #[serde(rename = "parentUuid")]
    parent_uuid: Option<Value>,
    #[serde(rename = "sessionId")]
    session_id: Option<Value>,
    timestamp: Option<Value>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<Value>,
    #[serde(rename = "agentId")]
    agent_id: Option<Value>,
    #[serde(rename = "agentName")]
    agent_name: Option<Value>,
    #[serde(rename = "teamName")]
    team_name: Option<Value>,
    slug: Option<Value>,
    cwd: Option<Value>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<Value>,
    version: Option<Value>,
    subtype: Option<Value>,
    #[serde(rename = "toolUseID")]
    tool_use_id_upper: Option<Value>,
    #[serde(rename = "toolUseId")]
    tool_use_id_lower: Option<Value>,
    #[serde(rename = "parentToolUseID")]
    parent_tool_use_id_upper: Option<Value>,
    #[serde(rename = "parentToolUseId")]
    parent_tool_use_id_lower: Option<Value>,
    role: Option<Value>,
    entrypoint: Option<Value>,
    #[serde(rename = "userType")]
    user_type: Option<Value>,
    #[serde(rename = "sessionKind")]
    session_kind: Option<Value>,
    #[serde(rename = "requestId")]
    request_id: Option<Value>,
    error: Option<Value>,
    #[serde(rename = "apiError")]
    api_error: Option<Value>,
    #[serde(rename = "isApiErrorMessage")]
    is_api_error_message: Option<Value>,
    #[serde(rename = "permissionMode")]
    permission_mode: Option<Value>,
    #[serde(rename = "isCompactSummary")]
    is_compact_summary: Option<Value>,
    #[serde(rename = "isMeta")]
    is_meta: Option<Value>,
    #[serde(rename = "isVisibleInTranscriptOnly")]
    is_visible_in_transcript_only: Option<Value>,
    origin: Option<Value>,
    #[serde(rename = "planContent")]
    plan_content: Option<Value>,
    #[serde(rename = "promptId")]
    prompt_id: Option<Value>,
    #[serde(rename = "sourceToolAssistantUUID")]
    source_tool_assistant_uuid: Option<Value>,
    #[serde(rename = "sourceToolUseID")]
    source_tool_use_id: Option<Value>,
    #[serde(rename = "thinkingMetadata")]
    thinking_metadata: Option<Value>,
    todos: Option<Value>,
    #[serde(rename = "toolUseResult")]
    tool_use_result: Option<Value>,
    data: Option<Value>,
    level: Option<Value>,
    cause: Option<Value>,
    #[serde(rename = "compactMetadata")]
    compact_metadata: Option<Value>,
    #[serde(rename = "durationMs")]
    duration_ms: Option<Value>,
    #[serde(rename = "hasOutput")]
    has_output: Option<Value>,
    #[serde(rename = "hookCount")]
    hook_count: Option<Value>,
    #[serde(rename = "hookErrors")]
    hook_errors: Option<Value>,
    #[serde(rename = "hookInfos")]
    hook_infos: Option<Value>,
    #[serde(rename = "logicalParentUuid")]
    logical_parent_uuid: Option<Value>,
    #[serde(rename = "maxRetries")]
    max_retries: Option<Value>,
    #[serde(rename = "messageCount")]
    message_count: Option<Value>,
    #[serde(rename = "preventedContinuation")]
    prevented_continuation: Option<Value>,
    #[serde(rename = "retryAttempt")]
    retry_attempt: Option<Value>,
    #[serde(rename = "retryInMs")]
    retry_in_ms: Option<Value>,
    #[serde(rename = "stopReason")]
    stop_reason: Option<Value>,
    url: Option<Value>,
    verb: Option<Value>,
    #[serde(rename = "writtenPaths")]
    written_paths: Option<Value>,
    operation: Option<Value>,
    #[serde(rename = "messageId")]
    message_id: Option<Value>,
    snapshot: Option<Value>,
    #[serde(rename = "isSnapshotUpdate")]
    is_snapshot_update: Option<Value>,
    #[serde(rename = "lastPrompt")]
    last_prompt: Option<Value>,
    attachment: Option<Value>,
    #[serde(rename = "customTitle")]
    custom_title: Option<Value>,
    #[serde(rename = "worktreeSession")]
    worktree_session: Option<Value>,
    #[serde(rename = "prNumber")]
    pr_number: Option<Value>,
    #[serde(rename = "prRepository")]
    pr_repository: Option<Value>,
    #[serde(rename = "prUrl")]
    pr_url: Option<Value>,
    #[serde(rename = "leafUuid")]
    leaf_uuid: Option<Value>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<Value>,
    #[serde(rename = "attributionPlugin")]
    attribution_plugin: Option<Value>,
    #[serde(rename = "attributionSkill")]
    attribution_skill: Option<Value>,
    #[serde(rename = "attributionAgent")]
    attribution_agent: Option<Value>,
    #[serde(rename = "attributionMcpServer")]
    attribution_mcp_server: Option<Value>,
    #[serde(rename = "attributionMcpTool")]
    attribution_mcp_tool: Option<Value>,
    #[serde(rename = "bridgeSessionId")]
    bridge_session_id: Option<Value>,
    #[serde(rename = "interruptedMessageId")]
    interrupted_message_id: Option<Value>,
    #[serde(rename = "lastSequenceNum")]
    last_sequence_num: Option<Value>,
    #[serde(rename = "mcpMeta")]
    mcp_meta: Option<Value>,
    #[serde(rename = "apiErrorStatus")]
    api_error_status: Option<Value>,
    #[serde(rename = "agentSetting")]
    agent_setting: Option<Value>,
    #[serde(rename = "errorDetails")]
    error_details: Option<Value>,
    content: Option<Value>,
    message: Option<Value>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMessage {
    id: Option<Value>,
    role: Option<Value>,
    content: Option<Value>,
    model: Option<Value>,
    r#type: Option<Value>,
    stop_reason: Option<Value>,
    stop_sequence: Option<Value>,
    stop_details: Option<Value>,
    container: Option<Value>,
    context_management: Option<Value>,
    diagnostics: Option<Value>,
    usage: Option<Value>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUsage {
    input_tokens: Option<Value>,
    output_tokens: Option<Value>,
    cache_read_input_tokens: Option<Value>,
    cache_creation_input_tokens: Option<Value>,
    cache_creation: Option<Value>,
    server_tool_use: Option<Value>,
    service_tier: Option<Value>,
    speed: Option<Value>,
    iterations: Option<Value>,
    inference_geo: Option<Value>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCacheCreation {
    ephemeral_5m_input_tokens: Option<Value>,
    ephemeral_1h_input_tokens: Option<Value>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServerToolUse {
    web_search_requests: Option<Value>,
    web_fetch_requests: Option<Value>,
}

/// Parse a main transcript file.
pub fn parse_session_file(path: &Path) -> Result<ParsedSession, JsonlError> {
    parse_file(path, "main", None)
}

/// Parse a subagent transcript file.
pub fn parse_subagent_file(path: &Path) -> Result<ParsedSession, JsonlError> {
    let agent_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.strip_prefix("agent-").unwrap_or(stem).to_string());
    let scope = agent_id
        .as_ref()
        .map_or_else(|| "subagent".to_string(), |id| format!("subagent:{id}"));
    parse_file(path, &scope, agent_id)
}

/// Scan a main transcript file for session metadata without retaining messages.
pub fn scan_session_file(path: &Path) -> Result<ParsedSession, JsonlError> {
    scan_file(path, "main", None)
}

/// Scan a subagent transcript file for session metadata without retaining messages.
pub fn scan_subagent_file(path: &Path) -> Result<ParsedSession, JsonlError> {
    let agent_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.strip_prefix("agent-").unwrap_or(stem).to_string());
    let scope = agent_id
        .as_ref()
        .map_or_else(|| "subagent".to_string(), |id| format!("subagent:{id}"));
    scan_file(path, &scope, agent_id)
}

/// Return the source scope used for parsing a transcript path.
pub fn source_scope_for_path(path: &Path, subagent: bool) -> (String, Option<String>) {
    if !subagent {
        return ("main".to_string(), None);
    }

    let agent_id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.strip_prefix("agent-").unwrap_or(stem).to_string());
    let scope = agent_id
        .as_ref()
        .map_or_else(|| "subagent".to_string(), |id| format!("subagent:{id}"));
    (scope, agent_id)
}

/// Parse one JSONL line into a normalized message.
pub fn parse_message_line(
    line: &str,
    ordinal: i64,
    source_scope: &str,
    line_number: usize,
) -> Result<Option<TranscriptMessage>, JsonlError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let value: Value = serde_json::from_str(trimmed).map_err(|source| JsonlError::Decode {
        line: line_number,
        source,
    })?;
    normalize_message(value, ordinal, source_scope, line_number).map(Some)
}

/// Return normalized content blocks for analytics.
pub fn content_blocks(content: &Value) -> Vec<&Map<String, Value>> {
    content
        .get("blocks")
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().filter_map(Value::as_object).collect())
        .unwrap_or_default()
}

fn parse_file(
    path: &Path,
    source_scope: &str,
    forced_agent_id: Option<String>,
) -> Result<ParsedSession, JsonlError> {
    let file = File::open(path).map_err(|source| JsonlError::Io {
        path: path.display().to_string(),
        source,
    })?;

    let mut messages = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|source| JsonlError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let Some(message) = parse_message_line(&line, index as i64, source_scope, index + 1)?
        else {
            continue;
        };
        messages.push(message);
    }

    let mut parsed = build_parsed_session(messages);
    if parsed.agent_id.is_none() {
        parsed.agent_id = forced_agent_id;
    }
    Ok(parsed)
}

fn scan_file(
    path: &Path,
    source_scope: &str,
    forced_agent_id: Option<String>,
) -> Result<ParsedSession, JsonlError> {
    let file = File::open(path).map_err(|source| JsonlError::Io {
        path: path.display().to_string(),
        source,
    })?;

    let mut first_ten = Vec::new();
    let mut started_at = None;
    let mut ended_at = None;
    let mut count = 0_usize;

    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|source| JsonlError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let Some(message) = parse_message_line(&line, index as i64, source_scope, index + 1)?
        else {
            continue;
        };

        if started_at.is_none() {
            started_at = message.timestamp;
        }
        if message.timestamp.is_some() {
            ended_at = message.timestamp;
        }
        if first_ten.len() < 10 {
            first_ten.push(message);
        }
        count += 1;
    }

    let refs = first_ten.iter().collect::<Vec<_>>();
    Ok(ParsedSession {
        session_id: first_string(&refs, |message| message.session_id.as_ref()),
        slug: first_string(&refs, |message| message.slug.as_ref()),
        cwd: first_string(&refs, |message| message.cwd.as_ref()),
        git_branch: first_string(&refs, |message| message.git_branch.as_ref()),
        version: first_string(&refs, |message| message.version.as_ref()),
        started_at,
        ended_at,
        agent_id: first_string(&refs, |message| message.agent_id.as_ref()).or(forced_agent_id),
        message_count: count,
        messages: Vec::new(),
    })
}

fn normalize_message(
    value: Value,
    ordinal: i64,
    source_scope: &str,
    line: usize,
) -> Result<TranscriptMessage, JsonlError> {
    let top = value.as_object().ok_or_else(|| JsonlError::UnknownField {
        line,
        level: "top-level",
        field: "<non-object>".to_string(),
    })?;
    reject_unknown(top, TOP_LEVEL_FIELDS, "top-level", line)?;

    let message = top.get("message").and_then(Value::as_object);
    if let Some(message) = message {
        reject_unknown(message, MESSAGE_FIELDS, "message", line)?;
    }

    let usage = message
        .and_then(|message| message.get("usage"))
        .and_then(Value::as_object);
    if let Some(usage) = usage {
        reject_unknown(usage, USAGE_FIELDS, "message.usage", line)?;
        validate_usage_children(usage, line)?;
    }
    validate_with_serde_deny_unknown_fields(&value, line)?;

    let record_type = string_field(top, "type");
    let timestamp = string_field(top, "timestamp").and_then(|raw| parse_timestamp(&raw));
    let message_content = message.and_then(|message| message.get("content"));
    let top_content = top.get("content");
    let content = normalize_content(message_content.or(top_content));
    let usage_tokens = usage.map_or_else(UsageTokens::default, usage_tokens);

    Ok(TranscriptMessage {
        ordinal,
        source_scope: source_scope.to_string(),
        normalized_type: normalize_type(record_type.as_deref()).to_string(),
        record_type,
        uuid: string_field(top, "uuid"),
        parent_uuid: string_field(top, "parentUuid"),
        message_id: message.and_then(|message| string_field(message, "id")),
        role: message.and_then(|message| string_field(message, "role")),
        content,
        raw_payload: value.clone(),
        timestamp,
        is_sidechain: bool_field(top, "isSidechain"),
        agent_id: string_field(top, "agentId"),
        tool_use_id: string_field(top, "toolUseID").or_else(|| string_field(top, "toolUseId")),
        parent_tool_use_id: string_field(top, "parentToolUseID")
            .or_else(|| string_field(top, "parentToolUseId")),
        session_id: string_field(top, "sessionId"),
        slug: string_field(top, "slug"),
        cwd: string_field(top, "cwd"),
        git_branch: string_field(top, "gitBranch"),
        version: string_field(top, "version"),
        team_name: string_field(top, "teamName"),
        agent_name: string_field(top, "agentName"),
        input_tokens: usage_tokens.input,
        output_tokens: usage_tokens.output,
        cache_read_input_tokens: usage_tokens.cache_read,
        cache_creation_input_tokens: usage_tokens.cache_creation,
        model: message.and_then(|message| string_field(message, "model")),
    })
}

fn validate_with_serde_deny_unknown_fields(value: &Value, line: usize) -> Result<(), JsonlError> {
    serde_json::from_value::<RawTopLevel>(value.clone())
        .map_err(|source| JsonlError::Decode { line, source })?;

    let Some(message) = value.get("message").and_then(Value::as_object) else {
        return Ok(());
    };
    serde_json::from_value::<RawMessage>(Value::Object(message.clone()))
        .map_err(|source| JsonlError::Decode { line, source })?;

    let Some(usage) = message.get("usage").and_then(Value::as_object) else {
        return Ok(());
    };
    serde_json::from_value::<RawUsage>(Value::Object(usage.clone()))
        .map_err(|source| JsonlError::Decode { line, source })?;

    if let Some(cache_creation) = usage.get("cache_creation").and_then(Value::as_object) {
        serde_json::from_value::<RawCacheCreation>(Value::Object(cache_creation.clone()))
            .map_err(|source| JsonlError::Decode { line, source })?;
    }

    if let Some(server_tool_use) = usage.get("server_tool_use").and_then(Value::as_object) {
        serde_json::from_value::<RawServerToolUse>(Value::Object(server_tool_use.clone()))
            .map_err(|source| JsonlError::Decode { line, source })?;
    }

    Ok(())
}

fn reject_unknown(
    map: &Map<String, Value>,
    allowed: &[&str],
    level: &'static str,
    line: usize,
) -> Result<(), JsonlError> {
    if let Some(field) = map.keys().find(|field| !allowed.contains(&field.as_str())) {
        return Err(JsonlError::UnknownField {
            line,
            level,
            field: field.clone(),
        });
    }
    Ok(())
}

fn validate_usage_children(usage: &Map<String, Value>, line: usize) -> Result<(), JsonlError> {
    if let Some(cache_creation) = usage.get("cache_creation").and_then(Value::as_object) {
        reject_unknown(
            cache_creation,
            CACHE_CREATION_FIELDS,
            "message.usage.cache_creation",
            line,
        )?;
    }

    if let Some(server_tool_use) = usage.get("server_tool_use").and_then(Value::as_object) {
        reject_unknown(
            server_tool_use,
            SERVER_TOOL_USE_FIELDS,
            "message.usage.server_tool_use",
            line,
        )?;
    }

    Ok(())
}

fn normalize_content(content: Option<&Value>) -> Value {
    match content {
        Some(Value::String(text)) => {
            let mut map = Map::new();
            map.insert("text".to_string(), Value::String(text.clone()));
            Value::Object(map)
        }
        Some(Value::Array(blocks)) => {
            let mut map = Map::new();
            map.insert("blocks".to_string(), Value::Array(blocks.clone()));
            Value::Object(map)
        }
        Some(Value::Object(map)) => Value::Object(map.clone()),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn build_parsed_session(messages: Vec<TranscriptMessage>) -> ParsedSession {
    let first_ten = messages.iter().take(10).collect::<Vec<_>>();
    ParsedSession {
        session_id: first_string(&first_ten, |message| message.session_id.as_ref()),
        slug: first_string(&first_ten, |message| message.slug.as_ref()),
        cwd: first_string(&first_ten, |message| message.cwd.as_ref()),
        git_branch: first_string(&first_ten, |message| message.git_branch.as_ref()),
        version: first_string(&first_ten, |message| message.version.as_ref()),
        started_at: messages.iter().find_map(|message| message.timestamp),
        ended_at: messages.iter().rev().find_map(|message| message.timestamp),
        agent_id: first_string(&first_ten, |message| message.agent_id.as_ref()),
        message_count: messages.len(),
        messages,
    }
}

fn first_string<F>(messages: &[&TranscriptMessage], accessor: F) -> Option<String>
where
    F: Fn(&TranscriptMessage) -> Option<&String>,
{
    messages
        .iter()
        .find_map(|message| accessor(message).cloned())
}

fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .ok()
}

fn string_field(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn bool_field(map: &Map<String, Value>, key: &str) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn int_field(map: &Map<String, Value>, key: &str) -> Option<i64> {
    map.get(key).and_then(Value::as_i64)
}

#[derive(Default)]
struct UsageTokens {
    input: Option<i64>,
    output: Option<i64>,
    cache_read: Option<i64>,
    cache_creation: Option<i64>,
}

fn usage_tokens(usage: &Map<String, Value>) -> UsageTokens {
    UsageTokens {
        input: int_field(usage, "input_tokens"),
        output: int_field(usage, "output_tokens"),
        cache_read: int_field(usage, "cache_read_input_tokens"),
        cache_creation: int_field(usage, "cache_creation_input_tokens"),
    }
}

fn normalize_type(raw_type: Option<&str>) -> &'static str {
    match raw_type {
        Some("user" | "human") => "user",
        Some("assistant") => "assistant",
        Some("tool_use") => "tool_use",
        Some("tool_result" | "result") => "tool_result",
        Some("progress") => "progress",
        Some("thinking") => "thinking",
        Some("file_history_snapshot" | "file-history-snapshot") => "file_history_snapshot",
        Some("queue-operation") => "queue_operation",
        Some("last-prompt") => "last_prompt",
        Some("attachment") => "attachment",
        Some("permission-mode") => "permission_mode",
        Some("custom-title") => "custom_title",
        Some("ai-title") => "ai_title",
        Some("agent-setting") => "agent_setting",
        Some("agent-name") => "agent_name",
        Some("worktree-state") => "worktree_state",
        Some("pr-link") => "pr_link",
        None | Some(_) => "system",
    }
}

const TOP_LEVEL_FIELDS: &[&str] = &[
    "type",
    "uuid",
    "parentUuid",
    "sessionId",
    "timestamp",
    "isSidechain",
    "agentId",
    "agentName",
    "teamName",
    "slug",
    "cwd",
    "gitBranch",
    "version",
    "subtype",
    "toolUseID",
    "toolUseId",
    "parentToolUseID",
    "parentToolUseId",
    "role",
    "entrypoint",
    "userType",
    "sessionKind",
    "requestId",
    "error",
    "apiError",
    "isApiErrorMessage",
    "permissionMode",
    "isCompactSummary",
    "isMeta",
    "isVisibleInTranscriptOnly",
    "origin",
    "planContent",
    "promptId",
    "sourceToolAssistantUUID",
    "sourceToolUseID",
    "thinkingMetadata",
    "todos",
    "toolUseResult",
    "data",
    "level",
    "cause",
    "compactMetadata",
    "durationMs",
    "hasOutput",
    "hookCount",
    "hookErrors",
    "hookInfos",
    "logicalParentUuid",
    "maxRetries",
    "messageCount",
    "preventedContinuation",
    "retryAttempt",
    "retryInMs",
    "stopReason",
    "url",
    "verb",
    "writtenPaths",
    "operation",
    "messageId",
    "snapshot",
    "isSnapshotUpdate",
    "lastPrompt",
    "attachment",
    "customTitle",
    "worktreeSession",
    "prNumber",
    "prRepository",
    "prUrl",
    "leafUuid",
    "aiTitle",
    "attributionPlugin",
    "attributionSkill",
    "attributionAgent",
    "attributionMcpServer",
    "attributionMcpTool",
    "bridgeSessionId",
    "interruptedMessageId",
    "lastSequenceNum",
    "mcpMeta",
    "apiErrorStatus",
    "agentSetting",
    "errorDetails",
    "content",
    "message",
];

const MESSAGE_FIELDS: &[&str] = &[
    "id",
    "role",
    "content",
    "model",
    "type",
    "stop_reason",
    "stop_sequence",
    "stop_details",
    "container",
    "context_management",
    "diagnostics",
    "usage",
];

const USAGE_FIELDS: &[&str] = &[
    "input_tokens",
    "output_tokens",
    "cache_read_input_tokens",
    "cache_creation_input_tokens",
    "cache_creation",
    "server_tool_use",
    "service_tier",
    "speed",
    "iterations",
    "inference_geo",
];

const CACHE_CREATION_FIELDS: &[&str] = &["ephemeral_5m_input_tokens", "ephemeral_1h_input_tokens"];

const SERVER_TOOL_USE_FIELDS: &[&str] = &["web_search_requests", "web_fetch_requests"];

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn parses_fixture() {
        let parsed = parse_session_file(Path::new("tests/fixtures/transcripts/short.jsonl"))
            .expect("fixture should parse");
        assert_eq!(
            parsed.session_id.as_deref(),
            Some("55604662-cf2a-4331-851a-ec234028f8ca")
        );
        assert!(!parsed.messages.is_empty());
    }

    #[test]
    fn accepts_session_kind_metadata() {
        let mut value = base_message();
        value
            .as_object_mut()
            .expect("object")
            .insert("sessionKind".to_string(), json!("bg"));

        let parsed = normalize_message(value, 0, "main", 1).expect("sessionKind should parse");

        assert_eq!(parsed.session_id.as_deref(), Some("session-a"));
    }

    proptest! {
        #[test]
        fn rejects_unknown_top_level(extra in "[a-z]{4,12}") {
            let mut value = base_message();
            let extra = format!("unknown_{extra}");
            value.as_object_mut().expect("object").insert(extra, json!(true));
            let error = normalize_message(value, 0, "main", 1).expect_err("unknown key rejected");
            assert!(matches!(error, JsonlError::UnknownField { level: "top-level", .. }));
        }

        #[test]
        fn rejects_unknown_message_field(extra in "[a-z]{4,12}") {
            let mut value = base_message();
            let extra = format!("unknown_{extra}");
            value["message"].as_object_mut().expect("message").insert(extra, json!(true));
            let error = normalize_message(value, 0, "main", 1).expect_err("unknown key rejected");
            assert!(matches!(error, JsonlError::UnknownField { level: "message", .. }));
        }

        #[test]
        fn rejects_unknown_usage_field(extra in "[a-z]{4,12}") {
            let mut value = base_message();
            let extra = format!("unknown_{extra}");
            value["message"]["usage"].as_object_mut().expect("usage").insert(extra, json!(true));
            let error = normalize_message(value, 0, "main", 1).expect_err("unknown key rejected");
            assert!(matches!(error, JsonlError::UnknownField { level: "message.usage", .. }));
        }

        #[test]
        fn rejects_unknown_cache_creation_field(extra in "[a-z]{4,12}") {
            let mut value = base_message();
            let extra = format!("unknown_{extra}");
            value["message"]["usage"]["cache_creation"] = json!({
                "ephemeral_5m_input_tokens": 0,
                "ephemeral_1h_input_tokens": 0,
                extra: true
            });
            let error = normalize_message(value, 0, "main", 1).expect_err("unknown key rejected");
            assert!(matches!(error, JsonlError::UnknownField { level: "message.usage.cache_creation", .. }));
        }

        #[test]
        fn rejects_unknown_server_tool_use_field(extra in "[a-z]{4,12}") {
            let mut value = base_message();
            let extra = format!("unknown_{extra}");
            value["message"]["usage"]["server_tool_use"] = json!({
                "web_search_requests": 0,
                "web_fetch_requests": 0,
                extra: true
            });
            let error = normalize_message(value, 0, "main", 1).expect_err("unknown key rejected");
            assert!(matches!(error, JsonlError::UnknownField { level: "message.usage.server_tool_use", .. }));
        }
    }

    fn base_message() -> Value {
        json!({
            "parentUuid": null,
            "isSidechain": false,
            "userType": "external",
            "cwd": "/tmp/project",
            "sessionId": "session-a",
            "version": "1",
            "gitBranch": "main",
            "message": {
                "model": "claude",
                "id": "msg",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "hello"}],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": 1,
                    "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": 0,
                    "output_tokens": 1,
                    "service_tier": "standard"
                }
            },
            "requestId": "req",
            "type": "assistant",
            "uuid": "uuid",
            "timestamp": "2026-02-10T21:11:59.186Z"
        })
    }
}
