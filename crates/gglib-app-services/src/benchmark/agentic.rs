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
    CONTROL_MIN_COMPOSITE_GAP, ControlVerdict, EFFECT_NOISE_RATIO, EffectVerdict, EvalArm,
    PairedEffect, control_sampling, replicate_seed_set, replicate_seeds,
};
use gglib_core::domain::benchmark::tune::config::ScoreWeights;
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
pub(crate) async fn run_agentic_eval(
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
            // A benchmark pins its own context explicitly above; the fallback
            // rung is unreachable here and must not smuggle in a floor.
            None,
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

    let plans = plan_arms(&config);

    // Per arm, results are grouped by task and ordered by seed within each
    // group, so the per-task drill-down can report N-of-M without re-keying.
    let mut arm_results: Vec<(EvalArm, Vec<Vec<TuneTaskResult>>)> = Vec::with_capacity(plans.len());
    for plan in &plans {
        let arm = plan.arm;
        let seeds = &plan.seeds;
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

        // Checked here rather than at the end, so a dead upstream costs one
        // arm instead of the whole run — and so the report is never assembled
        // out of an arm that has scores but no measurements.
        if let Some(error) = empty_column_error(arm, &per_task) {
            deps.bench_repo.fail_run(run_id, &error).await.ok();
            // The lease is released for the same reason the cancel path
            // releases it: whatever state the server is in, the next run should
            // start from a fresh launch rather than inherit this one.
            deps.runtime.stop_current().await.ok();
            let _ = tx.send(BenchmarkEvent::RunFailed { error }).await;
            return Ok(());
        }

        arm_results.push((arm, per_task));
    }

    // Taken by arm rather than popped in reverse push order: two of the four
    // arms are conditional, and an ordering the reader has to reconstruct from
    // the push sequence is one refactor away from silently attributing the
    // control's scores to the pipeline.
    let raw_results = take_arm(&mut arm_results, EvalArm::Raw);
    let gglib_results = take_arm(&mut arm_results, EvalArm::Gglib);
    // Every A/A pair, in plan order — take_arm removes the first match, so
    // draining repeatedly yields them in the order they ran.
    let mut replicate_runs: Vec<Vec<Vec<TuneTaskResult>>> = Vec::new();
    while let Some(run) = take_arm(&mut arm_results, EvalArm::RawReplicate) {
        replicate_runs.push(run);
    }
    let replicate_results = replicate_runs.first().cloned();
    let control_results = take_arm(&mut arm_results, EvalArm::Control);

    // Resolved once for the whole run, not per arm: an absent `weights` means
    // the client left the choice to us, and every arm must be scored on the
    // same scale or the delta between them measures the scale instead.
    let weights = config.weights.clone().unwrap_or_default();

    // Each arm is scored against *its own* seed count, not the run's: the
    // control repeats fewer seeds than the arms it is compared with, and
    // dividing its totals by the wrong denominator would misreport it.
    let score_arm = |results: &Option<Vec<Vec<TuneTaskResult>>>, arm: EvalArm| {
        results.as_ref().map(|r| {
            let seeds = plans
                .iter()
                .find(|p| p.arm == arm)
                .map_or(1, |p| p.seeds.len());
            arm_scores(&flatten(r), &weights, seeds, tasks.len())
        })
    };

    let raw = score_arm(&raw_results, EvalArm::Raw).unwrap_or_else(|| empty_scores(&weights));
    let gglib = score_arm(&gglib_results, EvalArm::Gglib).unwrap_or_else(|| empty_scores(&weights));
    let raw_replicate = score_arm(&replicate_results, EvalArm::RawReplicate);
    let raw_replicates: Vec<_> = replicate_runs
        .iter()
        .filter_map(|run| score_arm(&Some(run.clone()), EvalArm::RawReplicate))
        .collect();
    let control = score_arm(&control_results, EvalArm::Control);
    let delta = AgenticEvalReport::delta_of(&raw, &gglib);

    let tasks_cmp: Vec<AgenticTaskComparison> = raw_results
        .unwrap_or_default()
        .into_iter()
        .zip(gglib_results.unwrap_or_default())
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
        replicate_seeds: if raw_replicate.is_some() {
            replicate_seeds(&config.seeds)
        } else {
            Vec::new()
        },
        replicate_seed_sets: (1..=replicate_runs.len())
            .filter_map(|pair| u32::try_from(pair).ok())
            .map(|pair| replicate_seed_set(&config.seeds, pair))
            .collect(),
        raw_replicate,
        raw_replicates,
        paired: None,
    };
    let report = AgenticEvalReport {
        paired: PairedEffect::from_tasks(&report.tasks),
        ..report
    };

    // Said out loud, because a control that failed to move invalidates every
    // other number in the report and a reader scanning the deltas will not
    // otherwise notice.
    if let Some(verdict) = report.control_verdict()
        && !verdict.demonstrated_sensitivity()
    {
        warn_control(verdict, &report);
    }
    // Same rule, one step weaker: an effect inside its own noise does not
    // invalidate the report, but reading it as a result would.
    if let Some(verdict) = report.effect_verdict()
        && !verdict.exceeds_noise()
    {
        warn_effect(verdict);
    }
    // A wholly empty arm aborted above. A partly empty one is reported: the
    // report is worth keeping, and the amount by which it is contaminated is
    // knowable only from here.
    warn_partial_arms(&report);

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

/// Why an arm is an empty column rather than a low score, or `None` when at
/// least one of its runs reached the model.
///
/// # The failure this exists to stop
///
/// Measured: llama-server died partway through a run, and the two arms that
/// followed it recorded 45 instant failures each. Those scored a composite of
/// `0.222` — arithmetically correct over 45 zeros, and completely empty — which
/// rendered as an ordinary arm and produced a `-0.562` delta reading as a
/// catastrophic regression. Nothing in the report distinguished it from a real
/// one.
///
/// So the eval refuses to build a report out of it. This is the same rule
/// [ADR 0004] applies to its instruments, one level up: a comparison in which
/// nothing could have been observed, reporting a number, is worse than no
/// report at all — because the number is believable.
///
/// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
fn empty_column_error(arm: EvalArm, per_task: &[Vec<TuneTaskResult>]) -> Option<String> {
    let runs: Vec<&TuneTaskResult> = per_task.iter().flatten().collect();
    if runs.is_empty() || runs.iter().any(|r| r.is_measured()) {
        return None;
    }
    // The first reason, quoted verbatim. 45 copies of one transport error is
    // the common case, and the operator needs the text of it to act.
    let first = runs
        .iter()
        .find_map(|r| r.unmeasured.as_deref())
        .unwrap_or("no reason recorded");
    Some(format!(
        "agentic eval aborted: all {n} runs in the '{arm}' arm failed before reaching the model, \
         so this arm has no measurements — only zeros that would render as a score. First \
         failure: {first}. Check that llama-server is still up (the eval holds one admission \
         lease across every arm, so a crash mid-run empties every arm after it).",
        n = runs.len(),
    ))
}

/// One arm and the seeds it repeats every task under.
struct ArmPlan {
    arm: EvalArm,
    /// `None` entries name no seed at all — the pre-multi-seed behaviour.
    seeds: Vec<Option<u32>>,
}

/// Decide which arms run, and on which seeds.
///
/// The two real arms share the primary seed set, because they are being
/// compared with each other and any asymmetry between them would land in the
/// delta. The other two do not:
///
/// - the A/A arm runs **different** seeds, which is the entire point of it;
/// - the control runs **fewer**, because it is by far the most expensive arm
///   and the gap it has to clear is an order of magnitude above the threshold
///   that reads it.
///
/// Order matters for a run that gets interrupted: the real arms finish first,
/// then the cheap A/A arm, and the control — which on measured runs costs more
/// wall time than everything above it combined — goes last.
fn plan_arms(config: &AgenticEvalConfig) -> Vec<ArmPlan> {
    // An empty seed list still runs once with no seed named, which stays the
    // fastest smoke test.
    let primary: Vec<Option<u32>> = if config.seeds.is_empty() {
        vec![None]
    } else {
        config.seeds.iter().copied().map(Some).collect()
    };

    let mut plans = vec![
        ArmPlan {
            arm: EvalArm::Raw,
            seeds: primary.clone(),
        },
        ArmPlan {
            arm: EvalArm::Gglib,
            seeds: primary.clone(),
        },
    ];

    if config.replicate_raw {
        // One plan per requested pair, each on its own derived seed set —
        // pair 1 is the legacy set, so a multi-pair run's first pair stays
        // comparable with every single-pair run before it. Clamped at one:
        // zero pairs with replicate_raw on would be an A/A arm that does not
        // run while the config says it does.
        for pair in 1..=u32::try_from(config.replicate_pairs.max(1)).unwrap_or(1) {
            // Unseeded, the A/A arm is simply the same request again — which
            // still measures drift, since nothing was pinned in the first
            // place. Only one such pair is meaningful: with no seeds to
            // stride, every further pair would be literally the same plan.
            let seeds = if config.seeds.is_empty() {
                if pair > 1 {
                    break;
                }
                primary.clone()
            } else {
                replicate_seed_set(&config.seeds, pair)
                    .into_iter()
                    .map(Some)
                    .collect()
            };
            plans.push(ArmPlan {
                arm: EvalArm::RawReplicate,
                seeds,
            });
        }
    }

    if config.include_control {
        // Clamped rather than trusted: zero would produce an arm with no runs
        // whose empty scores would then be compared against as if measured.
        let count = config.control_seeds.clamp(1, primary.len());
        plans.push(ArmPlan {
            arm: EvalArm::Control,
            seeds: primary.into_iter().take(count).collect(),
        });
    }

    plans
}

/// Remove one arm's results, or `None` if it did not run.
fn take_arm(
    results: &mut Vec<(EvalArm, Vec<Vec<TuneTaskResult>>)>,
    arm: EvalArm,
) -> Option<Vec<Vec<TuneTaskResult>>> {
    let index = results.iter().position(|(a, _)| *a == arm)?;
    Some(results.remove(index).1)
}

/// Scores for an arm that produced nothing, so a cancelled run still yields a
/// well-formed report rather than a panic.
fn empty_scores(weights: &ScoreWeights) -> ArmScores {
    arm_scores(&[], weights, 0, 0)
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
        EvalArm::Raw | EvalArm::RawReplicate | EvalArm::Gglib => InferenceConfig {
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
        // The A/A arm is the raw arm in every respect but its seeds. Any other
        // difference here — however small — would turn the noise floor it
        // measures into a second A/B comparison wearing the wrong name.
        EvalArm::Raw | EvalArm::RawReplicate => adapter.with_raw_passthrough(true),
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

/// Say that the headline delta did not clear the eval's own drift.
///
/// Deliberately worded as unresolved rather than absent. The A/A arm cannot
/// show that a pipeline does nothing; it can only show that this run lacked the
/// resolution to tell, and the action that follows is more seeds — not a
/// different conclusion about the pipeline.
fn warn_effect(verdict: EffectVerdict) {
    let ratio = verdict
        .ratio()
        .map_or_else(|| "unmeasurable".to_owned(), |r| format!("{r:.1}×"));
    warn!(
        "agentic eval: the composite delta of {effect:+.3} is {ratio} the {noise:.3} drift \
         measured between two runs of the *same* raw arm, under the {EFFECT_NOISE_RATIO:.0}× this \
         report needs before calling a delta more than noise. Read the direction, not the \
         magnitude, and add seeds before quoting this figure.",
        effect = verdict.effect(),
        noise = verdict.noise(),
    );
}

/// Name any arm whose means are diluted by runs that never reached the model.
///
/// Not fatal, unlike a wholly empty arm — the surviving runs are real
/// observations and the report is worth keeping. But every mean in it is
/// dragged toward zero by runs that measured nothing, and the count is the only
/// record of by how much.
fn warn_partial_arms(report: &AgenticEvalReport) {
    let arms = [(EvalArm::Raw, &report.raw), (EvalArm::Gglib, &report.gglib)];
    let replicate = report
        .raw_replicate
        .as_ref()
        .map(|s| (EvalArm::RawReplicate, s));
    let control = report.control.as_ref().map(|s| (EvalArm::Control, s));

    for (arm, scores) in arms.into_iter().chain(replicate).chain(control) {
        if scores.is_partly_unmeasured() {
            warn!(
                "agentic eval: {n} of the '{arm}' arm's {runs} runs never reached the model, and \
                 scored zero for it. Every mean reported for this arm is diluted by them — treat \
                 its numbers as a floor, not a measurement.",
                n = scores.unmeasured_runs,
                runs = scores.runs,
            );
        }
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
    weights: &ScoreWeights,
    seeds: usize,
    tasks: usize,
) -> ArmScores {
    let axes = axis_scores(results);
    let composite = super::tune::compute_composite_score(results, weights);
    ArmScores {
        seeds,
        runs: tasks * seeds,
        unmeasured_runs: results.iter().filter(|r| !r.is_measured()).count(),
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

    /// Build a config from JSON so every test also exercises the serde
    /// defaults, which are what a daemon request actually arrives carrying.
    fn config(extra: &str) -> AgenticEvalConfig {
        let json = format!(r#"{{"model_id": 1, "task_suite": {{"source": "default"}}{extra}}}"#);
        serde_json::from_str(&json).expect("deserializes")
    }

    fn seeds_of(plans: &[ArmPlan], arm: EvalArm) -> Vec<Option<u32>> {
        plans
            .iter()
            .find(|p| p.arm == arm)
            .map(|p| p.seeds.clone())
            .unwrap_or_default()
    }

    /// The two real arms are compared with each other, so any asymmetry in
    /// their seeds would land in the delta rather than in the pipeline.
    #[test]
    fn the_two_real_arms_share_a_seed_set() {
        let plans = plan_arms(&config(r#", "seeds": [1, 2, 3]"#));

        assert_eq!(
            seeds_of(&plans, EvalArm::Raw),
            seeds_of(&plans, EvalArm::Gglib)
        );
        assert_eq!(seeds_of(&plans, EvalArm::Raw).len(), 3);
    }

    /// **The whole design of the A/A arm.** Sharing seeds with the raw arm
    /// would measure decode determinism instead of the seed-draw variance that
    /// actually bounds the primary comparison.
    #[test]
    fn the_replicate_arm_runs_different_seeds_of_the_same_size() {
        let plans = plan_arms(&config(r#", "seeds": [1, 2, 3]"#));

        let raw = seeds_of(&plans, EvalArm::Raw);
        let replicate = seeds_of(&plans, EvalArm::RawReplicate);
        assert_eq!(
            replicate.len(),
            raw.len(),
            "same sample size, or the two \
            composites are not comparable"
        );
        for seed in &replicate {
            assert!(!raw.contains(seed), "{seed:?} was reused");
        }
    }

    /// The expensive arm stops paying for precision nothing reads: one seed,
    /// not the run's five.
    #[test]
    fn the_control_repeats_fewer_seeds_than_the_real_arms() {
        let plans = plan_arms(&config(r#", "seeds": [1, 2, 3, 4, 5]"#));

        assert_eq!(seeds_of(&plans, EvalArm::Control), vec![Some(1)]);
        assert_eq!(seeds_of(&plans, EvalArm::Raw).len(), 5);
    }

    /// Zero would plan an arm with no runs, whose empty scores would then be
    /// compared against as though they had been measured.
    #[test]
    fn a_control_seed_count_of_zero_still_runs_once() {
        let plans = plan_arms(&config(r#", "seeds": [1, 2], "control_seeds": 0"#));

        assert_eq!(seeds_of(&plans, EvalArm::Control).len(), 1);
    }

    /// And asking for more seeds than the run has cannot invent them.
    #[test]
    fn a_control_seed_count_above_the_run_is_clamped_down() {
        let plans = plan_arms(&config(r#", "seeds": [1, 2], "control_seeds": 9"#));

        assert_eq!(seeds_of(&plans, EvalArm::Control).len(), 2);
    }

    /// An unseeded run is the fast smoke test, and the A/A arm still means
    /// something there: nothing was pinned, so repeating the request measures
    /// full decode variance.
    #[test]
    fn an_unseeded_run_still_plans_every_arm_once() {
        let plans = plan_arms(&config(r#", "seeds": []"#));

        for arm in [
            EvalArm::Raw,
            EvalArm::Gglib,
            EvalArm::RawReplicate,
            EvalArm::Control,
        ] {
            assert_eq!(seeds_of(&plans, arm), vec![None], "{arm}");
        }
    }

    /// Opting out of either calibration arm removes it and nothing else.
    #[test]
    fn the_calibration_arms_are_individually_optional() {
        let no_control = plan_arms(&config(r#", "include_control": false"#));
        let no_replicate = plan_arms(&config(r#", "replicate_raw": false"#));

        assert!(!no_control.iter().any(|p| p.arm == EvalArm::Control));
        assert!(no_control.iter().any(|p| p.arm == EvalArm::RawReplicate));
        assert!(!no_replicate.iter().any(|p| p.arm == EvalArm::RawReplicate));
        assert!(no_replicate.iter().any(|p| p.arm == EvalArm::Control));
    }

    /// The control is the most expensive arm by an order of magnitude, so an
    /// interrupted run should already have both real arms and the cheap A/A
    /// one before it starts.
    #[test]
    fn the_control_is_planned_last() {
        let plans = plan_arms(&config(""));

        assert_eq!(plans.last().map(|p| p.arm), Some(EvalArm::Control));
    }

    /// Results are taken by arm rather than popped in push order, so an arm
    /// that did not run yields nothing instead of another arm's scores.
    #[test]
    fn taking_an_arm_that_did_not_run_yields_nothing() {
        let mut results = vec![(EvalArm::Raw, vec![vec![]]), (EvalArm::Gglib, vec![vec![]])];

        assert!(take_arm(&mut results, EvalArm::Control).is_none());
        assert!(take_arm(&mut results, EvalArm::Gglib).is_some());
        assert!(
            take_arm(&mut results, EvalArm::Gglib).is_none(),
            "and it is removed, not cloned"
        );
        assert!(take_arm(&mut results, EvalArm::Raw).is_some());
    }

    fn run(passed: bool, unmeasured: Option<&str>) -> TuneTaskResult {
        TuneTaskResult {
            task_id: "t".to_owned(),
            category: TaskCategory::SingleCall,
            passed,
            tool_match_score: if passed { 1.0 } else { 0.0 },
            loop_detected: false,
            stagnation_detected: false,
            iterations: 1,
            latency_ms: 10,
            completion_tokens: None,
            time_to_first_tool_call_ms: None,
            detail: None,
            unmeasured: unmeasured.map(ToOwned::to_owned),
        }
    }

    /// **The failure this whole check exists for.** 45 runs against a dead
    /// upstream produce a composite that is arithmetically correct and
    /// completely empty, and it must abort rather than be reported.
    #[test]
    fn an_arm_where_nothing_reached_the_model_aborts_the_run() {
        let dead = vec![
            vec![run(false, Some("LLM stream error: connection refused"))],
            vec![run(false, Some("LLM stream error: connection refused"))],
        ];

        let error = empty_column_error(EvalArm::Gglib, &dead).expect("aborts");
        assert!(error.contains("gglib"), "names the arm: {error}");
        assert!(error.contains("all 2 runs"), "names the count: {error}");
        assert!(
            error.contains("connection refused"),
            "quotes the upstream's own reason, which is what the operator acts on: {error}"
        );
    }

    /// One surviving measurement is enough to make the arm a real, if bad,
    /// observation — the eval must not throw away a run over a transient blip.
    #[test]
    fn a_single_measured_run_keeps_the_arm() {
        let mostly_dead = vec![
            vec![run(false, Some("LLM stream error"))],
            vec![run(false, None)],
        ];

        assert!(empty_column_error(EvalArm::Raw, &mostly_dead).is_none());
    }

    /// **The distinction the check turns on.** An arm that failed every task
    /// while talking to the model perfectly well is a real result — a score of
    /// zero is the honest report of a model that got everything wrong.
    #[test]
    fn an_arm_that_merely_failed_everything_is_not_empty() {
        let all_wrong = vec![vec![run(false, None)], vec![run(false, None)]];

        assert!(empty_column_error(EvalArm::Gglib, &all_wrong).is_none());
    }

    /// An arm with no runs planned has nothing to be empty of, and must not be
    /// reported as an upstream failure.
    #[test]
    fn an_arm_with_no_runs_does_not_abort() {
        assert!(empty_column_error(EvalArm::Control, &[]).is_none());
        assert!(empty_column_error(EvalArm::Control, &[vec![]]).is_none());
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
