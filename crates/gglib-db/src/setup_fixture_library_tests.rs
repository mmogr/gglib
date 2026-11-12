//! What gglib 0.33.0's own CLI wrote to the library, read back through the
//! repositories: models, settings and MCP servers.
//!
//! Each leniently read column (JSON or text a row mapper turns into none,
//! `[]` or a default when it cannot parse it) is checked against the value it
//! holds, since a row that loads proves nothing of them. What was put in by
//! hand (two model columns, the benchmark summary, the model file row) says
//! so.

use gglib_core::domain::{
    BodyCodec, DefaultsOrigin, DialectSpec, EmissionProfile, FileType, ImageFamily,
    InferenceConfig, ReasoningEffort, ServerConfig, TemplateCaps,
};
use gglib_core::ports::{McpServerRepository, ModelFilesRepositoryPort};
use gglib_core::{McpLifecycle, ModelCapabilities, ModelRepository, SettingsRepository};

use super::{at, open_fixture};
use crate::{
    ModelFilesRepository, SqliteMcpRepository, SqliteModelRepository, SqliteSettingsRepository,
};

const AGENT_PATH: &str = "/private/tmp/gglib-fixture/models/fixture-agent.gguf";

#[tokio::test]
async fn models_read_back_whole() {
    let (_dir, _db, pool) = open_fixture().await;
    let models = SqliteModelRepository::new(pool.clone())
        .list()
        .await
        .unwrap();
    let [image, plain, agent] = <[_; 3]>::try_from(models).unwrap();

    // Written by 0.33.0's `model add`, `model update` and `model capabilities`.
    assert_eq!((agent.id, agent.name.as_str()), (1, "Fixture Agent 0.6B"));
    assert_eq!(agent.file_path.to_str(), Some(AGENT_PATH));
    assert_eq!(agent.model_key, "local:dd313b04720d7c83");
    assert_eq!(agent.added_at, at("2026-11-12 01:37:57.941536 UTC"));
    assert_eq!(agent.param_count_b, Some(0.6));
    assert_eq!(agent.architecture.as_deref(), Some("qwen3"));
    assert_eq!(agent.file_type, Some(FileType(15)));
    assert_eq!(agent.context_length, Some(32_768));
    assert_eq!(agent.tags, ["reasoning", "agent", "format:hermes"]);
    let caps = ModelCapabilities::SUPPORTS_SYSTEM_ROLE
        | ModelCapabilities::SUPPORTS_TOOL_CALLS
        | ModelCapabilities::SUPPORTS_REASONING;
    assert_eq!(agent.capabilities, caps);
    assert_eq!(agent.metadata.len(), 5);
    assert_eq!(agent.metadata["qwen3.context_length"], "32768");
    let sampling = InferenceConfig {
        temperature: Some(0.6),
        top_p: Some(0.95),
        top_k: Some(20),
        presence_penalty: Some(1.5),
        min_p: Some(0.0),
        reasoning_effort: Some(ReasoningEffort::High),
        ..Default::default()
    };
    assert_eq!(agent.inference_defaults, Some(sampling));
    assert_eq!(agent.defaults_origin, Some(DefaultsOrigin::User));
    let server = ServerConfig {
        jinja: Some(true),
        mtp: Some(false),
        ..Default::default()
    };
    assert_eq!(agent.server_defaults, Some(server));
    let dialect = DialectSpec {
        id: "qwen-xml".into(),
        tool_open: "<tool_call>".into(),
        tool_close: "</tool_call>".into(),
        body_codecs: vec![BodyCodec::Json, BodyCodec::FunctionXml],
        emission: EmissionProfile {
            newline_after_open: true,
            newline_before_close: true,
        },
        id_prefix: "call_qwen_".into(),
    };
    assert_eq!(agent.dialect_spec, Some(dialect));
    assert_eq!(agent.image_family, None);
    // By hand: the summary a benchmark leaves, read through the join.
    let summary = agent.benchmark_summary.expect("model 1 has a summary");
    assert_eq!(
        (summary.best_tg_tps, summary.perf_run_count),
        (Some(52.5), 1)
    );
    // By hand: the caps a launch records.
    let observed = TemplateCaps {
        supports_tools: Some(true),
        supports_tool_calls: Some(true),
        supports_system_role: Some(true),
        supports_parallel_tool_calls: Some(false),
        supports_reasoning_effort: Some(true),
        supports_string_content: Some(true),
        supports_typed_content: Some(false),
        ..Default::default()
    };
    assert_eq!(agent.template_caps, Some(observed));

    assert_eq!(plain.name, "Fixture Plain");
    assert_eq!(plain.capabilities, ModelCapabilities::SUPPORTS_TOOL_CALLS);
    assert_eq!(plain.metadata["fixture.note"], "kept");
    let sampling = InferenceConfig {
        max_tokens: Some(512),
        ..Default::default()
    };
    assert_eq!(plain.inference_defaults, Some(sampling));
    // By hand: an origin other than the one an unreadable value reads as.
    assert_eq!(plain.defaults_origin, Some(DefaultsOrigin::Published));
    assert_eq!(plain.server_defaults, None);

    assert_eq!(image.name, "Fixture Image");
    assert_eq!(image.image_family, Some(ImageFamily::QwenImage21));
    assert_eq!(image.bits_per_weight, Some(32.0));
    assert_eq!(image.tags, Vec::<String>::new());

    // By hand: a download's file row.
    let files = ModelFilesRepository::new(pool.clone());
    let [file] = <[_; 1]>::try_from(files.get_by_model_id(1).await.unwrap()).unwrap();
    assert_eq!(
        (file.file_path.as_str(), file.expected_size),
        (AGENT_PATH, 321)
    );
    let verified = chrono::DateTime::parse_from_rfc3339("2026-11-12T01:30:00+00:00").unwrap();
    assert_eq!(file.last_verified_at, Some(verified.into()));
    pool.close().await;
}

#[tokio::test]
async fn settings_and_mcp_servers_read_back_whole() {
    let (_dir, _db, pool) = open_fixture().await;

    let settings = SqliteSettingsRepository::new(pool.clone())
        .load()
        .await
        .unwrap();
    assert_eq!(settings.default_model_id, Some(1));
    assert_eq!(settings.default_context_size, Some(16_384));
    assert_eq!(settings.proxy_port, Some(18_080));
    assert_eq!(settings.max_tool_iterations, Some(12));
    // 0.33.0 stored `loop_guard_mode = "refuse"`: the guard is on, with no row.
    assert_eq!(settings.loop_guard, None);
    assert!(settings.effective_loop_guard());
    assert_eq!(settings.trust_client_sampling, Some(true));
    assert_eq!(
        settings.proxy_allowed_hosts,
        Some(vec!["fixture.lan".to_owned()])
    );
    let [profile] = <[_; 1]>::try_from(settings.inference_profiles.unwrap()).unwrap();
    assert_eq!(profile.name, "fixture");
    assert_eq!(profile.description.as_deref(), Some("A fixture profile"));
    let sampling = InferenceConfig {
        temperature: Some(0.2),
        top_k: Some(10),
        ..Default::default()
    };
    assert_eq!(profile.config, sampling);

    let servers = SqliteMcpRepository::new(pool.clone()).list().await.unwrap();
    let [off, tools] = <[_; 2]>::try_from(servers).unwrap();
    assert_eq!((tools.id, tools.name.as_str()), (1, "fixture-tools"));
    assert_eq!(
        (tools.enabled, tools.allow_remote, tools.is_valid),
        (true, true, true)
    );
    assert_eq!(tools.lifecycle, McpLifecycle::Eager);
    assert_eq!(tools.config.command.as_deref(), Some("/bin/echo"));
    assert_eq!(
        tools.config.args,
        Some(vec!["hello".to_owned(), "world".to_owned()])
    );
    assert_eq!(tools.config.path_extra.as_deref(), Some("/opt/fixture/bin"));
    let env: Vec<_> = tools
        .env
        .iter()
        .map(|e| (e.key.as_str(), e.value.as_str()))
        .collect();
    assert_eq!(env, [("FIXTURE_TOKEN", "dummy")]);
    assert_eq!(tools.created_at, at("2026-11-12 01:37:58"));
    assert_eq!(
        (off.name.as_str(), off.enabled, off.allow_remote),
        ("fixture-off", false, false)
    );
    assert_eq!(off.lifecycle, McpLifecycle::Lazy);
    assert_eq!(off.config.args, Some(vec![]));
    assert_eq!(off.config.working_dir.as_deref(), Some("/tmp"));
    assert!(off.env.is_empty());
    pool.close().await;
}
