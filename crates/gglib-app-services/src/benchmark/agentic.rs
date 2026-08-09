//! Raw-vs-gglib A/B agentic evaluation.
//!
//! Runs the tune sweep's task suite twice against the same loaded model —
//! once with the request pipeline bypassed (the *raw* arm: what a client
//! pointed straight at llama-server gets) and once through the full gglib
//! pipeline (the *gglib* arm) — and reports per-axis scores and their
//! difference. See [`gglib_core::domain::benchmark::agentic`] for the
//! report shape and the definition of each arm.
//!
//! One admission lease covers both arms, for the same reason the tune sweep
//! holds one across candidates: an arm measured across a model swap would
//! be measuring the swap. Tasks whose expected outcome demands a tool call
//! send `tool_choice: "required"` on their **opening turn** in both arms —
//! the same request an agentic client would make — which is what lets the
//! gglib arm's decode-time grammar stage engage while the raw arm shows what
//! that demand achieves without it. Later turns drop back to `"auto"`: a
//! model held at `"required"` for the whole run can never answer, so it
//! re-emits its last batch until the loop guard stops it, and the eval ends
//! up measuring its own harness.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::benchmark::agentic::{
    AgenticEvalConfig, AgenticEvalReport, AgenticTaskComparison, ArmScores,
    CONTROL_MIN_COMPOSITE_GAP, ControlVerdict, EvalArm, control_sampling,
};
use gglib_core::domain::benchmark::tune::result::TuneTaskResult;
use gglib_core::domain::benchmark::tune::task::{ExpectedOutcome, TuneTask};
use gglib_core::domain::benchmark::{BenchmarkEvent, BenchmarkRunType};
use gglib_core::ports::{LlmCompletionPort, UsageSink};
use gglib_core::request_pipeline::ModelContext;
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;
use gglib_runtime::LlmCompletionAdapter;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::BenchmarkDeps;
use super::tune::{axis_scores, run_task_with_llm, throughput_tps};

/// Entry point called by [`super::BenchmarkOps::run_agentic`].
pub async fn run_agentic_eval(
    deps: &BenchmarkDeps,
    config: AgenticEvalConfig,
    tx: Sender<BenchmarkEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    let tasks = config
        .task_suite
        .resolve()
        .context("failed to resolve agentic eval task suite")?;
    anyhow::ensure!(
        !tasks.is_empty(),
        "agentic eval task suite must not be empty"
    );

    let model = deps
        .model_repo
        .get_by_id(config.model_id)
        .await
        .with_context(|| format!("model {} not found", config.model_id))?;

    let config_json = serde_json::to_string(&config).ok();
    let run_id = deps
        .bench_repo
        .create_run(
            BenchmarkRunType::Agentic,
            &[config.model_id],
            None,
            None,
            config_json.as_deref(),
        )
        .await
        .context("failed to create agentic eval run record")?;

    // Resolve the serving context exactly as the tune sweep does.
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

    // One lease across both arms — an arm measured across a model swap would
    // be measuring the swap.
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
    let base_url = admission.target.base_url.clone();

    // The gglib arm's per-model facts, straight from the catalog row. Shared
    // with the tune sweep, which needs the identical context so a tuned value
    // means the same thing in both.
    let model_context = super::tune::model_context_for(&model);

    // One run per seed, per task. An empty seed list still runs once, with no
    // seed named — the pre-multi-seed behaviour, kept reachable as a fast
    // smoke test.
    let seeds: Vec<Option<u32>> = if config.seeds.is_empty() {
        vec![None]
    } else {
        config.seeds.iter().copied().map(Some).collect()
    };

    let mut arms = vec![EvalArm::Raw, EvalArm::Gglib];
    if config.include_control {
        arms.push(EvalArm::Control);
    }

    // Per arm, results are grouped by task and ordered by seed within each
    // group, so the per-task drill-down can report N-of-M without re-keying.
    let mut arm_results: Vec<Vec<Vec<TuneTaskResult>>> = Vec::with_capacity(arms.len());
    for arm in arms.iter().copied() {
        let _ = tx
            .send(BenchmarkEvent::AgenticArmStarted {
                arm,
                total_tasks: tasks.len() * seeds.len(),
            })
            .await;

        let mut per_task = Vec::with_capacity(tasks.len());
        for task in &tasks {
            let mut per_seed = Vec::with_capacity(seeds.len());
            for seed in seeds.iter().copied() {
                if cancel.is_cancelled() {
                    deps.bench_repo
                        .fail_run(run_id, "Aborted by user")
                        .await
                        .ok();
                    deps.runtime.stop_current().await.ok();
                    let _ = tx
                        .send(BenchmarkEvent::RunFailed {
                            error: "Aborted by user".into(),
                        })
                        .await;
                    return Ok(());
                }

                let result = run_task_with_llm(
                    |usage| {
                        build_arm_llm(
                            deps,
                            &base_url,
                            &model.name,
                            arm,
                            &model_context,
                            task,
                            seed,
                            usage,
                        )
                    },
                    task,
                )
                .await;
                let _ = tx
                    .send(BenchmarkEvent::AgenticTaskComplete {
                        arm,
                        task_id: task.id.clone(),
                        passed: result.passed,
                    })
                    .await;
                per_seed.push(result);
            }
            per_task.push(per_seed);
        }
        arm_results.push(per_task);
    }

    // Popped in reverse push order.
    let control_results = config
        .include_control
        .then(|| arm_results.pop().unwrap_or_default());
    let gglib_results = arm_results.pop().unwrap_or_default();
    let raw_results = arm_results.pop().unwrap_or_default();

    let raw = arm_scores(&flatten(&raw_results), &config, seeds.len(), tasks.len());
    let gglib = arm_scores(&flatten(&gglib_results), &config, seeds.len(), tasks.len());
    let control = control_results
        .as_ref()
        .map(|r| arm_scores(&flatten(r), &config, seeds.len(), tasks.len()));
    let delta = AgenticEvalReport::delta_of(&raw, &gglib);

    let tasks_cmp: Vec<AgenticTaskComparison> = raw_results
        .into_iter()
        .zip(gglib_results)
        .filter_map(|(raw, gglib)| {
            let first = raw.first().or_else(|| gglib.first())?;
            Some(AgenticTaskComparison {
                task_id: first.task_id.clone(),
                category: first.category,
                raw,
                gglib,
            })
        })
        .collect();

    let report = AgenticEvalReport {
        model_name: model.name.clone(),
        quantization: model.quantization.clone(),
        param_count_b: model.param_count_b,
        ctx_size: resolved_ctx,
        raw,
        gglib,
        delta,
        tasks: tasks_cmp,
        seeds: config.seeds.clone(),
        control,
    };

    // Said out loud, because a control that failed to move invalidates every
    // other number in the report and a reader scanning the deltas will not
    // otherwise notice.
    if let Some(verdict) = report.control_verdict()
        && !verdict.demonstrated_sensitivity()
    {
        warn_control(verdict, &report);
    }

    if let Err(e) = deps
        .bench_repo
        .save_agentic_result(&report, run_id, config.model_id)
        .await
    {
        warn!("failed to save agentic eval result: {e}");
    }
    if let Err(e) = deps.bench_repo.complete_run(run_id).await {
        warn!("failed to mark agentic eval run complete: {e}");
    }

    let _ = tx
        .send(BenchmarkEvent::AgenticEvalComplete {
            report: Box::new(report),
        })
        .await;
    Ok(())
}

/// Build the LLM port for one arm and one task.
///
/// The arm is the *entire* difference between the two runs:
///
/// - **Raw** bypasses the pipeline (`with_raw_passthrough`) and carries a
///   passthrough model context, so no shaping, no per-model sampling, no
///   dialect parsing and no grammar happen — llama-server's own defaults
///   and wire format, verbatim.
/// - **Gglib** carries the catalog-resolved [`ModelContext`], which switches
///   on exactly what the proxy would do for this model.
///
/// Both arms send `tool_choice: "required"` on the opening turn when the
/// task's expected outcome demands a call — identical requests, different
/// machinery — and both fall back to `"auto"` afterwards so the model can
/// finish.
#[allow(clippy::too_many_arguments)]
fn build_arm_llm(
    deps: &BenchmarkDeps,
    base_url: &str,
    model_name: &str,
    arm: EvalArm,
    model_context: &ModelContext,
    task: &TuneTask,
    seed: Option<u32>,
    usage: Arc<dyn UsageSink>,
) -> Arc<dyn LlmCompletionPort> {
    let tool_choice = demands_tool_call(task).then(|| "required".to_owned());

    // The seed is the only sampling value the raw arm carries, and carrying it
    // does not compromise the arm: `build_chat_body` writes the caller's
    // sampling before `raw_passthrough` returns, and a config naming nothing
    // but a seed adds no sampler policy to the body. So the control stays bare
    // — llama-server's own defaults — and becomes reproducible.
    // The control forces every truncation sampler off as well as the
    // temperature. Temperature alone does not degrade: llama.cpp runs
    // truncation first, so `top_k: 20` from a reasoning recipe leaves a
    // temperature of 2.0 reshaping only twenty surviving tokens. Measured —
    // that control scored *above* both real arms. See `control_sampling`.
    let sampling = match arm {
        EvalArm::Control => {
            let (temperature, top_k, top_p, min_p) = control_sampling();
            InferenceConfig {
                seed,
                temperature: Some(temperature),
                top_k: Some(top_k),
                top_p: Some(top_p),
                min_p: Some(min_p),
                ..InferenceConfig::default()
            }
        }
        EvalArm::Raw | EvalArm::Gglib => InferenceConfig {
            seed,
            ..InferenceConfig::default()
        },
    };

    let adapter = LlmCompletionAdapter::with_client(
        base_url.to_owned(),
        deps.http_client.clone(),
        Some(model_name.to_owned()),
    )
    .with_first_turn_tool_choice(tool_choice)
    .with_sampling(Some(sampling))
    .with_usage_sink(Some(usage));

    let adapter = match arm {
        EvalArm::Raw => adapter.with_raw_passthrough(true),
        // The control runs the same pipeline as the gglib arm, so the gap
        // between them is attributable to sampling rather than to shaping. It
        // is not a one-variable ablation — see `control_sampling` — because its
        // job is to be large and known-bad, not minimal.
        EvalArm::Gglib | EvalArm::Control => adapter.with_model_context(model_context.clone()),
    };
    Arc::new(adapter)
}

/// Whether a task's expected outcome demands at least one tool call.
fn demands_tool_call(task: &TuneTask) -> bool {
    matches!(&task.expected, ExpectedOutcome::ToolCalls { calls } if !calls.is_empty())
}

/// Say why the control failed, in terms that name the fix.
///
/// The two failures want different actions and must not share wording. An
/// earlier version reported both as "changed by only {gap}", which described a
/// control that had moved 0.090 in the wrong direction as though it had barely
/// moved at all.
fn warn_control(verdict: ControlVerdict, report: &AgenticEvalReport) {
    let control = report.control.as_ref().map_or(f64::NAN, |c| c.composite);
    match verdict {
        ControlVerdict::Moved { .. } => {}
        ControlVerdict::TooSmall { gap } => warn!(
            "agentic eval: the positive control moved only {gap:.3} (gglib {:.3} vs control \
             {control:.3}), under the {CONTROL_MIN_COMPOSITE_GAP:.2} needed to demonstrate \
             sensitivity. This run cannot distinguish 'no effect' from 'no sensitivity' — treat \
             every delta as uninterpretable.",
            report.gglib.composite
        ),
        ControlVerdict::WrongDirection { gap } => warn!(
            "agentic eval: the positive control scored {gap:.3} ABOVE the gglib arm (gglib \
             {:.3} vs control {control:.3}). The control's sampling was chosen to be bad, so this \
             contradicts its premise rather than merely failing a threshold — the control needs \
             fixing before any delta in this report means anything.",
            report.gglib.composite
        ),
    }
}

/// Flatten per-task, per-seed results into one list.
///
/// Every mean below is taken over the flat list rather than over per-task
/// means. With a balanced design — and this one is balanced by construction,
/// every task running every seed — the two are arithmetically identical, and
/// the flat form keeps one code path shared with the single-seed sweep.
fn flatten(per_task: &[Vec<TuneTaskResult>]) -> Vec<TuneTaskResult> {
    per_task.iter().flatten().cloned().collect()
}

/// Aggregate one arm's task results into [`ArmScores`].
fn arm_scores(
    results: &[TuneTaskResult],
    config: &AgenticEvalConfig,
    seeds: usize,
    tasks: usize,
) -> ArmScores {
    let axes = axis_scores(results);
    let composite = super::tune::compute_composite_score(results, &config.weights);
    ArmScores {
        seeds,
        runs: tasks * seeds,
        tool_accuracy: axes.as_ref().map_or(0.0, |a| a.tool_accuracy),
        loop_avoidance: axes.as_ref().and_then(|a| a.loop_avoidance),
        loop_eligible: axes.as_ref().map_or(0, |a| a.loop_eligible),
        task_completion: axes.as_ref().map_or(0.0, |a| a.task_completion),
        composite,
        tg_tps: throughput_tps(results),
        total_completion_tokens: total_completion_tokens(results),
        total_wall_ms: results.iter().map(|r| r.latency_ms).sum(),
        mean_time_to_first_tool_call_ms: mean_time_to_first_tool_call_ms(results),
    }
}

/// Suite-wide completion tokens. `None` only when no task reported usage,
/// which stays distinct from a measured zero.
fn total_completion_tokens(results: &[TuneTaskResult]) -> Option<u64> {
    let mut total: Option<u64> = None;
    for tokens in results.iter().filter_map(|r| r.completion_tokens) {
        total = Some(total.unwrap_or(0) + tokens);
    }
    total
}

/// Mean time to first tool call across the tasks that made one.
///
/// Averaged over callers only: an `Irrelevance` task correctly never calls a
/// tool, and folding its absence in as a zero would flatter whichever arm
/// abstained most.
fn mean_time_to_first_tool_call_ms(results: &[TuneTaskResult]) -> Option<f64> {
    let samples: Vec<u64> = results
        .iter()
        .filter_map(|r| r.time_to_first_tool_call_ms)
        .collect();
    if samples.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    Some(samples.iter().sum::<u64>() as f64 / samples.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::benchmark::tune::task::{ExpectedCall, TaskCategory};

    fn call_task(expected: ExpectedOutcome) -> TuneTask {
        TuneTask {
            id: "t".into(),
            category: TaskCategory::SingleCall,
            system_prompt: None,
            history: None,
            user_prompt: "do it".into(),
            tools: vec![],
            expected,
        }
    }

    /// A demanded call sends `tool_choice: "required"`; an irrelevance task
    /// must not, or the model would be forced to call a tool the task
    /// expects it to abstain from.
    #[test]
    fn tool_choice_follows_the_expected_outcome() {
        let demanding = call_task(ExpectedOutcome::ToolCalls {
            calls: vec![ExpectedCall {
                name: "f".into(),
                required_args: serde_json::Map::new(),
                ordered: false,
            }],
        });
        let abstaining = call_task(ExpectedOutcome::NoToolCall);

        assert!(demands_tool_call(&demanding));
        assert!(!demands_tool_call(&abstaining));
    }
}
