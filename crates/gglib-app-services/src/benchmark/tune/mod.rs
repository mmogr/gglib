#![doc = include_str!("README.md")]
//!
//! See the module README for the full design rationale (why no MCP
//! dependency, why no per-candidate model reload).

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_core::domain::benchmark::tune::config::{ScoreWeights, SweepSpec, TuneConfig};
use gglib_core::domain::benchmark::tune::result::{
    CandidateSource, TuneCandidateResult, TuneTaskResult,
};
use gglib_core::domain::benchmark::tune::task::{TaskCategory, TuneTask};
use gglib_core::domain::benchmark::{BenchmarkEvent, BenchmarkRunType};
use gglib_core::domain::{InferenceConfig, Model};
use gglib_core::ports::{LlmCompletionPort, RunningTarget, ToolExecutorPort};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;
use gglib_runtime::LlmCompletionAdapter;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub mod executor;
pub mod pruning;
pub mod scoring;

pub use executor::ScoringToolExecutorPort;
use pruning::select_survivors;
use scoring::score_outcome;

use super::BenchmarkDeps;

/// Entry point called by [`super::BenchmarkOps::run_tune`].
pub async fn run_tune(
    deps: &BenchmarkDeps,
    config: TuneConfig,
    tx: Sender<BenchmarkEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    let tasks = config
        .task_suite
        .resolve()
        .context("failed to resolve tune task suite")?;
    anyhow::ensure!(!tasks.is_empty(), "tune task suite must not be empty");

    let model = deps
        .model_repo
        .get_by_id(config.model_id)
        .await
        .with_context(|| format!("model {} not found", config.model_id))?;

    let config_json = serde_json::to_string(&config).ok();
    let run_id = deps
        .bench_repo
        .create_run(
            BenchmarkRunType::Tune,
            &[config.model_id],
            None,
            None,
            config_json.as_deref(),
        )
        .await
        .context("failed to create tune run record")?;

    let candidates = build_candidates(&config.sweep, &model, &config);

    // ── Load the model once — every candidate only varies per-request
    // sampling parameters, never the loaded llama-server process. ──────────
    let settings = deps.settings_repo.load().await.ok();
    let default_ctx = settings
        .as_ref()
        .and_then(|s| s.default_context_size)
        .unwrap_or(DEFAULT_CONTEXT_SIZE);
    let resolved_ctx = resolve_context_size(&ServerConfigOptions {
        context_size: config.ctx_size,
        model_server_ctx: model
            .server_defaults
            .as_ref()
            .and_then(|s| s.context_length),
        global_default_ctx: Some(default_ctx),
        ..Default::default()
    });

    let target = match deps
        .runtime
        .ensure_model_running(&model.name, Some(resolved_ctx), resolved_ctx)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            let msg = format!("failed to start model '{}': {e}", model.name);
            deps.bench_repo.fail_run(run_id, &msg).await.ok();
            let _ = tx.send(BenchmarkEvent::RunFailed { error: msg }).await;
            return Ok(());
        }
    };

    // ── Pre-screen round: one SingleCall + one Irrelevance task (cheapest,
    // most diagnostic pair) if the suite has them; otherwise the first two
    // tasks. ────────────────────────────────────────────────────────────────
    let prescreen_tasks = select_prescreen_tasks(&tasks);
    let remaining_tasks: Vec<&TuneTask> = tasks
        .iter()
        .filter(|t| !prescreen_tasks.iter().any(|p| p.id == t.id))
        .collect();

    let total = candidates.len();
    let mut prescreen_results: Vec<Vec<TuneTaskResult>> = Vec::with_capacity(total);

    for (idx, (candidate_config, _)) in candidates.iter().enumerate() {
        if cancel.is_cancelled() {
            deps.bench_repo
                .fail_run(run_id, "Aborted by user")
                .await
                .ok();
            deps.runtime.stop_current().await.ok();
            return Ok(());
        }

        let _ = tx
            .send(BenchmarkEvent::TuneCandidateStarted {
                candidate_index: idx,
                total,
            })
            .await;

        let mut results = Vec::with_capacity(prescreen_tasks.len());
        for task in &prescreen_tasks {
            let result = run_task(&deps.http_client, &target, &model, candidate_config, task).await;
            let _ = tx
                .send(BenchmarkEvent::TuneTaskComplete {
                    candidate_index: idx,
                    task_id: task.id.clone(),
                    passed: result.passed,
                })
                .await;
            results.push(result);
        }
        prescreen_results.push(results);
    }

    let prescreen_scores: Vec<f64> = prescreen_results
        .iter()
        .map(|results| compute_composite_score(results, &config.weights))
        .collect();
    let survivors = select_survivors(&prescreen_scores, config.prune_fraction);

    for (idx, (candidate_config, source)) in candidates.into_iter().enumerate() {
        if cancel.is_cancelled() {
            deps.bench_repo
                .fail_run(run_id, "Aborted by user")
                .await
                .ok();
            deps.runtime.stop_current().await.ok();
            return Ok(());
        }

        let is_survivor = survivors.contains(&idx);
        let mut task_results = std::mem::take(&mut prescreen_results[idx]);

        if is_survivor {
            for task in &remaining_tasks {
                let result =
                    run_task(&deps.http_client, &target, &model, &candidate_config, task).await;
                let _ = tx
                    .send(BenchmarkEvent::TuneTaskComplete {
                        candidate_index: idx,
                        task_id: task.id.clone(),
                        passed: result.passed,
                    })
                    .await;
                task_results.push(result);
            }
        } else {
            let _ = tx
                .send(BenchmarkEvent::TunePruned {
                    candidate_index: idx,
                    reason: format!(
                        "pre-screen score {:.3} did not survive successive-halving",
                        prescreen_scores[idx]
                    ),
                })
                .await;
        }

        let composite_score = compute_composite_score(&task_results, &config.weights);
        let result = TuneCandidateResult {
            config: candidate_config,
            source,
            task_results,
            composite_score,
            pruned: !is_survivor,
            tg_tps: None,
        };

        if let Err(e) = deps
            .bench_repo
            .save_tune_result(&result, run_id, config.model_id)
            .await
        {
            warn!("benchmark: failed to save tune result for candidate {idx}: {e}");
        }
        let _ = tx
            .send(BenchmarkEvent::TuneCandidateComplete { result })
            .await;
    }

    if let Err(e) = deps.bench_repo.complete_run(run_id).await {
        warn!("benchmark: failed to complete tune run {run_id}: {e}");
    }
    let _ = tx.send(BenchmarkEvent::RunComplete { run_id }).await;
    Ok(())
}

/// Pick the pre-screen task pair: the first `SingleCall` task and the first
/// `Irrelevance` task, falling back to the first two tasks in the suite if
/// either category is absent (never empty — `run_tune` already checked the
/// suite is non-empty).
fn select_prescreen_tasks(tasks: &[TuneTask]) -> Vec<TuneTask> {
    let single_call = tasks
        .iter()
        .find(|t| t.category == TaskCategory::SingleCall);
    let irrelevance = tasks
        .iter()
        .find(|t| t.category == TaskCategory::Irrelevance);

    match (single_call, irrelevance) {
        (Some(a), Some(b)) => vec![a.clone(), b.clone()],
        _ => tasks.iter().take(2).cloned().collect(),
    }
}

/// Build the full candidate list: the user's [`SweepSpec`] grid, plus
/// optional seeded candidates from GGUF author defaults and per-family
/// presets.
fn build_candidates(
    sweep: &SweepSpec,
    model: &Model,
    config: &TuneConfig,
) -> Vec<(InferenceConfig, CandidateSource)> {
    let mut candidates: Vec<(InferenceConfig, CandidateSource)> = build_candidate_grid(sweep)
        .into_iter()
        .map(|c| (c, CandidateSource::UserGrid))
        .collect();

    if config.seed_from_gguf
        && let Some(gguf_default) = gguf_author_default(model)
    {
        candidates.push((gguf_default, CandidateSource::GgufAuthorDefault));
    }

    if config.seed_from_family_presets {
        for (family, preset) in family_presets(model) {
            candidates.push((preset, CandidateSource::FamilyPreset { family }));
        }
    }

    candidates
}

/// Cartesian product of every non-empty [`SweepSpec`] dimension. A dimension
/// left empty contributes a single `None` value (don't vary it — the normal
/// inference-config resolution chain fills it in downstream).
fn build_candidate_grid(sweep: &SweepSpec) -> Vec<InferenceConfig> {
    let temps = sweep_dimension(&sweep.temperature);
    let top_ps = sweep_dimension(&sweep.top_p);
    let top_ks = sweep_dimension(&sweep.top_k);
    let min_ps = sweep_dimension(&sweep.min_p);
    let repeat_penalties = sweep_dimension(&sweep.repeat_penalty);

    let mut grid = Vec::new();
    for &temperature in &temps {
        for &top_p in &top_ps {
            for &top_k in &top_ks {
                for &min_p in &min_ps {
                    for &repeat_penalty in &repeat_penalties {
                        grid.push(InferenceConfig {
                            temperature,
                            top_p,
                            top_k,
                            min_p,
                            repeat_penalty,
                            max_tokens: None,
                            presence_penalty: None,
                        });
                    }
                }
            }
        }
    }
    grid
}

/// Convert a sweep dimension's candidate-value list into `Option<T>` values:
/// empty means "don't vary this dimension" (a single `None`).
fn sweep_dimension<T: Copy>(values: &[T]) -> Vec<Option<T>> {
    if values.is_empty() {
        vec![None]
    } else {
        values.iter().map(|v| Some(*v)).collect()
    }
}

/// GGUF author-recommended sampling defaults, when the model's metadata
/// carries them.
///
/// Always returns `None` today — no GGUF metadata convention for
/// author-recommended sampling defaults exists yet (see
/// <https://github.com/ggml-org/llama.cpp/discussions/17088>). This is a
/// forward-compatible extension point: once `gglib-gguf` can parse such
/// metadata, this function becomes the single place to wire it in.
fn gguf_author_default(_model: &Model) -> Option<InferenceConfig> {
    None
}

/// Built-in per-model-family sampling presets, keyed by a case-insensitive
/// substring match against the model's name.
///
/// Deliberately small: community consensus (as of this writing) documents
/// good coding/tool-use defaults for very few families. Extend this table as
/// more presets are validated, rather than guessing.
fn family_presets(model: &Model) -> Vec<(String, InferenceConfig)> {
    let name = model.name.to_lowercase();
    let mut presets = Vec::new();

    if name.contains("qwen") {
        presets.push((
            "qwen-coding".to_string(),
            InferenceConfig {
                temperature: Some(0.6),
                top_p: Some(0.95),
                top_k: Some(20),
                min_p: Some(0.0),
                repeat_penalty: None,
                max_tokens: None,
                presence_penalty: None,
            },
        ));
    }

    presets
}

/// Run one task against one candidate's sampling settings through the real
/// `AgentLoop`, scoring the recorded tool calls against the task's expected
/// outcome.
async fn run_task(
    http_client: &reqwest::Client,
    target: &RunningTarget,
    model: &Model,
    candidate: &InferenceConfig,
    task: &TuneTask,
) -> TuneTaskResult {
    let mut messages: Vec<AgentMessage> = task.history.clone().unwrap_or_default();
    if let Some(system_prompt) = &task.system_prompt {
        messages.insert(
            0,
            AgentMessage::System {
                content: system_prompt.clone(),
            },
        );
    }
    messages.push(AgentMessage::User {
        content: task.user_prompt.clone(),
    });

    let llm: Arc<dyn LlmCompletionPort> = Arc::new(
        LlmCompletionAdapter::with_client(
            target.base_url.clone(),
            http_client.clone(),
            Some(model.name.clone()),
        )
        .with_sampling(Some(candidate.clone())),
    );
    let executor = ScoringToolExecutorPort::new(task.tools.clone());
    let call_log = executor.call_log_handle();
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(executor);
    let agent_loop = AgentLoop::build(llm, tool_executor, None);

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(64);
    let agent_config = AgentConfig::default();

    let started_at = Instant::now();
    let join_handle =
        tokio::spawn(async move { agent_loop.run(messages, agent_config, event_tx).await });

    let mut iterations = 0usize;
    while let Some(event) = event_rx.recv().await {
        if let AgentEvent::IterationComplete { iteration, .. } = event {
            iterations = iteration;
        }
    }
    let run_result = join_handle.await;
    let latency_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);

    let recorded = call_log.lock().await.clone();
    let scoring = score_outcome(&task.expected, &recorded);

    let (loop_detected, stagnation_detected, error_detail) = match &run_result {
        Ok(Ok(_)) => (false, false, None),
        Ok(Err(gglib_core::ports::AgentError::LoopDetected { .. })) => (true, false, None),
        Ok(Err(gglib_core::ports::AgentError::StagnationDetected { .. })) => (false, true, None),
        Ok(Err(e)) => (false, false, Some(e.to_string())),
        Err(join_err) => (
            false,
            false,
            Some(format!("agent task panicked: {join_err}")),
        ),
    };

    let detail = match (scoring.detail, error_detail) {
        (Some(s), Some(e)) => Some(format!("{s}; {e}")),
        (Some(s), None) => Some(s),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    };

    TuneTaskResult {
        task_id: task.id.clone(),
        category: task.category,
        passed: scoring.passed,
        tool_match_score: scoring.tool_match_score,
        loop_detected,
        stagnation_detected,
        iterations,
        latency_ms,
        detail,
    }
}

/// Combine per-task results into one composite score using [`ScoreWeights`].
///
/// The `speed` weight is currently excluded from the denominator (no
/// per-candidate `tg_tps` measurement exists yet — see
/// [`TuneCandidateResult::tg_tps`]) so the three measurable components
/// (tool accuracy, loop avoidance, task completion) are renormalized to
/// sum to `1.0` of the available weight.
fn compute_composite_score(results: &[TuneTaskResult], weights: &ScoreWeights) -> f64 {
    if results.is_empty() {
        return 0.0;
    }

    #[allow(clippy::cast_precision_loss)]
    let n = results.len() as f64;
    let tool_accuracy = results.iter().map(|r| r.tool_match_score).sum::<f64>() / n;
    let loop_free = results
        .iter()
        .filter(|r| !r.loop_detected && !r.stagnation_detected)
        .count();
    #[allow(clippy::cast_precision_loss)]
    let loop_avoidance = loop_free as f64 / n;
    let passed = results.iter().filter(|r| r.passed).count();
    #[allow(clippy::cast_precision_loss)]
    let task_completion = passed as f64 / n;

    let weight_sum = f64::from(weights.tool_accuracy)
        + f64::from(weights.loop_avoidance)
        + f64::from(weights.task_completion);
    if weight_sum <= 0.0 {
        return 0.0;
    }

    (tool_accuracy * f64::from(weights.tool_accuracy)
        + loop_avoidance * f64::from(weights.loop_avoidance)
        + task_completion * f64::from(weights.task_completion))
        / weight_sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_result(tool_match_score: f64, passed: bool, loop_detected: bool) -> TuneTaskResult {
        TuneTaskResult {
            task_id: "t".to_string(),
            category: TaskCategory::SingleCall,
            passed,
            tool_match_score,
            loop_detected,
            stagnation_detected: false,
            iterations: 1,
            latency_ms: 10,
            detail: None,
        }
    }

    #[test]
    fn build_candidate_grid_is_cartesian_product() {
        let sweep = SweepSpec {
            temperature: vec![0.2, 0.8],
            top_p: vec![0.9],
            top_k: vec![],
            min_p: vec![],
            repeat_penalty: vec![],
        };
        let grid = build_candidate_grid(&sweep);
        assert_eq!(grid.len(), 2);
        assert!(grid.iter().any(|c| c.temperature == Some(0.2)));
        assert!(grid.iter().any(|c| c.temperature == Some(0.8)));
        assert!(grid.iter().all(|c| c.top_p == Some(0.9)));
        assert!(grid.iter().all(|c| c.top_k.is_none()));
    }

    #[test]
    fn empty_sweep_produces_one_all_none_candidate() {
        let grid = build_candidate_grid(&SweepSpec::default());
        assert_eq!(grid.len(), 1);
        assert_eq!(grid[0].temperature, None);
    }

    #[test]
    fn composite_score_rewards_accuracy_and_loop_avoidance() {
        let weights = ScoreWeights::default();
        let good = [task_result(1.0, true, false), task_result(1.0, true, false)];
        let bad = [task_result(0.0, false, true), task_result(0.0, false, true)];
        assert!(compute_composite_score(&good, &weights) > compute_composite_score(&bad, &weights));
    }

    #[test]
    fn composite_score_of_empty_results_is_zero() {
        assert_eq!(compute_composite_score(&[], &ScoreWeights::default()), 0.0);
    }

    #[test]
    fn qwen_family_preset_matches_case_insensitively() {
        let model = test_model("Qwen2.5-Coder-7B-Instruct");
        let presets = family_presets(&model);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].0, "qwen-coding");
    }

    #[test]
    fn unknown_family_has_no_presets() {
        let model = test_model("some-other-model");
        assert!(family_presets(&model).is_empty());
    }

    fn test_model(name: &str) -> Model {
        Model {
            id: 1,
            name: name.to_string(),
            model_key: String::new(),
            file_path: std::path::PathBuf::from("/tmp/model.gguf"),
            param_count_b: 7.0,
            architecture: None,
            quantization: None,
            context_length: None,
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: std::collections::HashMap::new(),
            added_at: chrono::Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: vec![],
            inference_defaults: None,
            server_defaults: None,
            capabilities: gglib_core::domain::capabilities::ModelCapabilities::default(),
            benchmark_summary: None,
        }
    }
}
