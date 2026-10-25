-- generation 3
-- user_version 4

CREATE INDEX idx_agentic_results_model ON benchmark_agentic_results(model_id, created_at DESC);

CREATE INDEX idx_compare_results_model ON model_compare_results(model_id, created_at DESC);

CREATE INDEX idx_conversations_lineage ON chat_conversations(lineage_id);

CREATE INDEX idx_loop_guard_trips_recorded_at ON loop_guard_trips(recorded_at);

CREATE INDEX idx_mcp_env_server ON mcp_server_env(server_id);

CREATE INDEX idx_message_attachments_attachment ON message_attachments(attachment_id);

CREATE INDEX idx_messages_conversation ON chat_messages(conversation_id);

CREATE INDEX idx_model_components_path ON model_components(path);

CREATE INDEX idx_model_files_model_id ON model_files(model_id);

CREATE INDEX idx_models_file_path ON models(file_path);

CREATE UNIQUE INDEX idx_models_model_key ON models(model_key);

CREATE INDEX idx_models_name ON models(name);

CREATE INDEX idx_perf_results_model ON model_perf_results(model_id, created_at DESC);

CREATE INDEX idx_tune_results_model ON benchmark_tune_results(model_id, created_at DESC);

CREATE TABLE attachments (
            id TEXT PRIMARY KEY NOT NULL,
            mime TEXT NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            data BLOB NOT NULL,
            created_at TEXT NOT NULL
        );

CREATE TABLE benchmark_agentic_results (
            id                INTEGER PRIMARY KEY,
            model_id          INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id            INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            raw_composite     REAL    NOT NULL,
            gglib_composite   REAL    NOT NULL,
            report_json       TEXT    NOT NULL,
            created_at        TEXT    NOT NULL
        );

CREATE TABLE benchmark_runs (
            id           INTEGER PRIMARY KEY,
            run_type     TEXT    NOT NULL,
            status       TEXT    NOT NULL,
            model_ids    TEXT    NOT NULL,
            prompt_text  TEXT,
            system_prompt TEXT,
            config_json  TEXT,
            applied_json TEXT,
            error        TEXT,
            created_at   TEXT    NOT NULL,
            completed_at TEXT
        );

CREATE TABLE benchmark_tune_results (
            id                INTEGER PRIMARY KEY,
            model_id          INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id            INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            config_json       TEXT    NOT NULL,
            source_json       TEXT    NOT NULL,
            composite_score   REAL    NOT NULL,
            pruned            INTEGER NOT NULL DEFAULT 0,
            tg_tps            REAL,
            task_results_json TEXT    NOT NULL,
            created_at        TEXT    NOT NULL
        );

CREATE TABLE chat_conversations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            model_id INTEGER,
            system_prompt TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')), settings TEXT, lineage_id INTEGER, branch_of INTEGER,
            FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE SET NULL
        );

CREATE TABLE chat_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id INTEGER NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('system', 'user', 'assistant', 'tool')),
            content TEXT NOT NULL,
            metadata TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')), origin_id INTEGER,
            FOREIGN KEY (conversation_id) REFERENCES chat_conversations(id) ON DELETE CASCADE
        );

CREATE TABLE loop_guard_scans (
            day           INTEGER NOT NULL,
            model_name    TEXT    NOT NULL,
            gglib_version TEXT    NOT NULL,
            mode          TEXT    NOT NULL,
            scanned       INTEGER NOT NULL,
            PRIMARY KEY (day, model_name, gglib_version, mode)
        );

CREATE TABLE loop_guard_trips (
            id             INTEGER PRIMARY KEY,
            recorded_at    INTEGER NOT NULL,
            model_name     TEXT    NOT NULL,
            gglib_version  TEXT    NOT NULL,
            mode           TEXT    NOT NULL,
            detector       TEXT    NOT NULL,
            signature_hash TEXT,
            session_hash   TEXT,
            repeat_count   INTEGER,
            threshold      INTEGER
        );

CREATE TABLE mcp_server_env (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            FOREIGN KEY (server_id) REFERENCES mcp_servers(id) ON DELETE CASCADE,
            UNIQUE(server_id, key)
        );

CREATE TABLE mcp_servers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            type TEXT NOT NULL CHECK (type IN ('stdio', 'sse')),
            enabled INTEGER NOT NULL DEFAULT 1,
            allow_remote INTEGER NOT NULL DEFAULT 0,
            lifecycle TEXT NOT NULL DEFAULT 'lazy' CHECK (lifecycle IN ('eager', 'lazy', 'manual')),
            command TEXT,
            resolved_path_cache TEXT,
            args TEXT NOT NULL DEFAULT '[]',
            cwd TEXT,
            path_extra TEXT,
            url TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_connected_at TEXT,
            is_valid INTEGER NOT NULL DEFAULT 0,
            last_error TEXT
        );

CREATE TABLE message_attachments (
            message_id INTEGER NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
            attachment_id TEXT NOT NULL REFERENCES attachments(id),
            position INTEGER NOT NULL,
            PRIMARY KEY (message_id, position)
        );

CREATE TABLE model_benchmark_summaries (
            model_id             INTEGER PRIMARY KEY REFERENCES models(id) ON DELETE CASCADE,
            best_tg_tps          REAL,
            best_pp_tps          REAL,
            latest_tg_tps        REAL,
            latest_pp_tps        REAL,
            latest_backend       TEXT,
            perf_run_count       INTEGER NOT NULL DEFAULT 0,
            compare_run_count    INTEGER NOT NULL DEFAULT 0,
            last_benchmarked_at  TEXT    NOT NULL,
            updated_at           TEXT    NOT NULL
        );

CREATE TABLE model_compare_results (
            id               INTEGER PRIMARY KEY,
            model_id         INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id           INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            prompt_text      TEXT    NOT NULL,
            system_prompt    TEXT,
            response_text    TEXT    NOT NULL,
            was_truncated    INTEGER NOT NULL DEFAULT 0,
            prompt_tokens    INTEGER,
            completion_tokens INTEGER,
            prompt_ms        REAL,
            generation_ms    REAL,
            prompt_tps       REAL,
            generation_tps   REAL,
            created_at       TEXT    NOT NULL
        );

CREATE TABLE model_components (
            model_id INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            role TEXT NOT NULL,
            path TEXT NOT NULL,
            PRIMARY KEY (model_id, role)
        );

CREATE TABLE model_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model_id INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            file_index INTEGER NOT NULL,
            expected_size INTEGER NOT NULL,
            hf_oid TEXT,
            last_verified_at TEXT,
            FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE,
            UNIQUE (model_id, file_path)
        );

CREATE TABLE model_perf_results (
            id           INTEGER PRIMARY KEY,
            model_id     INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id       INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            pp_tps       REAL    NOT NULL,
            tg_tps       REAL    NOT NULL,
            pp_tokens    INTEGER NOT NULL,
            tg_tokens    INTEGER NOT NULL,
            backend      TEXT,
            ngl          INTEGER,
            context_size INTEGER,
            repetitions  INTEGER NOT NULL,
            created_at   TEXT    NOT NULL
        );

CREATE TABLE models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            projector_path TEXT,
            param_count_b REAL NOT NULL,
            architecture TEXT,
            quantization TEXT,
            context_length INTEGER,
            inference_defaults TEXT,
            defaults_origin TEXT,
            server_defaults TEXT,
            expert_count INTEGER,
            expert_used_count INTEGER,
            expert_shared_count INTEGER,
            metadata TEXT,
            added_at TEXT NOT NULL,
            hf_repo_id TEXT,
            hf_commit_sha TEXT,
            hf_filename TEXT,
            download_date TEXT,
            last_update_check TEXT,
            tags TEXT DEFAULT '[]',
            model_key TEXT NOT NULL,
            file_paths_json TEXT,
            capabilities INTEGER DEFAULT 0,
            dialect_spec TEXT,
            template_caps TEXT,
            image_family TEXT
        );

CREATE TABLE schema_generation (generation INTEGER NOT NULL);

CREATE TABLE settings_kv (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

