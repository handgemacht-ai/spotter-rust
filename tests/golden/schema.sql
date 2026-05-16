CREATE INDEX idx_messages_timestamp ON messages(timestamp);
CREATE INDEX idx_messages_tool_use ON messages(tool_use_id);
CREATE INDEX idx_sessions_external ON sessions(external_session_id);
CREATE INDEX idx_sessions_project ON sessions(project_alias);
CREATE INDEX idx_tool_runs_project ON tool_call_runs(project_alias);
CREATE INDEX idx_tool_runs_session ON tool_call_runs(session_id);
CREATE INDEX idx_tool_runs_status ON tool_call_runs(status);
CREATE INDEX idx_tool_runs_tool_name ON tool_call_runs(tool_name);
CREATE TABLE messages (
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
