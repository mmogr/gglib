//! The rows of the library put in by hand, in the shape gglib 0.33.0 stores
//! them, read back through the repositories: chats, their branches and
//! pictures, benchmarks and the loop guard's counts.
//!
//! As for the models, each leniently read column is checked against the
//! value it holds.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::attachment::AttachmentId;
use gglib_core::domain::benchmark::{BenchmarkRunStatus as Status, BenchmarkRunType as Kind};
use gglib_core::domain::benchmark::{CandidateSource, TaskCategory};
use gglib_core::domain::chat::ConversationSettings;
use gglib_core::domain::chat::MessageRole::{Assistant, System, Tool, User};
use gglib_core::domain::{InferenceConfig, Machine, ModelRef, Thinking};
use gglib_core::ports::BenchmarkRepositoryPort;
use gglib_core::ports::attachment_store::AttachmentStore;
use gglib_core::ports::chat_history::ChatHistoryRepository;
use serde_json::json;

use super::{at, open_fixture};
use crate::{
    SqliteAttachmentStore, SqliteBenchmarkRepository, SqliteChatHistoryRepository,
    SqliteLoopGuardTripLog,
};

const PIXEL: &str = "e5dd92acd7d1f9550a1848c765289b33b408f0f6c002fd935696ac778965182d";

/// All by hand: 0.33.0's CLI saves no chat without a running model.
#[tokio::test]
async fn chats_branches_and_pictures_read_back_whole() {
    let (_dir, _db, pool) = open_fixture().await;
    let chats = SqliteChatHistoryRepository::new(pool.clone());

    let [first, branch] = <[_; 2]>::try_from(chats.list_conversations().await.unwrap()).unwrap();
    let [first, branch] = if first.id == 1 {
        [first, branch]
    } else {
        [branch, first]
    };
    let settings = ConversationSettings {
        model_name: Some("Fixture Agent 0.6B".into()),
        model: Some(ModelRef {
            machine: Machine::Local,
            id: 1,
        }),
        temperature: Some(0.6),
        top_k: Some(20),
        max_tokens: Some(1024),
        profile: Some("fixture".into()),
        ctx_size: Some("16384".into()),
        tools: vec!["fixture-tools:echo".into()],
        tool_timeout_ms: Some(30_000),
        max_parallel: Some(2),
        max_iterations: Some(12),
        thinking: Some(Thinking::Off),
        ..Default::default()
    };
    assert_eq!(first.settings.as_ref(), Some(&settings));
    assert_eq!(
        (first.model_id, first.system_prompt.as_deref()),
        (Some(1), Some("Be brief."))
    );
    assert_eq!((first.branch_of, first.lineage_id), (None, None));
    assert_eq!(branch.settings, Some(settings));
    assert_eq!((branch.branch_of, branch.lineage_id), (Some(1), Some(1)));

    let messages = chats.get_messages(1).await.unwrap();
    let roles: Vec<_> = messages.iter().map(|m| m.role).collect();
    assert_eq!(roles, [System, User, Assistant, Tool, Assistant]);
    let [image] = <[_; 1]>::try_from(messages[1].images.clone()).unwrap();
    assert_eq!(
        (image.id.as_str(), image.mime.as_str()),
        (PIXEL, "image/png")
    );
    assert_eq!((image.width, image.height), (1, 1));
    let call = json!({"tool_calls": [{
        "id": "call_1", "name": "fixture-tools:echo", "arguments": {"text": "hello"},
        "display_name": "echo", "server": "fixture-tools"
    }]});
    assert_eq!(messages[2].metadata, Some(call));
    let AgentMessage::Assistant { content } = messages[2].to_agent_message() else {
        panic!()
    };
    assert_eq!(content.tool_calls[0].id, "call_1");
    assert_eq!(
        messages[3].metadata,
        Some(json!({"tool_call_id": "call_1"}))
    );
    let AgentMessage::Tool { tool_call_id, .. } = messages[3].to_agent_message() else {
        panic!()
    };
    assert_eq!(tool_call_id, "call_1");
    let made = json!({
        "modelName": "Fixture Agent 0.6B", "modelQuantization": "Q4_K_M", "promptTokens": 64,
        "cachedTokens": 32, "completionTokens": 12, "turnDurationMs": 700,
        "writingDurationMs": 500, "tokensPerSecond": 24.0, "finishReason": "stop",
    });
    assert_eq!(messages[4].metadata, Some(made));

    let copied = chats.get_messages(2).await.unwrap();
    let keys: Vec<_> = copied
        .iter()
        .map(gglib_core::domain::chat::Message::key)
        .collect();
    assert_eq!(keys, [2, 7]);
    assert_eq!(copied[0].images.len(), 1);
    let family = chats.lineage(2).await.unwrap();
    let rows: Vec<Vec<i64>> = family
        .iter()
        .map(|c| c.rows.iter().map(|r| r.key).collect())
        .collect();
    assert_eq!(rows, [vec![2, 3, 4, 5], vec![2, 7]]);

    let store = SqliteAttachmentStore::new(pool.clone());
    let id = AttachmentId::parse(PIXEL).unwrap();
    let blob = store.blob(&id).await.unwrap().expect("the picture is kept");
    assert_eq!(AttachmentId::of(&blob.data), id);
    pool.close().await;
}

/// All by hand: a benchmark needs a running model.
#[tokio::test]
async fn benchmarks_read_back_whole() {
    let (_dir, _db, pool) = open_fixture().await;
    let bench = SqliteBenchmarkRepository::new(pool.clone());

    let runs = bench.list_runs(10, 0).await.unwrap();
    let read: Vec<_> = runs
        .iter()
        .map(|r| {
            (
                r.id,
                r.run_type.clone(),
                r.status.clone(),
                r.model_ids.clone(),
            )
        })
        .collect();
    assert_eq!(
        read,
        [
            (5, Kind::Perf, Status::Failed, vec![2]),
            (4, Kind::Agentic, Status::Complete, vec![1]),
            (3, Kind::Tune, Status::Complete, vec![1]),
            (2, Kind::Compare, Status::Complete, vec![1, 2]),
            (1, Kind::Perf, Status::Complete, vec![1]),
        ]
    );
    assert_eq!(
        runs[0].error.as_deref(),
        Some("Process terminated unexpectedly")
    );
    let applied = r#"{"verdict":{"verdict":"incumbent_stands","incumbent_mean":0.71},"applied_config":null,"prior_defaults":null,"prior_origin":null}"#;
    assert_eq!(runs[2].applied_json.as_deref(), Some(applied));
    assert_eq!(runs[2].created_at, at("2026-11-12 01:44:00"));

    let [perf] = <[_; 1]>::try_from(bench.get_model_perf_history(1, 10).await.unwrap()).unwrap();
    assert_eq!(
        (perf.pp_tps, perf.tg_tps, perf.backend.as_deref()),
        (812.5, 52.5, Some("Metal"))
    );
    let [cut] = <[_; 1]>::try_from(bench.get_model_compare_history(2, 10).await.unwrap()).unwrap();
    assert_eq!(
        (cut.response_text.as_str(), cut.was_truncated),
        ("Hello there, how", true)
    );

    let [incumbent, preset] = <[_; 2]>::try_from(bench.get_tune_results(3).await.unwrap()).unwrap();
    assert!(matches!(incumbent.source, CandidateSource::Incumbent));
    assert_eq!(incumbent.config, InferenceConfig::default());
    assert!(incumbent.task_results[0].passed);
    assert!(matches!(&preset.source, CandidateSource::FamilyPreset { family } if family == "qwen"));
    let sampling = InferenceConfig {
        temperature: Some(0.6),
        top_k: Some(20),
        ..Default::default()
    };
    assert_eq!(preset.config, sampling);
    assert_eq!((preset.pruned, preset.tg_tps), (true, None));
    assert_eq!(preset.task_results[0].category, TaskCategory::SingleCall);

    let [report] =
        <[_; 1]>::try_from(bench.get_model_agentic_history(1, 10).await.unwrap()).unwrap();
    assert_eq!(report.model_name, "Fixture Agent 0.6B");
    assert_eq!((report.raw.composite, report.gglib.composite), (0.62, 0.81));
    assert_eq!(
        (report.tasks.len(), report.seeds.as_slice()),
        (1, [7].as_slice())
    );

    pool.close().await;
}

/// All by hand: the loop guard counts only on a live endpoint.
#[tokio::test]
async fn loop_guard_counts_read_back_whole() {
    let (_dir, _db, pool) = open_fixture().await;
    let days = SqliteLoopGuardTripLog::new(pool.clone())
        .summary(0)
        .await
        .unwrap();
    let read: Vec<_> = days
        .iter()
        .map(|d| {
            let counts = [d.scanned, d.trips, d.loops, d.stagnations, d.sessions];
            (
                d.day.as_str(),
                d.model_name.as_str(),
                d.gglib_version.as_str(),
                counts,
            )
        })
        .collect();
    let agent = (
        "2026-11-12",
        "Fixture Agent 0.6B",
        "0.33.0",
        [40, 2, 1, 1, 1],
    );
    let plain = ("2026-11-11", "Fixture Plain", "0.33.0", [7, 0, 0, 0, 0]);
    assert_eq!(read, [agent, plain]);
    pool.close().await;
}
