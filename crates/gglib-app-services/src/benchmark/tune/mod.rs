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
use gglib_core::ports::{LlmCompletionPort, RunningTarget, ToolExecutorPort, UsageSink};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;
use gglib_runtime::LlmCompletionAdapter;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub mod executor;
pub mod pruning;
pub mod scoring;
pub mod usage;

pub use executor::ScoringToolExecutorPort;
use pruning::select_survivors;
use scoring::score_outcome;
use usage::TaskUsageTally;

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

    // The lease is held for the whole sweep: a candidate measured across a
    // model swap would be measuring the swap.
    let admission = match deps
        .runtime
        .admit(
            &model.name,
            Some(resolved_ctx),
            resolved_ctx,
            gglib_core::ports::LaunchOverrides::default(),
        )
        .await
    {
        Ok(a) => a,
        Err(e) => {
            let msg = format!("failed to start model '{}': {e}", model.name);
            deps.bench_repo.fail_run(run_id, &msg).await.ok();
            let _ = tx.send(BenchmarkEvent::RunFailed { error: msg }).await;
            return Ok(());
        }
    };
    let target = &admission.target;

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
            let result = run_task(&deps.http_client, target, &model, candidate_config, task).await;
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
                    run_task(&deps.http_client, target, &model, &candidate_config, task).await;
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
        let tg_tps = throughput_tps(&task_results);
        let result = TuneCandidateResult {
            config: candidate_config,
            source,
            task_results,
            composite_score,
            pruned: !is_survivor,
            tg_tps,
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
    run_task_with_llm(
        |usage| {
            Arc::new(
                LlmCompletionAdapter::with_client(
                    target.base_url.clone(),
                    http_client.clone(),
                    Some(model.name.clone()),
                )
                .with_sampling(Some(candidate.clone()))
                .with_usage_sink(Some(usage)),
            )
        },
        task,
    )
    .await
}

/// The task-execution core shared by the tune sweep and the raw-vs-gglib
/// agentic eval: drive one task through the real `AgentLoop` against a
/// caller-built [`LlmCompletionPort`], and score the recorded calls.
///
/// The adapter configuration is the caller's whole degree of freedom — tune
/// varies sampling per candidate; the eval varies the entire pipeline per
/// arm — so the caller builds it rather than passing parameters.
///
/// It arrives as a *builder* rather than a finished port so this function can
/// own the [`TaskUsageTally`] it has to read afterwards: the sink is handed to
/// the adapter on the way in, which makes "forgot to wire the tally"
/// unrepresentable at the call site.
pub(crate) async fn run_task_with_llm<F>(build_llm: F, task: &TuneTask) -> TuneTaskResult
where
    F: FnOnce(Arc<dyn UsageSink>) -> Arc<dyn LlmCompletionPort>,
{
    let usage = TaskUsageTally::new();
    let llm = build_llm(usage.clone());
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

    let executor = ScoringToolExecutorPort::new(task.tools.clone());
    let call_log = executor.call_log_handle();
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(executor);
    let agent_loop = AgentLoop::build(llm, tool_executor, None);

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<AgentEvent>(64);
    let agent_config = AgentConfig::default();

    let started_at = Instant::now();
    let join_handle =
        tokio::spawn(async move { agent_loop.run(messages, agent_config, event_tx).await });

    // Both figures are recovered from the event stream rather than the run's
    // return value, so they survive a guard-aborted run.
    let mut iterations = 0usize;
    let mut time_to_first_tool_call_ms: Option<u64> = None;
    while let Some(event) = event_rx.recv().await {
        match event {
            AgentEvent::IterationComplete { iteration, .. } => iterations = iteration,
            AgentEvent::ToolCallStart { .. } if time_to_first_tool_call_ms.is_none() => {
                time_to_first_tool_call_ms = u64::try_from(started_at.elapsed().as_millis()).ok();
            }
            _ => {}
        }
    }
    let run_result = join_handle.await;
    let latency_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX);

    let recorded = call_log.lock().await.clone();
    let scoring = score_outcome(&task.expected, &recorded);

    // Read from the tally, not from `AgentRunOutput`: a guard-aborted run
    // returns `Err` and its tokens would otherwise vanish — and those are the
    // runs whose cost matters most.
    let completion_tokens = usage.total_completion_tokens();
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
        completion_tokens,
        time_to_first_tool_call_ms,
        detail,
    }
}

/// Completion-token throughput across a result set: total completion tokens
/// over total wall time. `None` when no task reported usage. Wall time
/// includes prompt pre-fill, so this is a consistent within-run comparison
/// figure rather than a pure decode rate — see
/// [`TuneCandidateResult::tg_tps`].
///
/// Tasks the guards aborted count towards both sides of the ratio: their
/// tokens survive the abort, so a result set that loops on half its tasks is
/// no longer measured only on the half that behaved.
pub(crate) fn throughput_tps(results: &[TuneTaskResult]) -> Option<f64> {
    let tokens: u64 = results.iter().filter_map(|r| r.completion_tokens).sum();
    let millis: u64 = results
        .iter()
        .filter(|r| r.completion_tokens.is_some())
        .map(|r| r.latency_ms)
        .sum();
    if tokens == 0 || millis == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    Some(tokens as f64 / (millis as f64 / 1000.0))
}

/// Combine per-task results into one composite score using [`ScoreWeights`].
///
/// The `speed` weight is still excluded from the denominator: `tg_tps` is
/// now *measured* per candidate ([`TuneCandidateResult::tg_tps`]), but its
/// scoring definition is relative-to-fastest-in-run, which cannot be
/// computed while candidates stream out one at a time. The three
/// self-contained components (tool accuracy, loop avoidance, task
/// completion) are renormalized to sum to `1.0` of the available weight.
///
/// An unmeasured loop-avoidance axis ([`AxisScores::loop_avoidance`] of
/// `None`) drops out of both the numerator and the denominator rather than
/// scoring `0.0` or an imputed `1.0`. Two result sets in one sweep can
/// therefore be scored over different denominators — but the alternative,
/// imputing a perfect score, systematically rewards candidates that never
/// engage the guard at all, which is the defect this avoids. Ranking is
/// unaffected wherever candidates share an eligibility count, which includes
/// the pre-screen round by construction: it runs one `SingleCall` and one
/// `Irrelevance` task, neither of which is ever loop-eligible.
pub(crate) fn compute_composite_score(results: &[TuneTaskResult], weights: &ScoreWeights) -> f64 {
    let Some(axes) = axis_scores(results) else {
        return 0.0;
    };

    // Unmeasured axes contribute no score and claim no weight.
    let (loop_term, loop_weight) = axes.loop_avoidance.map_or((0.0, 0.0), |avoidance| {
        (
            avoidance * f64::from(weights.loop_avoidance),
            f64::from(weights.loop_avoidance),
        )
    });

    let weight_sum =
        f64::from(weights.tool_accuracy) + loop_weight + f64::from(weights.task_completion);
    if weight_sum <= 0.0 {
        return 0.0;
    }

    (axes.tool_accuracy * f64::from(weights.tool_accuracy)
        + loop_term
        + axes.task_completion * f64::from(weights.task_completion))
        / weight_sum
}

/// Completed tool-executing iterations a task needs before the loop guard
/// could possibly have fired.
///
/// `LoopDetector` compares tool-call batch signatures, so two batches must
/// exist before a repeat is even representable. With `AgentConfig`'s default
/// `max_repeated_batch_steps`, this is also the iteration count a guard-aborted
/// task reports — the aborting turn never completes, so it is never counted.
///
/// [`run_task_with_llm`] hardcodes `AgentConfig::default()`, which is what
/// makes this knowable here. Should the harness ever take a caller-supplied
/// config, this must be derived from its `max_repeated_batch_steps` instead.
const MIN_ITERATIONS_FOR_LOOP_RISK: usize = 2;

/// The measured axes of a result set, each `0.0`–`1.0`.
pub(crate) struct AxisScores {
    /// Mean AST-style tool-call match score.
    pub tool_accuracy: f64,
    /// Fraction of *loop-eligible* tasks that triggered neither the loop nor
    /// the stagnation guard.
    ///
    /// `None` when no task was eligible: the axis was not measured, which is
    /// deliberately distinct from a perfect `1.0`. Scoring an arm that never
    /// reached a second tool batch as having "avoided" a loop is what let a
    /// bare llama-server arm — one generating 32,500 tokens per task until it
    /// hit its cap — outscore the pipeline it was being compared against.
    pub loop_avoidance: Option<f64>,
    /// How many tasks were loop-eligible: the denominator behind
    /// `loop_avoidance`, and the sample size a reader needs to judge it.
    pub loop_eligible: usize,
    /// Fraction of tasks that passed outright.
    pub task_completion: f64,
}

/// Whether this task gave the guards a chance to fire.
///
/// A task that finished before a second tool batch existed cannot have looped,
/// so counting it as "avoided a loop" measures nothing. A task the guards did
/// abort is eligible by definition, whatever its iteration count.
///
/// Stagnation is folded in rather than given its own lower threshold. It
/// compares response *text* and so can fire a turn earlier than the loop
/// detector, which means a one-batch task is scored ineligible here even
/// though it could in principle have stagnated. That costs only a
/// "trivially didn't stagnate" credit and produces no false negatives — a task
/// that actually stagnated is eligible via `stagnation_detected` — whereas
/// crediting those tasks re-imports exactly the vacuity this removes.
fn is_loop_eligible(result: &TuneTaskResult) -> bool {
    result.loop_detected
        || result.stagnation_detected
        || result.iterations >= MIN_ITERATIONS_FOR_LOOP_RISK
}

/// Compute the per-axis scores for a result set. `None` when empty.
pub(crate) fn axis_scores(results: &[TuneTaskResult]) -> Option<AxisScores> {
    if results.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let n = results.len() as f64;
    let tool_accuracy = results.iter().map(|r| r.tool_match_score).sum::<f64>() / n;
    let loop_eligible = results.iter().filter(|r| is_loop_eligible(r)).count();
    let loop_free = results
        .iter()
        .filter(|r| is_loop_eligible(r) && !r.loop_detected && !r.stagnation_detected)
        .count();
    #[allow(clippy::cast_precision_loss)]
    let loop_avoidance = (loop_eligible > 0).then(|| loop_free as f64 / loop_eligible as f64);
    let passed = results.iter().filter(|r| r.passed).count();
    #[allow(clippy::cast_precision_loss)]
    let task_completion = passed as f64 / n;

    Some(AxisScores {
        tool_accuracy,
        loop_avoidance,
        loop_eligible,
        task_completion,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A loop-eligible result by default (`iterations` at the threshold), so
    /// scoring fixtures exercise the loop axis rather than skipping it.
    /// Tests that need an ineligible task lower `iterations` explicitly.
    fn task_result(tool_match_score: f64, passed: bool, loop_detected: bool) -> TuneTaskResult {
        TuneTaskResult {
            task_id: "t".to_string(),
            category: TaskCategory::SingleCall,
            passed,
            tool_match_score,
            loop_detected,
            stagnation_detected: false,
            iterations: MIN_ITERATIONS_FOR_LOOP_RISK,
            latency_ms: 10,
            completion_tokens: None,
            time_to_first_tool_call_ms: None,
            detail: None,
        }
    }

    /// The defect this axis was rebuilt around: a task that finished before a
    /// second tool batch existed cannot have looped, so counting it as
    /// "avoided a loop" inflates the score with tasks that never took the
    /// risk. The old code scored this set `2/3 = 0.667`.
    #[test]
    fn loop_avoidance_ignores_tasks_that_could_not_loop() {
        let mut answered_directly = task_result(1.0, true, false);
        answered_directly.iterations = 0;
        let mut one_batch_then_answer = task_result(1.0, true, false);
        one_batch_then_answer.iterations = 1;
        let looped = task_result(1.0, false, true);

        let axes = axis_scores(&[answered_directly, one_batch_then_answer, looped]).unwrap();
        assert_eq!(axes.loop_eligible, 1, "only the looping task risked a loop");
        assert_eq!(axes.loop_avoidance, Some(0.0));
    }

    /// The regression test for the reported artifact: a bare llama-server arm
    /// that generated to its token cap on every task, took one batch, and
    /// never iterated again scored a perfect 1.000 on an axis it had never
    /// been measured against.
    #[test]
    fn a_suite_that_never_risked_a_loop_reports_no_loop_avoidance() {
        let results: Vec<_> = (0..9)
            .map(|_| {
                let mut r = task_result(0.722, true, false);
                r.iterations = 1;
                r
            })
            .collect();

        let axes = axis_scores(&results).unwrap();
        assert_eq!(axes.loop_eligible, 0);
        assert!(
            axes.loop_avoidance.is_none(),
            "unmeasured must not read as perfect"
        );
    }

    /// Eligibility comes from the guard firing, not from the iteration count:
    /// stagnation can abort a run before any tool batch completes.
    #[test]
    fn a_guard_that_fired_is_always_eligible() {
        let mut stagnated = task_result(0.0, false, false);
        stagnated.stagnation_detected = true;
        stagnated.iterations = 0;

        let axes = axis_scores(&[stagnated]).unwrap();
        assert_eq!(axes.loop_eligible, 1);
        assert_eq!(axes.loop_avoidance, Some(0.0));
    }

    /// An unmeasured axis must claim no weight rather than scoring zero —
    /// otherwise a suite that never risked a loop is punished for it.
    #[test]
    fn composite_renormalizes_when_loop_avoidance_is_unmeasured() {
        let mut perfect = task_result(1.0, true, false);
        perfect.iterations = 1;

        let got = compute_composite_score(&[perfect], &ScoreWeights::default());
        assert!(
            (got - 1.0).abs() < 1e-9,
            "expected the loop weight to be redistributed, got {got}"
        );
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

    /// Total tokens over total wall time — and tasks that reported no usage
    /// contribute neither tokens nor time, so they cannot dilute the figure.
    #[test]
    fn throughput_is_tokens_over_wall_time() {
        let mut measured = task_result(1.0, true, false);
        measured.completion_tokens = Some(100);
        measured.latency_ms = 2_000;
        let mut unmeasured = task_result(1.0, true, false);
        unmeasured.completion_tokens = None;
        unmeasured.latency_ms = 5_000;

        let tps = throughput_tps(&[measured, unmeasured]).unwrap();
        assert!((tps - 50.0).abs() < 1e-9);
        assert_eq!(throughput_tps(&[task_result(1.0, true, false)]), None);
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
            dialect_spec: None,
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
            defaults_origin: None,
            server_defaults: None,
            capabilities: gglib_core::domain::capabilities::ModelCapabilities::default(),
            benchmark_summary: None,
        }
    }
}
