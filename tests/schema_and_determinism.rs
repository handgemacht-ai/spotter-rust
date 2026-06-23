use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde_json::{json, Value};
use tempfile::NamedTempFile;

use spotter::{config::Config, db, jsonl};

#[test]
fn schema_matches_checked_in_snapshot() {
    let db_file = NamedTempFile::new().expect("temp db");
    let conn = db::open(db_file.path()).expect("open db");
    let actual = db::canonical_schema(&conn).expect("schema");
    if std::env::var_os("SPOTTER_REGEN_GOLDEN").is_some() {
        fs::write("tests/golden/schema.sql", &actual).expect("write schema golden");
        return;
    }
    let expected = include_str!("golden/schema.sql");
    assert_eq!(actual, expected);
}

#[test]
fn migration_from_v2_snapshots_backfills_and_preserves_data() {
    let db_file = NamedTempFile::new().expect("temp db");
    seed_v2_database(db_file.path());

    let conn = db::open(db_file.path()).expect("migrate db");
    let backup = db_file.path().with_file_name(format!(
        "{}.bak.2",
        db_file
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("temp file name")
    ));
    assert!(backup.exists(), "missing migration backup");

    let user_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user version");
    assert_eq!(user_version, 5);

    let message_count: i64 = conn
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .expect("message count");
    assert_eq!(message_count, 1);

    let search_text: String = conn
        .query_row(
            "SELECT search_text FROM messages WHERE ordinal = 0",
            [],
            |row| row.get(0),
        )
        .expect("search text");
    assert_eq!(search_text, "{\"text\":\"hello\"}");

    for index in [
        "idx_messages_timestamp",
        "idx_messages_tool_use",
        "idx_sessions_external",
        "idx_sessions_project",
        "idx_tool_runs_project",
        "idx_tool_runs_session",
        "idx_tool_runs_status",
        "idx_tool_runs_tool_name",
        "idx_tool_runs_tool_use",
    ] {
        let exists: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name = ?1",
                [index],
                |row| row.get(0),
            )
            .expect("index lookup");
        assert_eq!(exists, 1, "missing index {index}");
    }

    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok");
}

#[test]
fn newer_schema_version_is_rejected_without_snapshot() {
    let db_file = NamedTempFile::new().expect("temp db");
    let conn = rusqlite::Connection::open(db_file.path()).expect("open seed db");
    conn.execute_batch("PRAGMA user_version = 99;")
        .expect("set future user version");
    drop(conn);

    let error = db::open(db_file.path()).expect_err("future schema should fail");
    assert!(
        error.to_string().contains("newer than this binary"),
        "unexpected error: {error:#}"
    );

    let backup = db_file.path().with_file_name(format!(
        "{}.bak.99",
        db_file
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("temp file name")
    ));
    assert!(!backup.exists(), "newer schema should not be snapshotted");
}

#[test]
fn syncing_same_jsonl_twice_is_deterministic() {
    let db_file = NamedTempFile::new().expect("temp db");
    let mut conn = db::open(db_file.path()).expect("open db");
    let fixture = Path::new("tests/fixtures/transcripts/tool_heavy.jsonl");
    let config = Config::default();

    sync_once(&mut conn, fixture, &config);
    let first = db::canonical_content_hash(&conn).expect("hash");
    sync_once(&mut conn, fixture, &config);
    let second = db::canonical_content_hash(&conn).expect("hash");

    assert_eq!(first, second);
}

#[test]
fn duplicate_tool_ids_are_scoped_by_session() {
    let db_file = NamedTempFile::new().expect("temp db");
    let mut conn = db::open(db_file.path()).expect("open db");

    let first = parsed_session_with_tool_id("session-a", "toolu_duplicate", "echo a");
    let second = parsed_session_with_tool_id("session-b", "toolu_duplicate", "echo b");

    db::ingest_session(
        &mut conn,
        &first,
        Path::new("/tmp/session-a.jsonl"),
        "fixture",
        Path::new("/tmp"),
        None,
    )
    .expect("first ingest");
    db::ingest_session(
        &mut conn,
        &second,
        Path::new("/tmp/session-b.jsonl"),
        "fixture",
        Path::new("/tmp"),
        None,
    )
    .expect("second ingest with duplicate tool id");

    let duplicate_runs: i64 = conn
        .query_row(
            "SELECT count(*) FROM tool_call_runs WHERE tool_use_id = 'toolu_duplicate'",
            [],
            |row| row.get(0),
        )
        .expect("duplicate run count");
    assert_eq!(duplicate_runs, 2);

    let distinct_sessions: i64 = conn
        .query_row(
            "SELECT count(DISTINCT session_id) FROM tool_call_runs WHERE tool_use_id = 'toolu_duplicate'",
            [],
            |row| row.get(0),
        )
        .expect("distinct session count");
    assert_eq!(distinct_sessions, 2);
}

#[test]
fn streaming_ingest_matches_parsed_ingest() {
    let parsed_db = NamedTempFile::new().expect("parsed db");
    let streaming_db = NamedTempFile::new().expect("streaming db");
    let fixture = Path::new("tests/fixtures/transcripts/tool_heavy.jsonl");
    let config = Config::default();

    let mut parsed_conn = db::open(parsed_db.path()).expect("open parsed db");
    let parsed = jsonl::parse_session_file(fixture).expect("parse");
    let alias = config.alias_for_cwd(parsed.cwd.as_deref());
    let project_path = parsed
        .cwd
        .as_ref()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    db::ingest_session(
        &mut parsed_conn,
        &parsed,
        fixture,
        &alias,
        &project_path,
        None,
    )
    .expect("parsed ingest");

    let mut streaming_conn = db::open(streaming_db.path()).expect("open streaming db");
    let scanned = jsonl::scan_session_file(fixture).expect("scan");
    let alias = config.alias_for_cwd(scanned.cwd.as_deref());
    let project_path = scanned
        .cwd
        .as_ref()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    db::ingest_session_file(&mut streaming_conn, fixture, &alias, &project_path, None)
        .expect("streaming ingest");

    assert_eq!(
        db::canonical_content_hash(&parsed_conn).expect("parsed hash"),
        db::canonical_content_hash(&streaming_conn).expect("streaming hash")
    );
}

#[test]
fn streaming_ingest_stops_when_cancelled() {
    let streaming_db = NamedTempFile::new().expect("streaming db");
    let mut streaming_conn = db::open(streaming_db.path()).expect("open streaming db");
    let fixture = Path::new("tests/fixtures/transcripts/tool_heavy.jsonl");
    let alias = "spotter".to_string();
    let project_path =
        PathBuf::from("/home/USER/projects/spotter-worktrees/spotter-public-fixture");
    let cancel = AtomicBool::new(true);

    let error = db::ingest_session_file_with_cancel(
        &mut streaming_conn,
        fixture,
        &alias,
        &project_path,
        None,
        &cancel,
    )
    .expect_err("cancelled ingest should fail");

    assert!(error.to_string().contains("cancelled"));
}

#[test]
fn seeded_database_keeps_data_indexes_and_integrity() {
    let db_file = NamedTempFile::new().expect("temp db");
    let mut conn = db::open(db_file.path()).expect("open db");
    let fixture = Path::new("tests/fixtures/transcripts/tool_heavy.jsonl");
    let config = Config::default();

    sync_once(&mut conn, fixture, &config);

    let message_count: i64 = conn
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .expect("message count");
    let run_count: i64 = conn
        .query_row("SELECT count(*) FROM tool_call_runs", [], |row| row.get(0))
        .expect("run count");
    assert_eq!(message_count, 39);
    assert_eq!(run_count, 6);

    for index in [
        "idx_messages_timestamp",
        "idx_messages_tool_use",
        "idx_sessions_external",
        "idx_sessions_project",
        "idx_tool_runs_project",
        "idx_tool_runs_session",
        "idx_tool_runs_status",
        "idx_tool_runs_tool_name",
        "idx_tool_runs_tool_use",
    ] {
        let exists: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name = ?1",
                [index],
                |row| row.get(0),
            )
            .expect("index lookup");
        assert_eq!(exists, 1, "missing index {index}");
    }

    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity check");
    assert_eq!(integrity, "ok");
}

fn parsed_session_with_tool_id(
    session_id: &str,
    tool_use_id: &str,
    command: &str,
) -> jsonl::ParsedSession {
    jsonl::ParsedSession {
        session_id: Some(session_id.to_string()),
        slug: None,
        cwd: Some("/tmp".to_string()),
        git_branch: Some("main".to_string()),
        version: Some("test".to_string()),
        started_at: None,
        ended_at: None,
        agent_id: None,
        message_count: 2,
        messages: vec![
            transcript_message(
                session_id,
                0,
                "assistant",
                json!({
                    "blocks": [{
                        "type": "tool_use",
                        "id": tool_use_id,
                        "name": "Bash",
                        "input": {"command": command}
                    }]
                }),
            ),
            transcript_message(
                session_id,
                1,
                "user",
                json!({
                    "blocks": [{
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": "ok",
                        "is_error": false
                    }]
                }),
            ),
        ],
    }
}

fn transcript_message(
    session_id: &str,
    ordinal: i64,
    normalized_type: &str,
    content: Value,
) -> jsonl::TranscriptMessage {
    jsonl::TranscriptMessage {
        ordinal,
        source_scope: "main".to_string(),
        record_type: Some(normalized_type.to_string()),
        normalized_type: normalized_type.to_string(),
        uuid: Some(format!("{session_id}-{ordinal}")),
        parent_uuid: None,
        message_id: None,
        role: None,
        content,
        raw_payload: Value::Null,
        timestamp: None,
        is_sidechain: false,
        agent_id: None,
        tool_use_id: None,
        parent_tool_use_id: None,
        session_id: Some(session_id.to_string()),
        slug: None,
        cwd: Some("/tmp".to_string()),
        git_branch: Some("main".to_string()),
        version: Some("test".to_string()),
        team_name: None,
        agent_name: None,
        input_tokens: None,
        output_tokens: None,
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        model: None,
    }
}

fn sync_once(conn: &mut rusqlite::Connection, fixture: &Path, config: &Config) {
    let parsed = jsonl::parse_session_file(fixture).expect("parse");
    let alias = config.alias_for_cwd(parsed.cwd.as_deref());
    let project_path = parsed
        .cwd
        .as_ref()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    db::ingest_session(conn, &parsed, fixture, &alias, &project_path, None).expect("ingest");
}

fn seed_v2_database(path: &Path) {
    let conn = rusqlite::Connection::open(path).expect("open seed db");
    conn.execute_batch(
        "
        CREATE TABLE projects (
            alias TEXT PRIMARY KEY,
            path TEXT NOT NULL
        );

        CREATE TABLE sessions (
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

        CREATE TABLE messages (
            session_id TEXT NOT NULL,
            ordinal INTEGER NOT NULL,
            uuid TEXT,
            parent_uuid TEXT,
            message_id TEXT,
            record_type TEXT NOT NULL,
            role TEXT,
            content TEXT NOT NULL,
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

        CREATE TABLE tool_call_runs (
            tool_use_id TEXT PRIMARY KEY,
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
            FOREIGN KEY(session_id) REFERENCES sessions(id),
            FOREIGN KEY(project_alias) REFERENCES projects(alias)
        );

        INSERT INTO projects(alias, path) VALUES ('legacy', '/tmp/legacy');
        INSERT INTO sessions(
            id, external_session_id, parent_session_id, is_subagent, agent_id, project_alias,
            transcript_path, cwd, slug, git_branch, version, started_at, ended_at, message_count
        ) VALUES (
            'legacy-session', 'legacy-session', NULL, 0, NULL, 'legacy',
            '/tmp/legacy.jsonl', '/tmp/legacy', NULL, 'main', '1', NULL, NULL, 1
        );
        INSERT INTO messages(
            session_id, ordinal, uuid, parent_uuid, message_id, record_type, role, content,
            raw_payload, timestamp, is_sidechain, agent_id, tool_use_id, parent_tool_use_id,
            input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens,
            model
        ) VALUES (
            'legacy-session', 0, NULL, NULL, NULL, 'user', 'user', '{\"text\":\"hello\"}',
            '{}', NULL, 0, NULL, NULL, NULL, 1, 0, 0, 0, NULL
        );
        PRAGMA user_version = 2;
        ",
    )
    .expect("seed v2 db");
}
