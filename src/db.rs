//! `SQLite` storage and transcript ingestion.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::jsonl::{content_blocks, ParsedSession, TranscriptMessage};

const SCHEMA_VERSION: i32 = 4;

/// A stored transcript session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionRecord {
    /// Internal session id. Subagents use `<parent>:agent:<agent_id>`.
    pub id: String,
    /// Claude Code session id.
    pub external_session_id: String,
    /// Parent internal session id for subagent sessions.
    pub parent_session_id: Option<String>,
    /// Whether this session is a subagent transcript.
    pub is_subagent: bool,
    /// Subagent id when present.
    pub agent_id: Option<String>,
    /// Project alias.
    pub project_alias: String,
    /// Transcript file path.
    pub transcript_path: String,
    /// Working directory.
    pub cwd: Option<String>,
    /// Session slug.
    pub slug: Option<String>,
    /// Git branch.
    pub git_branch: Option<String>,
    /// Claude Code version.
    pub version: Option<String>,
    /// Started timestamp.
    pub started_at: Option<String>,
    /// Ended timestamp.
    pub ended_at: Option<String>,
    /// Message count.
    pub message_count: i64,
}

/// A derived tool call.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRun {
    /// Tool use id.
    pub tool_use_id: String,
    /// Internal session id.
    pub session_id: String,
    /// Claude Code session id.
    pub external_session_id: String,
    /// Parent session id for subagents.
    pub parent_session_id: Option<String>,
    /// Whether the run came from a subagent transcript.
    pub is_subagent: bool,
    /// Agent id when present.
    pub agent_id: Option<String>,
    /// Tool name.
    pub tool_name: String,
    /// Bash command when present.
    pub command: Option<String>,
    /// Parsed command program when present.
    pub command_program: Option<String>,
    /// Parsed command args as JSON.
    pub command_args: Vec<String>,
    /// Normalized command fingerprint.
    pub command_fingerprint: Option<String>,
    /// Concise input summary.
    pub input_summary: Option<String>,
    /// Serialized input size in bytes.
    pub input_size: Option<i64>,
    /// Serialized output size in bytes.
    pub output_size: Option<i64>,
    /// File paths touched or referenced by the tool call.
    pub file_paths: Vec<String>,
    /// Run status.
    pub status: String,
    /// Start timestamp.
    pub started_at: Option<String>,
    /// Finish timestamp.
    pub finished_at: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Starting message ordinal.
    pub start_ordinal: Option<i64>,
    /// Ending message ordinal.
    pub end_ordinal: Option<i64>,
    /// Source scope.
    pub source_scope: Option<String>,
    /// Error content for failed tool calls.
    pub error_content: Option<String>,
    /// Project alias.
    pub project_alias: String,
    /// Worktree name from `cwd`.
    pub worktree_name: Option<String>,
    /// Canonical working directory.
    pub canonical_cwd: Option<String>,
}

/// A content search match.
#[derive(Debug, Clone, Serialize)]
pub struct MessageHit {
    /// Internal session id.
    pub session_id: String,
    /// Claude Code session id.
    pub external_session_id: String,
    /// Project alias.
    pub project_alias: String,
    /// Message ordinal.
    pub ordinal: i64,
    /// Message type.
    pub record_type: String,
    /// Message role.
    pub role: Option<String>,
    /// Timestamp.
    pub timestamp: Option<String>,
    /// Text snippet.
    pub snippet: String,
}

/// Open the `SQLite` database and run migrations.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create database dir {}", parent.display()))?;
    }

    let conn = Connection::open(path)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        ",
    )?;
    migrate(&conn, path)?;
    Ok(conn)
}

/// Return the checked-in schema snapshot text for a fresh database.
pub fn canonical_schema(conn: &Connection) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT sql FROM sqlite_schema \
         WHERE sql IS NOT NULL AND type IN ('table', 'index') \
         AND name NOT LIKE 'sqlite_%' AND name NOT GLOB 'messages_fts_*' \
         ORDER BY type, name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut sql = Vec::new();
    for row in rows {
        sql.push(row?);
    }
    Ok(sql.join(";\n") + ";\n")
}

/// Compute a deterministic hash of the current logical database content.
pub fn canonical_content_hash(conn: &Connection) -> Result<String> {
    let mut hasher = Sha256::new();
    for query in [
        "SELECT alias, path FROM projects ORDER BY alias",
        "SELECT id, external_session_id, parent_session_id, is_subagent, agent_id, project_alias, transcript_path, cwd, slug, git_branch, version, started_at, ended_at, message_count FROM sessions ORDER BY id",
        "SELECT session_id, ordinal, uuid, parent_uuid, message_id, record_type, role, content, search_text, raw_payload, timestamp, is_sidechain, agent_id, tool_use_id, parent_tool_use_id, input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens, model FROM messages ORDER BY session_id, ordinal",
        "SELECT tool_use_id, session_id, external_session_id, parent_session_id, is_subagent, agent_id, tool_name, command, command_program, command_args, command_fingerprint, input_summary, input_size, output_size, file_paths, status, started_at, finished_at, duration_ms, start_ordinal, end_ordinal, source_scope, error_content, project_alias, worktree_name, canonical_cwd FROM tool_call_runs ORDER BY session_id, tool_use_id",
    ] {
        let mut stmt = conn.prepare(query)?;
        let column_count = stmt.column_count();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            for index in 0..column_count {
                let value = row.get_ref(index)?;
                hasher.update(format!("{value:?}\u{1f}"));
            }
            hasher.update(b"\n");
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Upsert a project alias.
pub fn upsert_project(conn: &Connection, alias: &str, path: &Path) -> Result<()> {
    conn.execute(
        "INSERT INTO projects(alias, path) VALUES (?1, ?2) \
         ON CONFLICT(alias) DO UPDATE SET path = excluded.path",
        params![alias, path.display().to_string()],
    )?;
    Ok(())
}

/// Ingest one parsed transcript session and derive tool call runs.
pub fn ingest_session(
    conn: &mut Connection,
    parsed: &ParsedSession,
    transcript_path: &Path,
    project_alias: &str,
    project_path: &Path,
    parent_session_id: Option<&str>,
) -> Result<SessionRecord> {
    upsert_project(conn, project_alias, project_path)?;
    let record =
        session_record_from_parsed(parsed, transcript_path, project_alias, parent_session_id)?;
    let session_id = record.id.clone();

    let tx = conn.transaction()?;
    replace_session(&tx, &session_id, &record)?;

    for message in &parsed.messages {
        insert_message(&tx, &session_id, message)?;
    }

    for run in derive_runs(parsed, &record) {
        insert_tool_call_run(&tx, &run)?;
    }

    tx.commit()?;
    Ok(record)
}

/// Ingest a transcript file without retaining all JSONL records in memory.
pub fn ingest_session_file(
    conn: &mut Connection,
    transcript_path: &Path,
    project_alias: &str,
    project_path: &Path,
    parent_session_id: Option<&str>,
) -> Result<SessionRecord> {
    let cancel = AtomicBool::new(false);
    ingest_session_file_with_cancel(
        conn,
        transcript_path,
        project_alias,
        project_path,
        parent_session_id,
        &cancel,
    )
}

/// Ingest a transcript file and stop between records if cancellation is requested.
pub fn ingest_session_file_with_cancel(
    conn: &mut Connection,
    transcript_path: &Path,
    project_alias: &str,
    project_path: &Path,
    parent_session_id: Option<&str>,
    cancel: &AtomicBool,
) -> Result<SessionRecord> {
    check_cancelled(cancel)?;
    upsert_project(conn, project_alias, project_path)?;

    let subagent = parent_session_id.is_some();
    let parsed = if subagent {
        crate::jsonl::scan_subagent_file(transcript_path)
    } else {
        crate::jsonl::scan_session_file(transcript_path)
    }
    .with_context(|| format!("failed to scan transcript {}", transcript_path.display()))?;
    let record =
        session_record_from_parsed(&parsed, transcript_path, project_alias, parent_session_id)?;
    let session_id = record.id.clone();
    let (source_scope, _) = crate::jsonl::source_scope_for_path(transcript_path, subagent);

    let tx = conn.transaction()?;
    replace_session(&tx, &session_id, &record)?;

    let file = File::open(transcript_path)
        .with_context(|| format!("failed to read transcript {}", transcript_path.display()))?;
    let mut uses = BTreeMap::new();
    let mut results = HashMap::new();

    for (index, line) in BufReader::new(file).lines().enumerate() {
        check_cancelled(cancel)?;
        let line = line
            .with_context(|| format!("failed to read transcript {}", transcript_path.display()))?;
        let Some(message) =
            crate::jsonl::parse_message_line(&line, index as i64, &source_scope, index + 1)
                .with_context(|| {
                    format!(
                        "failed to parse transcript {} line {}",
                        transcript_path.display(),
                        index + 1
                    )
                })?
        else {
            continue;
        };
        collect_tool_parts(&message, &mut uses, &mut results);
        insert_message(&tx, &session_id, &message)?;
    }

    for run in runs_from_parts(uses, results, &record) {
        insert_tool_call_run(&tx, &run)?;
    }

    tx.commit()?;
    Ok(record)
}

fn check_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("operation cancelled by SIGINT");
    }
    Ok(())
}

fn session_record_from_parsed(
    parsed: &ParsedSession,
    transcript_path: &Path,
    project_alias: &str,
    parent_session_id: Option<&str>,
) -> Result<SessionRecord> {
    let external_session_id = parsed
        .session_id
        .clone()
        .or_else(|| {
            transcript_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToString::to_string)
        })
        .context("transcript has no session id")?;
    let is_subagent = parent_session_id.is_some() || parsed.agent_id.is_some();
    let id = if is_subagent {
        let agent_id = parsed.agent_id.as_deref().unwrap_or("unknown");
        format!("{external_session_id}:agent:{agent_id}")
    } else {
        external_session_id.clone()
    };

    let record = SessionRecord {
        id,
        external_session_id,
        parent_session_id: parent_session_id.map(ToString::to_string),
        is_subagent,
        agent_id: parsed.agent_id.clone(),
        project_alias: project_alias.to_string(),
        transcript_path: transcript_path.display().to_string(),
        cwd: parsed.cwd.clone(),
        slug: parsed.slug.clone(),
        git_branch: parsed.git_branch.clone(),
        version: parsed.version.clone(),
        started_at: parsed.started_at.map(|timestamp| timestamp.to_rfc3339()),
        ended_at: parsed.ended_at.map(|timestamp| timestamp.to_rfc3339()),
        message_count: parsed.message_count as i64,
    };
    Ok(record)
}

fn replace_session(conn: &Connection, session_id: &str, record: &SessionRecord) -> Result<()> {
    conn.execute(
        "DELETE FROM tool_call_runs WHERE session_id = ?1",
        params![session_id],
    )?;
    if messages_fts_exists(conn)? {
        conn.execute(
            "DELETE FROM messages_fts WHERE session_id = ?1",
            params![session_id],
        )?;
    }
    conn.execute(
        "DELETE FROM messages WHERE session_id = ?1",
        params![session_id],
    )?;
    conn.execute(
        "INSERT INTO sessions(id, external_session_id, parent_session_id, is_subagent, agent_id, project_alias, transcript_path, cwd, slug, git_branch, version, started_at, ended_at, message_count) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
         ON CONFLICT(id) DO UPDATE SET external_session_id = excluded.external_session_id, parent_session_id = excluded.parent_session_id, is_subagent = excluded.is_subagent, agent_id = excluded.agent_id, project_alias = excluded.project_alias, transcript_path = excluded.transcript_path, cwd = excluded.cwd, slug = excluded.slug, git_branch = excluded.git_branch, version = excluded.version, started_at = excluded.started_at, ended_at = excluded.ended_at, message_count = excluded.message_count",
        params![
            &record.id,
            &record.external_session_id,
            &record.parent_session_id,
            i64::from(record.is_subagent),
            &record.agent_id,
            &record.project_alias,
            &record.transcript_path,
            &record.cwd,
            &record.slug,
            &record.git_branch,
            &record.version,
            &record.started_at,
            &record.ended_at,
            record.message_count,
        ],
    )?;
    Ok(())
}

/// Load all sessions.
pub fn list_sessions(conn: &Connection) -> Result<Vec<SessionRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, external_session_id, parent_session_id, is_subagent, agent_id, project_alias, transcript_path, cwd, slug, git_branch, version, started_at, ended_at, message_count \
         FROM sessions ORDER BY started_at DESC, id",
    )?;
    let rows = stmt.query_map([], session_from_row)?;
    collect_rows(rows)
}

/// Load all tool call runs.
pub fn list_runs(conn: &Connection) -> Result<Vec<ToolCallRun>> {
    let mut stmt = conn.prepare(
        "SELECT tool_use_id, session_id, external_session_id, parent_session_id, is_subagent, agent_id, tool_name, command, command_program, command_args, command_fingerprint, input_summary, input_size, output_size, file_paths, status, started_at, finished_at, duration_ms, start_ordinal, end_ordinal, source_scope, error_content, project_alias, worktree_name, canonical_cwd \
         FROM tool_call_runs ORDER BY COALESCE(started_at, ''), start_ordinal, tool_use_id",
    )?;
    let rows = stmt.query_map([], run_from_row)?;
    collect_rows(rows)
}

/// Find a session by internal or external id.
pub fn find_session(conn: &Connection, id: &str) -> Result<Option<SessionRecord>> {
    conn.query_row(
        "SELECT id, external_session_id, parent_session_id, is_subagent, agent_id, project_alias, transcript_path, cwd, slug, git_branch, version, started_at, ended_at, message_count \
         FROM sessions WHERE id = ?1 OR external_session_id = ?1 ORDER BY is_subagent ASC LIMIT 1",
        params![id],
        session_from_row,
    )
    .optional()
    .map_err(Into::into)
}

/// Search message content.
pub fn search_messages(conn: &Connection, needle: &str, limit: usize) -> Result<Vec<MessageHit>> {
    if needle.trim().is_empty() {
        return Ok(Vec::new());
    }

    ensure_messages_fts_current(conn)?;
    let query = fts_phrase_query(needle);
    let mut stmt = conn.prepare(
        "SELECT messages.session_id, sessions.external_session_id, sessions.project_alias, messages.ordinal, messages.record_type, messages.role, messages.timestamp, messages.content \
         FROM messages_fts \
         JOIN messages ON messages.session_id = messages_fts.session_id AND messages.ordinal = CAST(messages_fts.ordinal AS INTEGER) \
         JOIN sessions ON sessions.id = messages.session_id \
         WHERE messages_fts MATCH ?1 \
         ORDER BY messages.timestamp, messages.ordinal LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![query, limit as i64], |row| {
        let content: String = row.get(7)?;
        Ok(MessageHit {
            session_id: row.get(0)?,
            external_session_id: row.get(1)?,
            project_alias: row.get(2)?,
            ordinal: row.get(3)?,
            record_type: row.get(4)?,
            role: row.get(5)?,
            timestamp: row.get(6)?,
            snippet: truncate_chars(&plain_text(&content), 180),
        })
    })?;
    collect_rows(rows)
}

/// Load message snippets around an ordinal range.
pub fn messages_between(
    conn: &Connection,
    session_id: &str,
    min_ordinal: i64,
    max_ordinal: i64,
) -> Result<Vec<MessageHit>> {
    let mut stmt = conn.prepare(
        "SELECT messages.session_id, sessions.external_session_id, sessions.project_alias, messages.ordinal, messages.record_type, messages.role, messages.timestamp, messages.content \
         FROM messages JOIN sessions ON sessions.id = messages.session_id \
         WHERE messages.session_id = ?1 AND messages.ordinal >= ?2 AND messages.ordinal <= ?3 \
         ORDER BY messages.ordinal",
    )?;
    let rows = stmt.query_map(params![session_id, min_ordinal, max_ordinal], |row| {
        let content: String = row.get(7)?;
        Ok(MessageHit {
            session_id: row.get(0)?,
            external_session_id: row.get(1)?,
            project_alias: row.get(2)?,
            ordinal: row.get(3)?,
            record_type: row.get(4)?,
            role: row.get(5)?,
            timestamp: row.get(6)?,
            snippet: truncate_chars(&plain_text(&content), 240),
        })
    })?;
    collect_rows(rows)
}

/// Count JSONL rows in a file.
pub fn count_jsonl_lines(path: &Path) -> Result<i64> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read transcript {}", path.display()))?;
    Ok(text.lines().filter(|line| !line.trim().is_empty()).count() as i64)
}

fn migrate(conn: &Connection, path: &Path) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        anyhow::bail!("database schema version {version} is newer than this binary");
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    if version > 0 {
        snapshot_database(path, version)?;
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS projects (
            alias TEXT PRIMARY KEY,
            path TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            external_session_id TEXT NOT NULL,
            parent_session_id TEXT,
            is_subagent INTEGER NOT NULL,
            agent_id TEXT,
            project_alias TEXT NOT NULL,
            transcript_path TEXT NOT NULL,
            cwd TEXT,
            slug TEXT,
            git_branch TEXT,
            version TEXT,
            started_at TEXT,
            ended_at TEXT,
            message_count INTEGER NOT NULL,
            FOREIGN KEY(project_alias) REFERENCES projects(alias)
        );

        CREATE INDEX IF NOT EXISTS idx_sessions_external ON sessions(external_session_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_alias);

        CREATE TABLE IF NOT EXISTS messages (
            session_id TEXT NOT NULL,
            ordinal INTEGER NOT NULL,
            uuid TEXT,
            parent_uuid TEXT,
            message_id TEXT,
            record_type TEXT NOT NULL,
            role TEXT,
            content TEXT NOT NULL,
            search_text TEXT NOT NULL,
            raw_payload TEXT NOT NULL,
            timestamp TEXT,
            is_sidechain INTEGER NOT NULL,
            agent_id TEXT,
            tool_use_id TEXT,
            parent_tool_use_id TEXT,
            input_tokens INTEGER,
            output_tokens INTEGER,
            cache_read_input_tokens INTEGER,
            cache_creation_input_tokens INTEGER,
            model TEXT,
            PRIMARY KEY(session_id, ordinal),
            FOREIGN KEY(session_id) REFERENCES sessions(id)
        );

        CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
        CREATE INDEX IF NOT EXISTS idx_messages_tool_use ON messages(tool_use_id);

        CREATE TABLE IF NOT EXISTS tool_call_runs (
            tool_use_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            external_session_id TEXT NOT NULL,
            parent_session_id TEXT,
            is_subagent INTEGER NOT NULL,
            agent_id TEXT,
            tool_name TEXT NOT NULL,
            command TEXT,
            command_program TEXT,
            command_args TEXT NOT NULL,
            command_fingerprint TEXT,
            input_summary TEXT,
            input_size INTEGER,
            output_size INTEGER,
            file_paths TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT,
            finished_at TEXT,
            duration_ms INTEGER,
            start_ordinal INTEGER,
            end_ordinal INTEGER,
            source_scope TEXT,
            error_content TEXT,
            project_alias TEXT NOT NULL,
            worktree_name TEXT,
            canonical_cwd TEXT,
            PRIMARY KEY(session_id, tool_use_id),
            FOREIGN KEY(session_id) REFERENCES sessions(id),
            FOREIGN KEY(project_alias) REFERENCES projects(alias)
        );

        CREATE INDEX IF NOT EXISTS idx_tool_runs_tool_use ON tool_call_runs(tool_use_id);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_session ON tool_call_runs(session_id);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_project ON tool_call_runs(project_alias);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_status ON tool_call_runs(status);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_tool_name ON tool_call_runs(tool_name);

        ",
    )?;
    ensure_search_text_column(conn)?;
    ensure_tool_call_runs_session_key(conn)?;
    conn.execute_batch("PRAGMA user_version = 4;")?;
    Ok(())
}

fn ensure_tool_call_runs_session_key(conn: &Connection) -> Result<()> {
    if tool_call_runs_has_session_key(conn)? {
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tool_runs_tool_use ON tool_call_runs(tool_use_id)",
            [],
        )?;
        return Ok(());
    }

    conn.execute_batch(
        "
        ALTER TABLE tool_call_runs RENAME TO tool_call_runs_old;

        CREATE TABLE tool_call_runs (
            tool_use_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            external_session_id TEXT NOT NULL,
            parent_session_id TEXT,
            is_subagent INTEGER NOT NULL,
            agent_id TEXT,
            tool_name TEXT NOT NULL,
            command TEXT,
            command_program TEXT,
            command_args TEXT NOT NULL,
            command_fingerprint TEXT,
            input_summary TEXT,
            input_size INTEGER,
            output_size INTEGER,
            file_paths TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at TEXT,
            finished_at TEXT,
            duration_ms INTEGER,
            start_ordinal INTEGER,
            end_ordinal INTEGER,
            source_scope TEXT,
            error_content TEXT,
            project_alias TEXT NOT NULL,
            worktree_name TEXT,
            canonical_cwd TEXT,
            PRIMARY KEY(session_id, tool_use_id),
            FOREIGN KEY(session_id) REFERENCES sessions(id),
            FOREIGN KEY(project_alias) REFERENCES projects(alias)
        );

        INSERT INTO tool_call_runs(
            tool_use_id, session_id, external_session_id, parent_session_id, is_subagent,
            agent_id, tool_name, command, command_program, command_args, command_fingerprint,
            input_summary, input_size, output_size, file_paths, status, started_at, finished_at,
            duration_ms, start_ordinal, end_ordinal, source_scope, error_content, project_alias,
            worktree_name, canonical_cwd
        )
        SELECT
            tool_use_id, session_id, external_session_id, parent_session_id, is_subagent,
            agent_id, tool_name, command, command_program, command_args, command_fingerprint,
            input_summary, input_size, output_size, file_paths, status, started_at, finished_at,
            duration_ms, start_ordinal, end_ordinal, source_scope, error_content, project_alias,
            worktree_name, canonical_cwd
        FROM tool_call_runs_old;

        DROP TABLE tool_call_runs_old;

        CREATE INDEX IF NOT EXISTS idx_tool_runs_tool_use ON tool_call_runs(tool_use_id);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_session ON tool_call_runs(session_id);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_project ON tool_call_runs(project_alias);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_status ON tool_call_runs(status);
        CREATE INDEX IF NOT EXISTS idx_tool_runs_tool_name ON tool_call_runs(tool_name);
        ",
    )?;
    Ok(())
}

fn tool_call_runs_has_session_key(conn: &Connection) -> Result<bool> {
    let mut stmt = conn.prepare("PRAGMA table_info(tool_call_runs)")?;
    let columns = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let session_pk = columns
        .iter()
        .find_map(|(name, pk)| (name == "session_id").then_some(*pk));
    let tool_pk = columns
        .iter()
        .find_map(|(name, pk)| (name == "tool_use_id").then_some(*pk));

    Ok(session_pk == Some(1) && tool_pk == Some(2))
}

fn snapshot_database(path: &Path, version: i32) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let backup = path.with_file_name(format!(
        "{}.bak.{version}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("spotter.db")
    ));
    fs::copy(path, backup)?;
    Ok(())
}

fn rebuild_messages_fts(conn: &Connection) -> Result<()> {
    ensure_messages_fts_table(conn)?;
    conn.execute("DELETE FROM messages_fts", [])?;
    conn.execute(
        "INSERT INTO messages_fts(session_id, ordinal, content) \
         SELECT session_id, ordinal, search_text FROM messages",
        [],
    )?;
    Ok(())
}

fn ensure_messages_fts_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            session_id UNINDEXED,
            ordinal UNINDEXED,
            content
        );
        ",
    )?;
    Ok(())
}

fn ensure_search_text_column(conn: &Connection) -> Result<()> {
    let has_column = conn
        .prepare("PRAGMA table_info(messages)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|name| name == "search_text");
    if !has_column {
        conn.execute(
            "ALTER TABLE messages ADD COLUMN search_text TEXT NOT NULL DEFAULT ''",
            [],
        )?;
        conn.execute("UPDATE messages SET search_text = content", [])?;
    }
    Ok(())
}

fn ensure_messages_fts_current(conn: &Connection) -> Result<()> {
    if !messages_fts_exists(conn)? {
        rebuild_messages_fts(conn)?;
        return Ok(());
    }
    let message_count = conn.query_row("SELECT COUNT(*) FROM messages", [], |row| {
        row.get::<_, i64>(0)
    })?;
    let fts_count = conn.query_row("SELECT COUNT(*) FROM messages_fts", [], |row| {
        row.get::<_, i64>(0)
    })?;
    if message_count != fts_count {
        rebuild_messages_fts(conn)?;
    }
    Ok(())
}

fn messages_fts_exists(conn: &Connection) -> Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'messages_fts')",
        [],
        |row| row.get::<_, bool>(0),
    )
    .map_err(Into::into)
}

fn fts_phrase_query(needle: &str) -> String {
    format!("\"{}\"", needle.trim().replace('"', "\"\""))
}

fn insert_message(conn: &Connection, session_id: &str, message: &TranscriptMessage) -> Result<()> {
    let content = serde_json::to_string(&message.content)?;
    let search_text = content_text(&message.content);
    let raw_payload = serde_json::to_string(&message.raw_payload)?;
    conn.execute(
        "INSERT INTO messages(session_id, ordinal, uuid, parent_uuid, message_id, record_type, role, content, search_text, raw_payload, timestamp, is_sidechain, agent_id, tool_use_id, parent_tool_use_id, input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens, model) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            session_id,
            message.ordinal,
            &message.uuid,
            &message.parent_uuid,
            &message.message_id,
            &message.normalized_type,
            &message.role,
            &content,
            &search_text,
            &raw_payload,
            message.timestamp.map(|timestamp| timestamp.to_rfc3339()),
            i64::from(message.is_sidechain),
            &message.agent_id,
            &message.tool_use_id,
            &message.parent_tool_use_id,
            message.input_tokens,
            message.output_tokens,
            message.cache_read_input_tokens,
            message.cache_creation_input_tokens,
            &message.model,
        ],
    )?;
    Ok(())
}

fn insert_tool_call_run(conn: &Connection, run: &ToolCallRun) -> Result<()> {
    conn.execute(
        "INSERT INTO tool_call_runs(tool_use_id, session_id, external_session_id, parent_session_id, is_subagent, agent_id, tool_name, command, command_program, command_args, command_fingerprint, input_summary, input_size, output_size, file_paths, status, started_at, finished_at, duration_ms, start_ordinal, end_ordinal, source_scope, error_content, project_alias, worktree_name, canonical_cwd) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
        params![
            &run.tool_use_id,
            &run.session_id,
            &run.external_session_id,
            &run.parent_session_id,
            i64::from(run.is_subagent),
            &run.agent_id,
            &run.tool_name,
            &run.command,
            &run.command_program,
            serde_json::to_string(&run.command_args)?,
            &run.command_fingerprint,
            &run.input_summary,
            run.input_size,
            run.output_size,
            serde_json::to_string(&run.file_paths)?,
            &run.status,
            &run.started_at,
            &run.finished_at,
            run.duration_ms,
            run.start_ordinal,
            run.end_ordinal,
            &run.source_scope,
            &run.error_content,
            &run.project_alias,
            &run.worktree_name,
            &run.canonical_cwd,
        ],
    )?;
    Ok(())
}

fn derive_runs(parsed: &ParsedSession, session: &SessionRecord) -> Vec<ToolCallRun> {
    let mut uses = BTreeMap::new();
    let mut results = HashMap::new();

    for message in &parsed.messages {
        collect_tool_parts(message, &mut uses, &mut results);
    }

    runs_from_parts(uses, results, session)
}

fn collect_tool_parts(
    message: &TranscriptMessage,
    uses: &mut BTreeMap<String, ToolUseInfo>,
    results: &mut HashMap<String, ToolResultInfo>,
) {
    for block in content_blocks(&message.content) {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                if let Some(tool_use_id) = block.get("id").and_then(Value::as_str) {
                    uses.insert(
                        tool_use_id.to_string(),
                        ToolUseInfo::from_block(message, block),
                    );
                }
            }
            Some("tool_result") => {
                if let Some(tool_use_id) = block.get("tool_use_id").and_then(Value::as_str) {
                    results.insert(
                        tool_use_id.to_string(),
                        ToolResultInfo::from_block(message, block),
                    );
                }
            }
            _ => {}
        }
    }
}

fn runs_from_parts(
    uses: BTreeMap<String, ToolUseInfo>,
    mut results: HashMap<String, ToolResultInfo>,
    session: &SessionRecord,
) -> Vec<ToolCallRun> {
    let mut runs = Vec::new();
    for (tool_use_id, use_info) in uses {
        let result = results.remove(&tool_use_id);
        runs.push(build_run(tool_use_id, use_info, result, session));
    }

    for (tool_use_id, result) in results {
        runs.push(build_orphan_run(tool_use_id, result, session));
    }

    runs.sort_by(|left, right| {
        left.start_ordinal
            .cmp(&right.start_ordinal)
            .then_with(|| left.tool_use_id.cmp(&right.tool_use_id))
    });
    runs
}

#[derive(Debug)]
struct ToolUseInfo {
    tool_name: String,
    command: Option<String>,
    command_program: Option<String>,
    command_args: Vec<String>,
    command_fingerprint: Option<String>,
    input_summary: Option<String>,
    input_size: Option<i64>,
    file_paths: Vec<String>,
    started_at: Option<DateTime<Utc>>,
    start_ordinal: i64,
    source_scope: String,
    agent_id: Option<String>,
}

impl ToolUseInfo {
    fn from_block(message: &TranscriptMessage, block: &Map<String, Value>) -> Self {
        let input = block.get("input").unwrap_or(&Value::Null);
        let command = input
            .get("command")
            .and_then(Value::as_str)
            .map(|command| truncate_chars(command, 500));
        let (command_program, command_args) = parse_command(command.as_deref());
        Self {
            tool_name: block
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .to_string(),
            command_fingerprint: command.as_deref().map(command_fingerprint),
            input_summary: input_summary(input),
            input_size: Some(input.to_string().len() as i64),
            file_paths: file_paths_from_input(input, command.as_deref()),
            command,
            command_program,
            command_args,
            started_at: message.timestamp,
            start_ordinal: message.ordinal,
            source_scope: message.source_scope.clone(),
            agent_id: message.agent_id.clone(),
        }
    }
}

#[derive(Debug)]
struct ToolResultInfo {
    is_error: bool,
    error_content: Option<String>,
    output_size: Option<i64>,
    finished_at: Option<DateTime<Utc>>,
    end_ordinal: i64,
}

impl ToolResultInfo {
    fn from_block(message: &TranscriptMessage, block: &Map<String, Value>) -> Self {
        let content = block.get("content").unwrap_or(&Value::Null);
        let is_error = block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Self {
            is_error,
            error_content: is_error.then(|| truncate_chars(&content_text(content), 2_000)),
            output_size: Some(content.to_string().len() as i64),
            finished_at: message.timestamp,
            end_ordinal: message.ordinal,
        }
    }
}

fn build_run(
    tool_use_id: String,
    use_info: ToolUseInfo,
    result: Option<ToolResultInfo>,
    session: &SessionRecord,
) -> ToolCallRun {
    let status = match &result {
        Some(result) if result.is_error => "error",
        Some(_) => "completed",
        None => "ongoing",
    };
    let finished_at = result.as_ref().and_then(|result| result.finished_at);
    let duration_ms = duration_ms(use_info.started_at, finished_at);

    ToolCallRun {
        tool_use_id,
        session_id: session.id.clone(),
        external_session_id: session.external_session_id.clone(),
        parent_session_id: session.parent_session_id.clone(),
        is_subagent: session.is_subagent,
        agent_id: use_info.agent_id.or_else(|| session.agent_id.clone()),
        tool_name: use_info.tool_name,
        command: use_info.command,
        command_program: use_info.command_program,
        command_args: use_info.command_args,
        command_fingerprint: use_info.command_fingerprint,
        input_summary: use_info.input_summary,
        input_size: use_info.input_size,
        output_size: result.as_ref().and_then(|result| result.output_size),
        file_paths: use_info.file_paths,
        status: status.to_string(),
        started_at: use_info.started_at.map(|timestamp| timestamp.to_rfc3339()),
        finished_at: finished_at.map(|timestamp| timestamp.to_rfc3339()),
        duration_ms,
        start_ordinal: Some(use_info.start_ordinal),
        end_ordinal: result.as_ref().map(|result| result.end_ordinal),
        source_scope: Some(use_info.source_scope),
        error_content: result.and_then(|result| result.error_content),
        project_alias: session.project_alias.clone(),
        worktree_name: session.cwd.as_deref().and_then(worktree_name),
        canonical_cwd: session.cwd.clone(),
    }
}

fn build_orphan_run(
    tool_use_id: String,
    result: ToolResultInfo,
    session: &SessionRecord,
) -> ToolCallRun {
    ToolCallRun {
        tool_use_id,
        session_id: session.id.clone(),
        external_session_id: session.external_session_id.clone(),
        parent_session_id: session.parent_session_id.clone(),
        is_subagent: session.is_subagent,
        agent_id: session.agent_id.clone(),
        tool_name: "Unknown".to_string(),
        command: None,
        command_program: None,
        command_args: Vec::new(),
        command_fingerprint: None,
        input_summary: None,
        input_size: None,
        output_size: result.output_size,
        file_paths: Vec::new(),
        status: "orphan".to_string(),
        started_at: None,
        finished_at: result.finished_at.map(|timestamp| timestamp.to_rfc3339()),
        duration_ms: None,
        start_ordinal: None,
        end_ordinal: Some(result.end_ordinal),
        source_scope: None,
        error_content: result.error_content,
        project_alias: session.project_alias.clone(),
        worktree_name: session.cwd.as_deref().and_then(worktree_name),
        canonical_cwd: session.cwd.clone(),
    }
}

fn duration_ms(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Option<i64> {
    Some(end?.signed_duration_since(start?).num_milliseconds())
}

fn input_summary(input: &Value) -> Option<String> {
    let object = input.as_object()?;
    let parts = ["file_path", "pattern", "command", "description"]
        .iter()
        .filter_map(|key| {
            object.get(*key).map(|value| {
                let text = value
                    .as_str()
                    .map_or_else(|| value.to_string(), ToString::to_string);
                format!("{key}={}", truncate_chars(&text, 80))
            })
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(truncate_chars(&parts.join(", "), 200))
    }
}

fn file_paths_from_input(input: &Value, command: Option<&str>) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for key in ["file_path", "path"] {
        if let Some(path) = input.get(key).and_then(Value::as_str) {
            paths.insert(path.to_string());
        }
    }

    if let Some(command) = command {
        for token in command.split_whitespace() {
            let trimmed = token.trim_matches(|ch: char| {
                matches!(ch, '\'' | '"' | ',' | ';' | ':' | ')' | '(' | '[' | ']')
            });
            if trimmed.contains('/')
                && !trimmed.starts_with("http://")
                && !trimmed.starts_with("https://")
            {
                paths.insert(trimmed.to_string());
            }
        }
    }

    paths.into_iter().collect()
}

fn parse_command(command: Option<&str>) -> (Option<String>, Vec<String>) {
    let Some(command) = command else {
        return (None, Vec::new());
    };
    let mut parts = command.split_whitespace();
    let program = parts.next().map(ToString::to_string);
    let args = parts.map(ToString::to_string).collect();
    (program, args)
}

fn command_fingerprint(command: &str) -> String {
    let path_regex = Regex::new(r"/[^\s]+").ok();
    let normalized = path_regex.map_or_else(
        || command.to_string(),
        |regex| regex.replace_all(command, "<path>/").to_string(),
    );
    truncate_chars(
        &normalized.split_whitespace().collect::<Vec<_>>().join(" "),
        200,
    )
}

fn content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(content_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .and_then(Value::as_str)
            .map_or_else(|| value.to_string(), ToString::to_string),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn plain_text(json_text: &str) -> String {
    serde_json::from_str::<Value>(json_text)
        .map_or_else(|_| json_text.to_string(), |value| content_text(&value))
}

fn worktree_name(cwd: &str) -> Option<String> {
    Path::new(cwd)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
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

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>> {
    let mut output = Vec::new();
    for row in rows {
        output.push(row?);
    }
    Ok(output)
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: row.get(0)?,
        external_session_id: row.get(1)?,
        parent_session_id: row.get(2)?,
        is_subagent: row.get::<_, i64>(3)? != 0,
        agent_id: row.get(4)?,
        project_alias: row.get(5)?,
        transcript_path: row.get(6)?,
        cwd: row.get(7)?,
        slug: row.get(8)?,
        git_branch: row.get(9)?,
        version: row.get(10)?,
        started_at: row.get(11)?,
        ended_at: row.get(12)?,
        message_count: row.get(13)?,
    })
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolCallRun> {
    let command_args: String = row.get(9)?;
    let file_paths: String = row.get(14)?;
    Ok(ToolCallRun {
        tool_use_id: row.get(0)?,
        session_id: row.get(1)?,
        external_session_id: row.get(2)?,
        parent_session_id: row.get(3)?,
        is_subagent: row.get::<_, i64>(4)? != 0,
        agent_id: row.get(5)?,
        tool_name: row.get(6)?,
        command: row.get(7)?,
        command_program: row.get(8)?,
        command_args: serde_json::from_str(&command_args).unwrap_or_default(),
        command_fingerprint: row.get(10)?,
        input_summary: row.get(11)?,
        input_size: row.get(12)?,
        output_size: row.get(13)?,
        file_paths: serde_json::from_str(&file_paths).unwrap_or_default(),
        status: row.get(15)?,
        started_at: row.get(16)?,
        finished_at: row.get(17)?,
        duration_ms: row.get(18)?,
        start_ordinal: row.get(19)?,
        end_ordinal: row.get(20)?,
        source_scope: row.get(21)?,
        error_content: row.get(22)?,
        project_alias: row.get(23)?,
        worktree_name: row.get(24)?,
        canonical_cwd: row.get(25)?,
    })
}

/// Find main transcript files under a root.
pub fn transcript_files_under(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("jsonl")
                && !path
                    .components()
                    .any(|component| component.as_os_str() == std::ffi::OsStr::new("subagents"))
        })
        .collect()
}
