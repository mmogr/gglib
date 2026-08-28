#![doc = include_str!("README.md")]
//!
//! See the module README for the full design rationale (why no MCP
//! dependency, why no per-candidate model reload).

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context as _, Result};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage};
use gglib_core::domain::benchmark::agentic::REPLICATE_SEED_OFFSET;
use gglib_core::domain::benchmark::tune::config::{ScoreWeights, SweepSpec, TuneConfig};
use gglib_core::domain::benchmark::tune::result::{
    CandidateSource, TuneCandidateResult, TuneTaskResult,
};
use gglib_core::domain::benchmark::tune::task::{TaskCategory, TuneTask};
use gglib_core::domain::benchmark::{BenchmarkEvent, BenchmarkRunType};
use gglib_core::domain::{InferenceConfig, Model};
use gglib_core::ports::{LlmCompletionPort, RunningTarget, ToolExecutorPort, UsageSink};
use gglib_core::request_pipeline::ModelContext;
use gglib_runtime::LlmCompletionAdapter;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub mod apply_run;
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

    // Built and bounded before the run row exists, so an oversized grid is
    // refused outright rather than recorded as a run that failed.
    //
    // Reported as `RunFailed` rather than returned as `Err`: the Axum handler
    // spawns this future and only logs a returned error to tracing, so an
    // `Err` here would abort the run with the caller seeing nothing at all.
    // Every other early failure in this function reports the same way.
    let candidates = build_candidates(&config.sweep, &model, &config);
    if candidates.len() > MAX_CANDIDATES {
        let msg = format!(
            "this sweep would run {} candidates, over the {MAX_CANDIDATES} limit. \
             Every candidate costs at least one agent loop per pre-screen task, and \
             pruning only starts after the whole grid has run, so the cost is the \
             full count. Reduce the values per dimension, or sweep fewer dimensions.",
            candidates.len()
        );
        let _ = tx.send(BenchmarkEvent::RunFailed { error: msg }).await;
        return Ok(());
    }

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

    // ── Load the model once — every candidate only varies per-request
    // sampling parameters, never the loaded llama-server process. ──────────
    let settings = deps.settings_repo.load().await.ok();
    // Passed through, not resolved. `.unwrap_or(DEFAULT_CONTEXT_SIZE)` turned
    // "the user set nothing" into "the user set 4096" and sent it as `num_ctx`
    // — the explicit rung — so the fit below it was computed and discarded on
    // every benchmark launch, and the resident it produced disagreed with the
    // one the proxy wanted. They share a `ProcessManager`, so that disagreement
    // is an evict and a relaunch, both ways.
    let default_ctx = settings.as_ref().and_then(|s| s.default_context_size);
    // The lease is held for the whole sweep: a candidate measured across a
    // model swap would be measuring the swap.
    let admission = match deps
        .runtime
        .admit(
            &model.name,
            // Only what the user actually pinned reaches the explicit rung.
            config.ctx_size,
            default_ctx,
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
    let model_context = model_context_for(&model);
    let mut prescreen_results: Vec<Vec<TuneTaskResult>> = Vec::with_capacity(total);

    for (idx, (candidate_config, source)) in candidates.iter().enumerate() {
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
            let result = run_task(
                &deps.http_client,
                target,
                &model,
                &model_context,
                &seeded(candidate_config, task, source),
                task,
            )
            .await;
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

    // Resolved once: an absent `weights` means the client left the choice to
    // us, so the server's own defaults apply to every candidate in the run.
    let weights = config.weights.clone().unwrap_or_default();

    let prescreen_scores: Vec<f64> = prescreen_results
        .iter()
        .map(|results| compute_composite_score(results, &weights))
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

        // The incumbent pair always runs the full suite: pruning either twin
        // would leave the run uncalibrated, and a pre-screen composite is
        // not comparable with the full-suite scores the gate reads.
        let is_survivor = survivors.contains(&idx)
            || matches!(
                source,
                CandidateSource::Incumbent | CandidateSource::IncumbentCalibration
            );
        let mut task_results = std::mem::take(&mut prescreen_results[idx]);

        if is_survivor {
            for task in &remaining_tasks {
                let result = run_task(
                    &deps.http_client,
                    target,
                    &model,
                    &model_context,
                    &seeded(&candidate_config, task, &source),
                    task,
                )
                .await;
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

        let composite_score = compute_composite_score(&task_results, &weights);
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

/// Hard ceiling on the candidate count, checked before any model work.
///
/// Every candidate costs at least `prescreen_tasks` real agent loops, and
/// pruning cannot save you from a large grid: [`select_survivors`] runs only
/// *after* the whole grid has completed the pre-screen round, and keeps a
/// floor of three. So cost grows linearly in candidates with no upper bound —
/// five axes at four values each is already 1,024 candidates and something
/// like a day of generation.
///
/// This is a runaway guard, not a policy. It sits far above any sweep worth
/// running, so hitting it means a dimension was mistyped or a grid was not
/// thought through.
const MAX_CANDIDATES: usize = 256;

/// The per-model facts a candidate should resolve against.
///
/// Shared with the raw-vs-gglib agentic eval, which needs exactly the same
/// thing for its `gglib` arm. Without it a candidate resolves against
/// [`ModelContext::passthrough`], which quietly changes three things: a
/// `reasoning`-tagged model is tuned against the neutral floor instead of
/// `reasoning_floor`, the agentic temperature ceiling resolves to the
/// non-reasoning 0.3 rather than 0.6, and no dialect or capability shaping
/// applies. Values tuned that way do not transfer to production, which is the
/// whole point of tuning them.
pub(super) fn model_context_for(model: &Model) -> ModelContext {
    ModelContext {
        capabilities: model.capabilities,
        // Same tag fallback the catalog resolution path applies in
        // `From<&ModelSummary> for ModelContext`.
        dialect: gglib_core::normalize::registry::dialect_for_tags(&model.tags),
        tags: model.tags.clone(),
        inference_defaults: model.inference_defaults.clone(),
        defaults_origin: model.defaults_origin,
        context_length: model.context_length,
        template_caps: model.template_caps.clone(),
        catalog_resolved: true,
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

    if config.seed_from_family_presets {
        for (family, preset) in family_presets(model) {
            candidates.push((preset, CandidateSource::FamilyPreset { family }));
        }
    }

    // The incumbent pair, always: an all-None overlay resolves through the
    // normal chain and is therefore exactly what the model does today, and
    // the gap between the identical twins is the run's own drift — the
    // number the apply gate divides every margin by. Two extra candidates
    // is the price of a run that can calibrate itself; a run without them
    // is Uncalibrated and nothing may be applied from it.
    candidates.push((InferenceConfig::default(), CandidateSource::Incumbent));
    candidates.push((
        InferenceConfig::default(),
        CandidateSource::IncumbentCalibration,
    ));

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
    let dry_multipliers = sweep_dimension(&sweep.dry_multiplier);
    let dynatemp_ranges = sweep_dimension(&sweep.dynatemp_range);
    let dynatemp_exponents = sweep_dimension(&sweep.dynatemp_exponent);
    let top_n_sigmas = sweep_dimension(&sweep.top_n_sigma);

    let mut grid = Vec::new();
    for &temperature in &temps {
        for &top_p in &top_ps {
            for &top_k in &top_ks {
                for &min_p in &min_ps {
                    for &repeat_penalty in &repeat_penalties {
                        for &dry_multiplier in &dry_multipliers {
                            for &dynatemp_range in &dynatemp_ranges {
                                for &dynatemp_exponent in &dynatemp_exponents {
                                    for &top_n_sigma in &top_n_sigmas {
                                        grid.push(InferenceConfig {
                                            temperature,
                                            top_p,
                                            top_k,
                                            min_p,
                                            repeat_penalty,
                                            dry_multiplier,
                                            dynatemp_range,
                                            dynatemp_exponent,
                                            top_n_sigma,
                                            max_tokens: None,
                                            presence_penalty: None,
                                            // Not a dimension yet: sweeping
                                            // presence_penalty's twin waits
                                            // for the same per-model evidence
                                            // any new axis does.
                                            frequency_penalty: None,
                                            // The sweep varies sampling
                                            // policy; a seed is not one, and
                                            // is stamped per run by the
                                            // executor so the same candidate
                                            // can be measured at several.
                                            seed: None,
                                            // Not dimensions: llama.cpp's
                                            // defaults (1.75, 2, 64) are
                                            // reasonable, and varying them
                                            // too would multiply the grid
                                            // by 81.
                                            dry_base: None,
                                            dry_allowed_length: None,
                                            dry_penalty_last_n: None,
                                            // Not dimensions, and not
                                            // sweepable ones either: neither
                                            // reasoning control is observable
                                            // at the sampling boundary, and a
                                            // sweep axis nothing can read back
                                            // is an axis whose winner cannot
                                            // be verified to have been applied
                                            // (ADR 0007 finding 7a).
                                            reasoning_effort: None,
                                            reasoning_budget_tokens: None,
                                        });
                                    }
                                }
                            }
                        }
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

/// Built-in per-model-family sampling presets, keyed by a case-insensitive
/// substring match against the model's name.
///
/// Deliberately small: community consensus (as of this writing) documents
/// good coding/tool-use defaults for very few families. Extend this table as
/// more presets are validated, rather than guessing.
///
/// # These are task-regime values, and that is why they disagree with the GGUF
///
/// Do not "correct" a value here to match what the model's own metadata says.
/// The two are answering different questions.
///
/// Publishers document several regimes per model — Qwen documents three or
/// four — but `generation_config.json` encodes exactly one, and that is the
/// one the GGUF converter copies into `general.sampling.*`. For Qwen3.6-27B
/// the embedded value is **temp 1.0**, which is the thinking/general-chat
/// regime. The precise-coding and agentic regime is **0.6**, which is what
/// this table carries and what gglib's traffic actually is.
///
/// So a tool that blindly trusts `general.sampling.temp` runs an agent at
/// 1.67× the author's recommended coding temperature while believing it is
/// being faithful to the author. This table is the complement to that
/// channel, not a legacy workaround for it: the machine-readable path
/// supplies the model's general defaults, and this supplies the regime
/// gglib is actually operating in.
///
/// Two related traps, recorded so they are not rediscovered the hard way:
/// GGUF has no key at all for `presence_penalty` or `frequency_penalty`, and
/// the converter silently drops HF's `repetition_penalty` (it looks for
/// `penalty_repeat`) — so a meaningful part of a publisher's advice is
/// structurally unrepresentable in the embedded channel. And Qwen's card
/// explicitly warns against greedy decoding, so a preset here must never
/// drop to 0.0 in pursuit of determinism.
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
                seed: None,
                // Left open until a sweep validates values for this family,
                // per the rule stated above this table: extend with measured
                // presets, don't guess.
                dry_multiplier: None,
                dry_base: None,
                dry_allowed_length: None,
                dry_penalty_last_n: None,
                dynatemp_range: None,
                dynatemp_exponent: None,
                top_n_sigma: None,
                frequency_penalty: None,
                reasoning_effort: None,
                reasoning_budget_tokens: None,
            },
        ));
    }

    presets
}

/// The seed a candidate runs `task` under.
///
/// # Why tune tasks are seeded at all
///
/// Measured on the first live gated apply: unseeded, the incumbent twins
/// scored *identically*, so the run's drift read 0.000 — and at zero drift
/// the ratio gate passes any positive margin, leaving the paired check as
/// the only guard. A calibration instrument that reads zero because nothing
/// was pinned is the inert-organ trap in one more costume.
///
/// # The design, mirrored from the agentic eval
///
/// Every candidate runs task `T` under the **same** derived seed — common
/// random numbers, so candidate-versus-candidate comparisons stay tightly
/// paired. The calibration twin alone runs offset seeds
/// ([`REPLICATE_SEED_OFFSET`], the same constant and rationale as the A/A
/// arm's): its gap from the incumbent then genuinely samples seed-to-seed
/// variance under the incumbent's own configuration, which is exactly the
/// noise the apply gate's question is about — would this margin replicate
/// under different draws?
///
/// Derived from the task *id*, not its position, so the seed survives suite
/// reordering and additions; derived rather than drawn so a surprising score
/// can be re-run and reproduced instead of chased.
fn tune_task_seed(task_id: &str, calibration_twin: bool) -> u32 {
    // djb2 over the id bytes: stable, dependency-free, and well-spread
    // enough for a decode seed — this is reproducibility, not cryptography.
    let mut hash: u32 = 5381;
    for byte in task_id.as_bytes() {
        hash = hash.wrapping_mul(33) ^ u32::from(*byte);
    }
    if calibration_twin {
        hash = hash.wrapping_add(REPLICATE_SEED_OFFSET);
    }
    hash
}

/// A candidate's config with the per-task seed stamped in.
fn seeded(
    candidate: &InferenceConfig,
    task: &TuneTask,
    source: &CandidateSource,
) -> InferenceConfig {
    let mut config = candidate.clone();
    config.seed = Some(tune_task_seed(
        &task.id,
        matches!(source, CandidateSource::IncumbentCalibration),
    ));
    config
}

/// Run one task against one candidate's sampling settings through the real
/// `AgentLoop`, scoring the recorded tool calls against the task's expected
/// outcome.
async fn run_task(
    http_client: &reqwest::Client,
    target: &RunningTarget,
    model: &Model,
    model_context: &ModelContext,
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
                // Resolve against the real model, not `passthrough` — see
                // `model_context_for`.
                .with_model_context(model_context.clone())
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
    F: Fn(Arc<dyn UsageSink>) -> Arc<dyn LlmCompletionPort>,
{
    let mut retries = 0u32;
    loop {
        let mut result = run_task_once(&build_llm, task).await;
        if !should_retry(&result, retries) {
            result.transport_retries = retries;
            return result;
        }
        retries += 1;
        warn!(
            task_id = %task.id,
            attempt = retries,
            of = TRANSPORT_RETRY_ATTEMPTS,
            reason = result.unmeasured.as_deref().unwrap_or("unknown"),
            "benchmark: run reached no model; retrying"
        );
    }
}

/// Whether an attempt is worth throwing away and repeating.
///
/// Keyed on `unmeasured`, never on a matched error string. That field is
/// already the harness's answer to "was anything observed here", and it is set
/// for exactly the two ways a run can produce nothing: the loop could not reach
/// the upstream, or its task panicked.
///
/// The half that matters is the one that says **no**. Every way of *doing
/// badly* — a detected loop, a stagnated answer, a wrong tool call, an
/// exhausted iteration budget — leaves `unmeasured` at `None` and is returned
/// untouched. Retrying those would be the eval resampling until it liked the
/// answer, which is a far worse defect than the one this retry fixes.
const fn should_retry(result: &TuneTaskResult, retries: u32) -> bool {
    !result.is_measured() && retries < TRANSPORT_RETRY_ATTEMPTS
}

/// How many extra attempts a run reaching no model is given.
///
/// Small on purpose. This exists so a single transient transport failure does
/// not delete a run from one arm and silently skew the comparison — not so the
/// eval can grind through a genuinely dead upstream. An arm whose every run is
/// unmeasured still aborts the eval, and does so three times slower now, which
/// is the price of not mistaking a blip for a corpse.
const TRANSPORT_RETRY_ATTEMPTS: u32 = 2;

/// One attempt at a task. See [`run_task_with_llm`], which owns the retry.
///
/// `latency_ms` here is *this attempt's* wall time, so a retried run reports
/// what its successful attempt cost rather than the sum of its failures. The
/// discarded time is not lost — it is what `transport_retries` is for.
async fn run_task_once<F>(build_llm: &F, task: &TuneTask) -> TuneTaskResult
where
    F: Fn(Arc<dyn UsageSink>) -> Arc<dyn LlmCompletionPort>,
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

    // `unmeasured` separates "the model did badly" from "there was no model".
    //
    // Every guard and budget below is a real observation: a loop, a stagnated
    // answer, an over-wide parallel batch and an exhausted iteration budget are
    // all things the model *did*, and they score honestly as failures.
    // `Internal` is the odd one out — it is the loop reporting that it could
    // not talk to the upstream at all, or could not start — and a run that
    // never reached the model has no score to give, only a zero that looks
    // exactly like a bad one.
    let (loop_detected, stagnation_detected, error_detail, unmeasured) = match &run_result {
        Ok(Ok(_)) => (false, false, None, None),
        Ok(Err(gglib_core::ports::AgentError::LoopDetected { .. })) => (true, false, None, None),
        Ok(Err(gglib_core::ports::AgentError::StagnationDetected { .. })) => {
            (false, true, None, None)
        }
        Ok(Err(e @ gglib_core::ports::AgentError::Internal(_))) => {
            (false, false, Some(e.to_string()), Some(e.to_string()))
        }
        Ok(Err(e)) => (false, false, Some(e.to_string()), None),
        // A panicked task produced nothing either, and its zeros are just as
        // empty as an unreachable upstream's.
        Err(join_err) => {
            let msg = format!("agent task panicked: {join_err}");
            (false, false, Some(msg.clone()), Some(msg))
        }
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
        unmeasured,
        // Stamped by `run_task_with_llm`, which is the only thing that knows
        // how many attempts this one cost.
        transport_retries: 0,
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
/// There is no speed component. `tg_tps` is *measured* per candidate
/// ([`TuneCandidateResult::tg_tps`]) and reported, but scoring it would mean
/// ranking each candidate against the fastest in the run, which cannot be
/// computed while candidates stream out one at a time. A `speed` weight
/// existed for a while and was never read; it is gone, along with the
/// `--weight-speed` flag that set it. The three self-contained components
/// (tool accuracy, loop avoidance, task completion) are renormalized to sum
/// to `1.0` of the available weight.
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
    use gglib_core::domain::benchmark::tune::task::TaskSuite;

    // ── Task seeding (the calibration instrument's integrity) ───────────────

    /// Common random numbers: every non-calibration candidate runs a task
    /// under the same seed, so candidate comparisons stay paired.
    #[test]
    fn every_candidate_runs_a_task_under_the_same_seed() {
        assert_eq!(
            tune_task_seed("single_call_weather", false),
            tune_task_seed("single_call_weather", false),
        );
    }

    /// The calibration twin alone runs offset seeds — its gap from the
    /// incumbent samples seed-to-seed variance, which is what makes the
    /// drift a reading instead of the 0.000 the first live run measured.
    #[test]
    fn the_calibration_twin_runs_different_seeds() {
        let primary = tune_task_seed("single_call_weather", false);
        let twin = tune_task_seed("single_call_weather", true);
        assert_ne!(primary, twin);
        assert_eq!(twin, primary.wrapping_add(REPLICATE_SEED_OFFSET));
    }

    /// Seeds derive from the task id, not its position: distinct tasks get
    /// distinct seeds, and reordering the suite changes nothing.
    #[test]
    fn distinct_tasks_get_distinct_seeds() {
        let suite = TaskSuite::Default.resolve().expect("suite resolves");
        let mut seeds: Vec<u32> = suite.iter().map(|t| tune_task_seed(&t.id, false)).collect();
        let total = seeds.len();
        seeds.sort_unstable();
        seeds.dedup();
        assert_eq!(seeds.len(), total, "seed collision in the default suite");
    }

    /// The stamp lands on the config the task actually runs under, and the
    /// candidate's own fields survive it.
    #[test]
    fn seeded_stamps_the_seed_and_keeps_the_candidate() {
        let suite = TaskSuite::Default.resolve().expect("suite resolves");
        let task = &suite[0];
        let candidate = InferenceConfig {
            temperature: Some(0.7),
            ..Default::default()
        };
        let stamped = seeded(&candidate, task, &CandidateSource::UserGrid);
        assert_eq!(stamped.temperature, Some(0.7));
        assert_eq!(stamped.seed, Some(tune_task_seed(&task.id, false)));

        let twin = seeded(&candidate, task, &CandidateSource::IncumbentCalibration);
        assert_eq!(twin.seed, Some(tune_task_seed(&task.id, true)));
    }

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
            unmeasured: None,
            transport_retries: 0,
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

    /// A run that reached no model is worth repeating, up to the budget.
    #[test]
    fn an_unmeasured_run_is_retried_until_the_budget_runs_out() {
        let mut unreachable = task_result(0.0, false, false);
        unreachable.unmeasured = Some("SSE byte-stream error: operation timed out".to_owned());

        for spent in 0..TRANSPORT_RETRY_ATTEMPTS {
            assert!(
                should_retry(&unreachable, spent),
                "an unmeasured run with {spent} attempt(s) spent should be retried"
            );
        }
        assert!(
            !should_retry(&unreachable, TRANSPORT_RETRY_ATTEMPTS),
            "the budget must be finite — a dead upstream is not retried forever"
        );
    }

    /// **The half of the rule that matters.** Retrying a run the model actually
    /// failed would let the eval resample until it liked the answer, which is a
    /// worse defect than the lost runs the retry exists to prevent. Every one
    /// of these is a real observation and must be returned as it stands.
    #[test]
    fn a_measured_failure_is_never_retried() {
        let wrong_call = task_result(0.0, false, false);
        let looped = task_result(0.0, false, true);
        let mut stagnated = task_result(0.0, false, false);
        stagnated.stagnation_detected = true;

        for (name, result) in [
            ("a wrong tool call", wrong_call),
            ("a detected loop", looped),
            ("a stagnated answer", stagnated),
        ] {
            assert!(
                result.is_measured(),
                "{name} is an observation, not an absence of one"
            );
            assert!(!should_retry(&result, 0), "{name} must not be re-rolled");
        }
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
            ..Default::default()
        };
        let grid = build_candidate_grid(&sweep);
        assert_eq!(grid.len(), 2);
        assert!(grid.iter().any(|c| c.temperature == Some(0.2)));
        assert!(grid.iter().any(|c| c.temperature == Some(0.8)));
        assert!(grid.iter().all(|c| c.top_p == Some(0.9)));
        assert!(grid.iter().all(|c| c.top_k.is_none()));
    }

    /// The new axis, and the reason it exists: `0.0` is a real candidate
    /// meaning "DRY off", so one sweep measures off against two strengths.
    #[test]
    fn dry_multiplier_is_a_sweep_dimension() {
        let sweep = SweepSpec {
            dry_multiplier: vec![0.0, 0.4, 0.8],
            ..Default::default()
        };
        let grid = build_candidate_grid(&sweep);
        assert_eq!(grid.len(), 3);
        for expected in [0.0, 0.4, 0.8] {
            assert!(
                grid.iter().any(|c| c.dry_multiplier == Some(expected)),
                "missing dry_multiplier {expected}"
            );
        }
        // The other three DRY parameters are not dimensions; they stay unset
        // so llama.cpp's own defaults apply.
        assert!(grid.iter().all(|c| c.dry_base.is_none()));
        assert!(grid.iter().all(|c| c.dry_allowed_length.is_none()));
        assert!(grid.iter().all(|c| c.dry_penalty_last_n.is_none()));
    }

    /// The grid multiplies, so the guard has to be checked against a product
    /// rather than any single dimension's length.
    #[test]
    fn dimensions_multiply() {
        let sweep = SweepSpec {
            temperature: vec![0.2, 0.8],
            top_p: vec![0.9, 0.95],
            top_k: vec![20, 40],
            min_p: vec![0.0, 0.05],
            repeat_penalty: vec![1.0, 1.1],
            dry_multiplier: vec![0.0, 0.8],
            ..Default::default()
        };
        assert_eq!(build_candidate_grid(&sweep).len(), 64);
    }

    /// The entropy-adaptive dimensions multiply like the original six, and an
    /// off-vs-on sweep pairs a disabled sentinel with a live value in one
    /// grid — the shape the flat-vs-dynatemp comparison runs.
    #[test]
    fn entropy_adaptive_dimensions_multiply() {
        let sweep = SweepSpec {
            temperature: vec![0.6, 1.0],
            dynatemp_range: vec![0.0, 0.4],
            top_n_sigma: vec![-1.0, 1.0],
            ..Default::default()
        };
        let grid = build_candidate_grid(&sweep);
        assert_eq!(grid.len(), 8);
        assert!(grid.iter().any(|c| c.dynatemp_range == Some(0.4)
            && c.top_n_sigma == Some(-1.0)
            && c.temperature == Some(1.0)));
        // Unswept entropy fields stay unset, not zeroed.
        let unswept = SweepSpec {
            temperature: vec![0.5],
            ..Default::default()
        };
        assert!(
            build_candidate_grid(&unswept)
                .iter()
                .all(|c| c.dynatemp_range.is_none()
                    && c.dynatemp_exponent.is_none()
                    && c.top_n_sigma.is_none())
        );
    }

    /// `MAX_CANDIDATES` is a runaway guard, so it must sit above any sweep
    /// worth running and below the point where a mistyped grid burns a day.
    #[test]
    fn the_candidate_cap_admits_a_realistic_sweep_and_rejects_a_runaway() {
        let realistic = SweepSpec {
            temperature: vec![0.2, 0.5, 0.8],
            dry_multiplier: vec![0.0, 0.4, 0.8],
            ..Default::default()
        };
        assert!(build_candidate_grid(&realistic).len() <= MAX_CANDIDATES);

        let runaway = SweepSpec {
            temperature: vec![0.1, 0.2, 0.4, 0.6],
            top_p: vec![0.8, 0.9, 0.95, 1.0],
            top_k: vec![10, 20, 40, 80],
            min_p: vec![0.0, 0.02, 0.05, 0.1],
            repeat_penalty: vec![1.0, 1.05, 1.1, 1.2],
            dry_multiplier: vec![0.0, 0.4, 0.8, 1.2],
            ..Default::default()
        };
        assert!(build_candidate_grid(&runaway).len() > MAX_CANDIDATES);
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
            template_caps: None,
            benchmark_summary: None,
        }
    }
}
