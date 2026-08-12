//! CLI handler for `gglib benchmark`.
//!
//! Mutating subcommands (compare, perf, tune) run on the gglib daemon — the
//! one process that owns llama-server — via its `/api/benchmark/…` SSE
//! routes; this handler streams the events back and renders them exactly as
//! the old in-process channel consumer did. Read-only subcommands (list,
//! show, model) stay local: they are DB reads.

use anyhow::{Context as _, Result, anyhow};

use gglib_core::domain::InferenceConfig;
use gglib_core::domain::benchmark::tune::config::{ScoreWeights, SweepSpec, TuneConfig};
use gglib_core::domain::benchmark::tune::task::{TaskSuite, TuneTask};
use gglib_core::domain::benchmark::{
    AgenticEvalConfig, AgenticEvalReport, ArmScores, BenchmarkEvent, BenchmarkModelResult,
    CONTROL_MIN_COMPOSITE_GAP, CompareConfig, ControlVerdict, DEFAULT_SEEDS, EFFECT_NOISE_RATIO,
    ModelCompareResult, ModelPerfResult, PerfConfig,
};

use crate::benchmark_commands::BenchmarkCommand;
use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::presentation::style;

// ─── Public entry point ──────────────────────────────────────────────────────

/// Route a `BenchmarkCommand` to its handler.
pub async fn dispatch(ctx: &CliContext, cmd: BenchmarkCommand) -> Result<()> {
    match cmd {
        BenchmarkCommand::Compare {
            prompt,
            models,
            system_prompt,
            temperature,
            max_tokens,
            ctx_size,
        } => {
            cmd_compare(
                ctx,
                prompt,
                models,
                system_prompt,
                temperature,
                max_tokens,
                ctx_size,
            )
            .await
        }

        BenchmarkCommand::Perf {
            models,
            pp,
            tg,
            reps,
        } => cmd_perf(ctx, models, pp, tg, reps).await,

        BenchmarkCommand::Tune {
            model,
            sweep,
            task_suite,
            seed_from_gguf,
            seed_from_family_presets,
            prune_fraction,
            weight_tool_accuracy,
            weight_loop_avoidance,
            weight_task_completion,
            weight_speed,
            ctx_size,
            apply,
        } => {
            cmd_tune(
                ctx,
                model,
                sweep,
                task_suite,
                seed_from_gguf,
                seed_from_family_presets,
                prune_fraction,
                weight_tool_accuracy,
                weight_loop_avoidance,
                weight_task_completion,
                weight_speed,
                ctx_size,
                apply,
            )
            .await
        }

        BenchmarkCommand::Agentic {
            model,
            task_suite,
            ctx_size,
            seeds,
            no_control,
            no_replicate,
            replicate_pairs,
            control_seeds,
            json,
            output,
        } => {
            cmd_agentic(
                ctx,
                model,
                task_suite,
                ctx_size,
                seeds,
                !no_control,
                !no_replicate,
                replicate_pairs,
                control_seeds,
                json,
                output,
            )
            .await
        }

        // Read-only commands: plain DB reads, no daemon involved.
        BenchmarkCommand::List { limit } => cmd_list(ctx, limit).await,
        BenchmarkCommand::Show { run_id } => cmd_show(ctx, run_id).await,
        BenchmarkCommand::Model { model_id } => cmd_model(ctx, model_id).await,
    }
}

/// Run one benchmark on the daemon, rendering its SSE events as they land.
///
/// Dropping the stream (Ctrl-C aborts the future) disconnects the request,
/// which is the daemon's cancellation signal — the run stops at the next
/// model boundary and VRAM is freed, exactly as the old in-process
/// `CancellationToken` did.
async fn run_on_daemon(
    path: &str,
    body: &impl serde::Serialize,
    mut on_event: impl FnMut(&BenchmarkEvent),
) -> Result<()> {
    let handle = daemon_client::ensure_daemon().await?;
    let url = format!("{}{path}", daemon_client::base_url());
    let stream =
        daemon_client::sse::stream_json::<BenchmarkEvent, _>(&handle.client, &url, body, |event| {
            on_event(&event)
        });
    tokio::select! {
        result = stream => result,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\n  cancelled — the daemon stops the run at the next model boundary");
            Ok(())
        }
    }
}

// ─── Resolve model identifier → i64 ──────────────────────────────────────────

async fn resolve_model_ids(ctx: &CliContext, identifiers: &[String]) -> Result<Vec<i64>> {
    let mut ids = Vec::with_capacity(identifiers.len());
    for name in identifiers {
        let model = ctx
            .app
            .models()
            .find_by_identifier(name)
            .await
            .with_context(|| format!("model not found: {name}"))?;
        ids.push(model.id);
    }
    Ok(ids)
}

// ─── benchmark compare ────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn cmd_compare(
    ctx: &CliContext,
    prompt: String,
    models: Vec<String>,
    system_prompt: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    ctx_size: Option<u64>,
) -> Result<()> {
    let model_ids = resolve_model_ids(ctx, &models).await?;

    let inference = if temperature.is_some() || max_tokens.is_some() {
        Some(InferenceConfig {
            temperature,
            max_tokens,
            ..Default::default()
        })
    } else {
        None
    };
    let config = CompareConfig {
        model_ids,
        prompt,
        system_prompt,
        inference,
        ctx_size,
    };

    style::print_info_banner("Benchmark Compare", "\u{1f4ca}");
    eprintln!("  Models : {}", models.join(", "));
    style::print_banner_close();

    run_on_daemon("/api/benchmark/compare", &config, render_event).await
}

// ─── benchmark perf ───────────────────────────────────────────────────────────

async fn cmd_perf(
    ctx: &CliContext,
    models: Vec<String>,
    pp: u32,
    tg: u32,
    reps: u32,
) -> Result<()> {
    let model_ids = resolve_model_ids(ctx, &models).await?;

    let config = PerfConfig {
        model_ids,
        pp_tokens: pp,
        tg_tokens: tg,
        repetitions: reps,
    };

    style::print_info_banner("Benchmark Perf", "\u{26a1}");
    eprintln!(
        "  Models : {}  |  pp={pp}  tg={tg}  reps={reps}",
        models.join(", ")
    );
    style::print_banner_close();

    run_on_daemon("/api/benchmark/perf", &config, render_event).await
}

// ─── benchmark tune ───────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn cmd_tune(
    ctx: &CliContext,
    model: String,
    sweep: Vec<String>,
    task_suite: String,
    seed_from_gguf: bool,
    seed_from_family_presets: bool,
    prune_fraction: f32,
    weight_tool_accuracy: Option<f32>,
    weight_loop_avoidance: Option<f32>,
    weight_task_completion: Option<f32>,
    weight_speed: Option<f32>,
    ctx_size: Option<u64>,
    apply: bool,
) -> Result<()> {
    let model_id = resolve_model_ids(ctx, std::slice::from_ref(&model))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("model not found: {model}"))?;

    let sweep_spec = parse_sweep_args(&sweep)?;
    let resolved_task_suite = load_task_suite(&task_suite)?;

    let defaults = ScoreWeights::default();
    let weights = ScoreWeights {
        tool_accuracy: weight_tool_accuracy.unwrap_or(defaults.tool_accuracy),
        loop_avoidance: weight_loop_avoidance.unwrap_or(defaults.loop_avoidance),
        task_completion: weight_task_completion.unwrap_or(defaults.task_completion),
        speed: weight_speed.unwrap_or(defaults.speed),
    };

    let config = TuneConfig {
        model_id,
        task_suite: resolved_task_suite,
        sweep: sweep_spec,
        seed_from_gguf,
        seed_from_family_presets,
        weights,
        prune_fraction,
        ctx_size,
        // A person at a terminal: the activity surfaces show this run as
        // the operator's, not the daemon's.
        initiator: None,
    };

    style::print_info_banner("Benchmark Tune", "\u{1f3af}");
    eprintln!("  Model : {model}");
    eprintln!("  Suite : {task_suite}");
    style::print_banner_close();

    let mut completed_run: Option<i64> = None;
    run_on_daemon("/api/benchmark/tune", &config, |event| {
        if let BenchmarkEvent::RunComplete { run_id } = event {
            completed_run = Some(*run_id);
        }
        render_event(event);
    })
    .await?;

    if apply {
        match completed_run {
            Some(run_id) => apply_gated(run_id).await?,
            None => eprintln!(
                "{}note:{} the run did not complete, so there is nothing to judge",
                style::WARNING,
                style::RESET
            ),
        }
    }

    Ok(())
}

/// Ask the daemon to judge the run against the apply gate and render the
/// verdict — a refusal is an outcome with evidence, not an error.
async fn apply_gated(run_id: i64) -> Result<()> {
    use gglib_app_services::benchmark::tune::apply_run::ApplyOutcome;
    use gglib_core::domain::benchmark::tune::apply::ApplyVerdict;

    let handle = daemon_client::ensure_daemon().await?;
    let url = format!(
        "{}/api/benchmark/tune/{run_id}/apply",
        daemon_client::base_url()
    );
    let outcome: ApplyOutcome = handle
        .client
        .post(&url)
        .send()
        .await
        .context("apply request failed")?
        .error_for_status()
        .context("apply request rejected")?
        .json()
        .await
        .context("apply response unreadable")?;

    match outcome.verdict {
        ApplyVerdict::Apply {
            winner_composite,
            incumbent_mean,
            margin,
            drift,
            paired,
        } => {
            println!(
                "{SUCCESS}\u{2713} applied as measured defaults{RESET}: winner \
                 {winner_composite:.3} over incumbent {incumbent_mean:.3}, margin \
                 {margin:+.3} against drift {drift:.3}",
                SUCCESS = style::SUCCESS,
                RESET = style::RESET,
            );
            if let Some(p) = paired {
                println!(
                    "  paired: {}W-{}L-{}T over {} tasks",
                    p.wins, p.losses, p.ties, p.pairs
                );
            }
        }
        ApplyVerdict::IncumbentStands { incumbent_mean } => println!(
            "{}incumbent stands{} at {incumbent_mean:.3}: no candidate beat the model's \
             current defaults. The run answered its question, and the answer is \
             'change nothing'.",
            style::WARNING,
            style::RESET,
        ),
        ApplyVerdict::WithinDrift { margin, drift } => println!(
            "{}not applied{}: the winner's {margin:+.3} margin is inside the run's own \
             {drift:.3} drift. Unresolved, not absent; more tasks or a re-run resolves \
             it, a smaller threshold never does.",
            style::WARNING,
            style::RESET,
        ),
        ApplyVerdict::PairedDisagrees { wins, losses } => println!(
            "{}not applied{}: the winner's mean rests on a minority of tasks \
             ({wins}W-{losses}L against the incumbent), the lucky-outlier shape, \
             refused by the pairs.",
            style::WARNING,
            style::RESET,
        ),
        ApplyVerdict::Uncalibrated => println!(
            "{}not applied{}: this run has no incumbent calibration pair, so nothing \
             measures its drift. Re-run the tune; every new run carries the pair.",
            style::WARNING,
            style::RESET,
        ),
        ApplyVerdict::Contaminated { unmeasured_runs } => println!(
            "{}not applied{}: {unmeasured_runs} task run(s) never reached the model, so \
             the compared scores are contaminated. A zero from a dead upstream is not a \
             low score.",
            style::WARNING,
            style::RESET,
        ),
    }
    Ok(())
}

/// Run the raw-vs-gglib A/B agentic eval on the daemon and render the delta.
#[allow(clippy::too_many_arguments)]
async fn cmd_agentic(
    ctx: &CliContext,
    model: String,
    task_suite: String,
    ctx_size: Option<u64>,
    seeds: Option<Vec<u32>>,
    include_control: bool,
    replicate_raw: bool,
    replicate_pairs: usize,
    control_seeds: usize,
    json: bool,
    output: Option<std::path::PathBuf>,
) -> Result<()> {
    let model_id = resolve_model_ids(ctx, std::slice::from_ref(&model))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("model not found: {model}"))?;

    // `--seeds` unpassed keeps the default; `--seeds ""` is an explicit
    // request for one unseeded run, so an empty vec must survive as empty
    // rather than falling back to the default.
    let seeds = seeds.unwrap_or_else(|| DEFAULT_SEEDS.to_vec());
    let config = AgenticEvalConfig {
        model_id,
        task_suite: load_task_suite(&task_suite)?,
        weights: ScoreWeights::default(),
        ctx_size,
        seeds: seeds.clone(),
        include_control,
        replicate_raw,
        replicate_pairs,
        control_seeds,
    };

    let mut arms = vec!["raw (pipeline bypassed)", "gglib (full pipeline)"];
    if replicate_raw {
        arms.push("raw again (A/A, disjoint seeds)");
    }
    if include_control {
        arms.push("control (sampling deliberately broken)");
    }
    style::print_info_banner("Agentic A/B Eval", "\u{2696}\u{fe0f}");
    eprintln!("  Model : {model}");
    eprintln!("  Suite : {task_suite}");
    eprintln!("  Arms  : {}", arms.join(" vs "));
    if seeds.is_empty() {
        eprintln!(
            "  Seeds : none — one unseeded run per task, so scores carry full decode variance"
        );
    } else {
        eprintln!(
            "  Seeds : {list} ({n} {runs} per task, scores are their mean)",
            list = seeds
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            n = seeds.len(),
            runs = plural(seeds.len(), "run"),
        );
    }
    // Stated up front rather than discovered in the report: the control's
    // composite sits beside two five-seed numbers and is not one of them.
    if include_control && !seeds.is_empty() && control_seeds < seeds.len() {
        eprintln!(
            "  Note  : the control repeats only {n} of them — it is the slowest arm by far and \
             only has to clear a detection threshold",
            n = control_seeds.max(1),
        );
    }
    style::print_banner_close();

    let mut report: Option<AgenticEvalReport> = None;
    run_on_daemon("/api/benchmark/agentic", &config, |event| {
        if let BenchmarkEvent::AgenticEvalComplete { report: r } = event {
            report = Some((**r).clone());
        }
        render_event(event);
    })
    .await?;

    let report = report.ok_or_else(|| {
        anyhow!("the eval ended without a report (run may have been aborted or failed)")
    })?;

    render_agentic_report(&report);

    if json || output.is_some() {
        let export = serde_json::json!({
            "gglib_version": env!("CARGO_PKG_VERSION"),
            "hardware": fetch_hardware_snapshot().await,
            "report": report,
        });
        let pretty = serde_json::to_string_pretty(&export)?;
        if let Some(path) = output {
            std::fs::write(&path, &pretty)
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("  report written to {}", path.display());
        }
        if json {
            println!("{pretty}");
        }
    }

    Ok(())
}

/// Render the A/B table: one row per axis, columns raw / gglib / delta.
fn render_agentic_report(report: &AgenticEvalReport) {
    eprintln!();
    eprintln!(
        "  {BOLD}{model} ({params}B{quant}) @ {ctx} ctx{RESET}",
        model = report.model_name,
        params = report.param_count_b,
        quant = report
            .quantization
            .as_deref()
            .map_or(String::new(), |q| format!(", {q}")),
        ctx = report.ctx_size,
        BOLD = style::BOLD,
        RESET = style::RESET,
    );
    // The sample size, stated before any score. A composite from one seed and
    // one from five are different measurements, and a table that renders them
    // identically invites exactly the over-reading this eval exists to stop.
    eprintln!();
    if report.gglib.seeds > 1 {
        eprintln!(
            "  {MUTED}every score below is the mean of {seeds} seeds ({runs} runs per arm):              {list}{RESET}",
            seeds = report.gglib.seeds,
            runs = report.gglib.runs,
            list = report
                .seeds
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
    } else {
        eprintln!(
            "  {WARN}single sample per task — these scores carry full decode variance. Two runs              of the identical raw arm have scored 0.728 and 0.543 on this suite.{RESET}",
            WARN = style::WARNING,
            RESET = style::RESET,
        );
    }
    eprintln!();
    eprintln!("  axis              raw    gglib   delta");
    eprintln!("  ─────────────── ────── ────── ───────");
    for (name, raw, gglib, delta) in [
        (
            "tool accuracy  ",
            Some(report.raw.tool_accuracy),
            Some(report.gglib.tool_accuracy),
            Some(report.delta.tool_accuracy),
        ),
        (
            "loop avoidance ",
            report.raw.loop_avoidance,
            report.gglib.loop_avoidance,
            report.delta.loop_avoidance,
        ),
        (
            "task completion",
            Some(report.raw.task_completion),
            Some(report.gglib.task_completion),
            Some(report.delta.task_completion),
        ),
        (
            "composite      ",
            Some(report.raw.composite),
            Some(report.gglib.composite),
            Some(report.delta.composite),
        ),
    ] {
        let colour = match delta {
            Some(d) if d > 0.0 => style::SUCCESS,
            Some(d) if d < 0.0 => style::DANGER,
            _ => "",
        };
        eprintln!(
            "  {name} {raw} {gglib} {colour}{delta}{RESET}",
            raw = fmt_axis(raw, 6),
            gglib = fmt_axis(gglib, 6),
            delta = fmt_delta(delta),
            RESET = style::RESET
        );
    }
    // An arm that never reached a second tool batch cannot have looped, so its
    // loop-avoidance score is unmeasured rather than perfect. Say so, and say
    // over how many tasks — the denominator is the whole story.
    if report.raw.loop_avoidance.is_none() || report.gglib.loop_avoidance.is_none() {
        eprintln!(
            "  {MUTED}loop avoidance measured on {raw}/{total} raw and {gglib}/{total} gglib \
             tasks; unmeasured arms are excluded from the composite{RESET}",
            raw = report.raw.loop_eligible,
            gglib = report.gglib.loop_eligible,
            total = report.tasks.len(),
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
    }

    render_unmeasured_block(report);
    render_noise_block(report);
    render_paired_block(report);
    render_control_block(report);
    render_stability_block(report);
    render_efficiency_block(report);
    eprintln!();
}

/// Runs that never reached the model, and so scored zero for it.
///
/// An arm where *every* run failed aborts the eval and never reaches this
/// renderer. What lands here is the partial case, which is worse to read
/// silently: the arm has real observations, so its numbers look ordinary while
/// being dragged toward zero by runs that measured nothing.
fn render_unmeasured_block(report: &AgenticEvalReport) {
    let arms = [
        ("raw", Some(&report.raw)),
        ("gglib", Some(&report.gglib)),
        ("A/A", report.raw_replicate.as_ref()),
        ("control", report.control.as_ref()),
    ];
    let affected: Vec<(&str, &ArmScores)> = arms
        .into_iter()
        .filter_map(|(name, scores)| scores.map(|s| (name, s)))
        .filter(|(_, s)| s.is_partly_unmeasured())
        .collect();
    if affected.is_empty() {
        return;
    }

    eprintln!();
    eprintln!(
        "  {DANGER}some runs never reached the model and scored zero for it:{RESET}",
        DANGER = style::DANGER,
        RESET = style::RESET,
    );
    for (name, scores) in affected {
        eprintln!(
            "  {DANGER}  {name}: {n}/{runs} runs unmeasured{RESET}",
            n = scores.unmeasured_runs,
            runs = scores.runs,
            DANGER = style::DANGER,
            RESET = style::RESET,
        );
    }
    eprintln!(
        "  {DANGER}Every mean above is diluted by them. Read those arms as a floor, not a \
         measurement.{RESET}",
        DANGER = style::DANGER,
        RESET = style::RESET,
    );
}

/// What the A/A arm says about the size of the delta just rendered.
///
/// Placed immediately under the axis table, because it is the sentence that
/// decides how the composite row should be read — not a footnote to it. A delta
/// of 0.082 above a drift of 0.031 is a finding; the same 0.082 above a drift
/// of 0.070 is a coin landing the same way twice, and the table alone cannot
/// tell them apart.
fn render_noise_block(report: &AgenticEvalReport) {
    let Some(verdict) = report.effect_verdict() else {
        eprintln!();
        eprintln!(
            "  {MUTED}no A/A arm ran, so nothing here shows how much of the delta above is \
             drift — read it as a direction, not a magnitude{RESET}",
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
        return;
    };
    let replicate = report
        .raw_replicate
        .as_ref()
        .map_or(f64::NAN, |r| r.composite);
    let ratio = verdict
        .ratio()
        .map_or_else(|| "—".to_owned(), |r| format!("{r:.1}×"));

    // An unseeded run has no seed list to name, and "re-run on 0 disjoint
    // seeds" would describe an arm that did in fact run.
    let how = if report.raw_replicates.len() > 1 {
        format!(
            "re-run {n} times on disjoint seed sets",
            n = report.raw_replicates.len(),
        )
    } else if report.replicate_seeds.is_empty() {
        "re-run unseeded".to_owned()
    } else {
        format!(
            "re-run on {n} disjoint {seeds}",
            n = report.replicate_seeds.len(),
            seeds = plural(report.replicate_seeds.len(), "seed"),
        )
    };

    eprintln!();
    eprintln!(
        "  {MUTED}A/A: the raw arm {how} scored {replicate:.3} against its own {raw:.3}{RESET}",
        raw = report.raw.composite,
        MUTED = style::MUTED,
        RESET = style::RESET,
    );
    let over = match verdict.pairs() {
        0 | 1 => String::new(),
        pairs => format!(" (mean over {pairs} pairwise gaps)"),
    };
    if verdict.exceeds_noise() {
        eprintln!(
            "  {SUCCESS}effect exceeds drift{RESET}: the {effect:+.3} composite delta is {ratio} \
             the {noise:.3} this eval moves with nothing changed{over}.",
            effect = verdict.effect(),
            noise = verdict.noise(),
            SUCCESS = style::SUCCESS,
            RESET = style::RESET,
        );
    } else {
        eprintln!(
            "  {WARN}effect is within drift{RESET}: the {effect:+.3} composite delta is {ratio} \
             the {noise:.3} this eval moves with nothing changed{over}, under the {min:.0}× \
             needed to call it more than noise.",
            effect = verdict.effect(),
            noise = verdict.noise(),
            min = EFFECT_NOISE_RATIO,
            WARN = style::WARNING,
            RESET = style::RESET,
        );
        eprintln!(
            "  {WARN}That is unresolved, not absent — the fix is more seeds, not a different \
             conclusion.{RESET}",
            WARN = style::WARNING,
            RESET = style::RESET,
        );
    }
    // Printed on success as well as failure. A ratio computed from a
    // handful of gaps is the kind of number that gets quoted as though it
    // were a p-value, and the caveat has to travel with it — sized to the
    // run: the single-pair wording on a three-gap estimate misstates the
    // degrees of freedom in the caveat about degrees of freedom (caught on
    // the first live multi-pair run).
    let df = match verdict.pairs() {
        0 | 1 => "one A/A pair estimates that drift from a single degree of freedom".to_owned(),
        pairs => format!("{pairs} pairwise gaps back that drift estimate"),
    };
    eprintln!(
        "  {MUTED}{df} — this is a sanity ratio, not a significance test{RESET}",
        MUTED = style::MUTED,
        RESET = style::RESET,
    );
}

/// The paired view: the same cells the delta above averages, compared as
/// matched pairs — which is what removes the eval's identical-arm spread
/// from the comparison.
fn render_paired_block(report: &AgenticEvalReport) {
    let Some(paired) = report.paired_effect() else {
        return;
    };
    eprintln!();
    let p = paired.p_value.map_or_else(
        || {
            format!(
                "too few non-tied pairs for a p — read {wins}W against {losses}L directly",
                wins = paired.wins,
                losses = paired.losses,
            )
        },
        |p| format!("Wilcoxon one-sided p = {p:.4}"),
    );
    eprintln!(
        "  paired: {wins}W–{losses}L–{ties}T over {pairs} (task, seed) {pair_word}, \
         mean Δ {mean:+.3} on tool-match; {p}",
        wins = paired.wins,
        losses = paired.losses,
        ties = paired.ties,
        pairs = paired.pairs,
        pair_word = plural(paired.pairs, "pair"),
        mean = paired.mean_delta,
    );
    if paired.unmeasured_pairs > 0 {
        eprintln!(
            "  {WARN}{n} {pairs} dropped: at least one side never reached the model.{RESET}",
            n = paired.unmeasured_pairs,
            pairs = plural(paired.unmeasured_pairs, "pair"),
            WARN = style::WARNING,
            RESET = style::RESET,
        );
    }
}

/// The positive control's verdict.
///
/// Rendered **before** the efficiency numbers and never as a footnote: a
/// control that failed to move invalidates every delta above it, and a reader
/// scanning for the headline number has to meet that fact first.
fn render_control_block(report: &AgenticEvalReport) {
    let Some(verdict) = report.control_verdict() else {
        // Not run. Distinct from "ran and failed", and said so rather than
        // left silent — the same rule the sampling readback applies to blind.
        eprintln!();
        eprintln!(
            "  {MUTED}no control arm ran, so nothing here shows whether this eval could have \
             detected a difference at all{RESET}",
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
        return;
    };
    let control = report.control.as_ref().map_or(f64::NAN, |c| c.composite);
    let gglib = report.gglib.composite;

    eprintln!();
    match verdict {
        ControlVerdict::Moved { gap } => eprintln!(
            "  {SUCCESS}control moved{RESET}: the deliberately broken sampling cost {gap:.3} \
             composite ({control:.3} vs {gglib:.3}), so this run can detect a sampling change.",
            SUCCESS = style::SUCCESS,
            RESET = style::RESET,
        ),
        ControlVerdict::TooSmall { gap } => {
            eprintln!(
                "  {DANGER}control did not move{RESET}: the deliberately broken sampling changed \
                 the composite by only {gap:.3} ({control:.3} vs {gglib:.3}), below the \
                 {min:.2} this apparatus needs to demonstrate sensitivity.",
                min = CONTROL_MIN_COMPOSITE_GAP,
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
            eprintln!(
                "  {DANGER}Treat every delta above as uninterpretable: this run cannot tell \"no \
                 effect\" from \"no sensitivity\".{RESET}",
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
        }
        // Never worded as "barely moved". It moved a great deal, the wrong
        // way, which contradicts the control's premise rather than failing a
        // threshold — and the fix is to the control, not to the suite size.
        ControlVerdict::WrongDirection { gap } => {
            eprintln!(
                "  {DANGER}control moved the WRONG WAY{RESET}: the deliberately broken sampling \
                 scored {gap:.3} ABOVE the gglib arm ({control:.3} vs {gglib:.3}).",
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
            eprintln!(
                "  {DANGER}Its sampling was chosen to be bad, so this contradicts the control's \
                 premise. Fix the control before reading any delta above.{RESET}",
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
        }
    }

    // The control's composite is a coarser number than the two it is printed
    // beside, and nothing else on the line says so.
    let control_seeds = report.control.as_ref().map_or(0, |c| c.seeds);
    if control_seeds < report.gglib.seeds {
        eprintln!(
            "  {MUTED}measured on {control_seeds} of the run's {run_seeds} seeds — enough for a \
             gap this size, and it is the slowest arm in the eval{RESET}",
            run_seeds = report.gglib.seeds,
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
    }

    // What the control does *not* establish, said where it will be read. A
    // control that clears 0.5 licenses no claim about resolving 0.08 — that is
    // the A/A arm's job, and conflating them is the easiest misreading of this
    // whole report.
    if verdict.demonstrated_sensitivity()
        && let Some(effect) = report.effect_verdict()
    {
        eprintln!(
            "  {MUTED}that demonstrates sensitivity at {gap:.3}, not at the {effect:.3} measured \
             above — see the A/A line for that{RESET}",
            gap = match verdict {
                ControlVerdict::Moved { gap }
                | ControlVerdict::TooSmall { gap }
                | ControlVerdict::WrongDirection { gap } => gap,
            },
            effect = effect.effect().abs(),
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
    }
}

/// Tasks that disagreed with themselves across seeds.
///
/// The direct read of where run-to-run variance lives. A suite-level delta
/// smaller than the number of unstable tasks would suggest is a delta to
/// distrust, and this is what makes that judgement possible from the output
/// rather than from a re-run.
fn render_stability_block(report: &AgenticEvalReport) {
    if report.gglib.seeds < 2 {
        return;
    }
    let unstable = report.unstable_tasks();
    if unstable.is_empty() {
        eprintln!(
            "  {MUTED}every task returned the same verdict on all {seeds} seeds in both              arms{RESET}",
            seeds = report.gglib.seeds,
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
        return;
    }
    eprintln!();
    eprintln!(
        "  {WARN}{n} task(s) flipped between seeds — this is where the suite's variance          is:{RESET}",
        n = unstable.len(),
        WARN = style::WARNING,
        RESET = style::RESET,
    );
    for task in unstable {
        let (raw_passed, gglib_passed) = task.pass_counts();
        eprintln!(
            "  {MUTED}  {id}: raw {raw}/{n}, gglib {gglib}/{n} seeds passed{RESET}",
            id = task.task_id,
            raw = raw_passed,
            gglib = gglib_passed,
            n = report.gglib.seeds,
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
    }
}

/// The second table: what the quality axes cannot see.
///
/// Kept separate from the axis table on purpose. These figures are reported
/// beside the composite and never folded into it — they are the arm's cost,
/// not its correctness, and blending them would make the composite
/// incomparable across machines. Deltas here are ratios (`raw ÷ gglib`,
/// "gglib is N× better") rather than differences, because lower is better on
/// every row and the gaps are multiplicative.
fn render_efficiency_block(report: &AgenticEvalReport) {
    eprintln!();
    eprintln!("  efficiency          raw    gglib    factor");
    eprintln!("  ─────────────── ─────── ──────── ─────────");

    eprintln!(
        "  suite wall time {raw} {gglib} {colour}{factor}{RESET}",
        raw = format_args!("{:>7}", fmt_duration(report.raw.total_wall_ms)),
        gglib = format_args!("{:>8}", fmt_duration(report.gglib.total_wall_ms)),
        colour = factor_colour(report.delta.wall_time_speedup),
        factor = fmt_factor(report.delta.wall_time_speedup),
        RESET = style::RESET,
    );
    eprintln!(
        "  completion tok  {raw:>7} {gglib:>8} {colour}{factor}{RESET}",
        raw = fmt_count(report.raw.total_completion_tokens),
        gglib = fmt_count(report.gglib.total_completion_tokens),
        colour = factor_colour(report.delta.completion_token_ratio),
        factor = fmt_factor(report.delta.completion_token_ratio),
        RESET = style::RESET,
    );
    eprintln!(
        "  1st tool call   {raw:>7} {gglib:>8} {blank:>9}",
        raw = fmt_ms(report.raw.mean_time_to_first_tool_call_ms),
        gglib = fmt_ms(report.gglib.mean_time_to_first_tool_call_ms),
        blank = "—",
    );
    eprintln!(
        "  throughput t/s  {raw:>7} {gglib:>8} {blank:>9}",
        raw = fmt_tps(&report.raw),
        gglib = fmt_tps(&report.gglib),
        blank = "—",
    );
}

/// `"seed"` or `"seeds"`. A one-seed run is the common case for the control
/// arm, and "1 disjoint seeds" reads like a formatting fault in output whose
/// whole job is to be believed.
fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

/// Green when gglib came out ahead on a ratio row, red when it came out behind.
fn factor_colour(factor: Option<f64>) -> &'static str {
    match factor {
        Some(f) if f > 1.0 => style::SUCCESS,
        Some(f) if f < 1.0 => style::DANGER,
        _ => "",
    }
}

/// `230.0×`, or an em-dash when the ratio was not measurable.
fn fmt_factor(factor: Option<f64>) -> String {
    factor.map_or_else(
        || format!("{:>9}", "—"),
        |f| {
            let rendered = if f >= 100.0 {
                format!("{f:.0}×")
            } else {
                format!("{f:.1}×")
            };
            format!("{rendered:>9}")
        },
    )
}

/// Milliseconds as `4.8s` past a second, `336ms` below it.
fn fmt_duration(millis: u64) -> String {
    if millis >= 1_000 {
        #[allow(clippy::cast_precision_loss)]
        let secs = millis as f64 / 1_000.0;
        format!("{secs:.1}s")
    } else {
        format!("{millis}ms")
    }
}

fn fmt_ms(millis: Option<f64>) -> String {
    millis.map_or_else(|| "—".to_owned(), |m| fmt_duration(m.round() as u64))
}

fn fmt_count(count: Option<u64>) -> String {
    count.map_or_else(|| "—".to_owned(), |c| c.to_string())
}

fn fmt_tps(scores: &ArmScores) -> String {
    scores
        .tg_tps
        .map_or_else(|| "—".to_owned(), |t| format!("{t:.1}"))
}

/// Render one axis cell, right-aligned in `width`, with an em-dash for an axis
/// that was never measured.
fn fmt_axis(value: Option<f64>, width: usize) -> String {
    value.map_or_else(
        || format!("{:>width$}", "—", width = width),
        |v| format!("{v:>width$.3}", width = width),
    )
}

/// Render a delta cell, signed and right-aligned, with an em-dash when either
/// arm left the axis unmeasured.
fn fmt_delta(value: Option<f64>) -> String {
    value.map_or_else(|| format!("{:>7}", "—"), |v| format!("{v:>+7.3}"))
}

/// Best-effort hardware snapshot for the JSON export, from the daemon's
/// setup-status endpoint. `null` when unavailable — the report is still
/// valid, just unpinned to a machine.
async fn fetch_hardware_snapshot() -> serde_json::Value {
    let url = format!(
        "{}/api/config/system/setup-status",
        daemon_client::base_url()
    );
    match reqwest::Client::new().get(&url).send().await {
        Ok(resp) => resp
            .json::<serde_json::Value>()
            .await
            .unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    }
}

/// Parse `--sweep DIM=V1,V2,...` arguments into a [`SweepSpec`].
fn parse_sweep_args(args: &[String]) -> Result<SweepSpec> {
    let mut sweep = SweepSpec::default();
    for arg in args {
        let (key, values) = arg
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid --sweep '{arg}': expected DIM=V1,V2,..."))?;
        match key {
            "temperature" => sweep.temperature = parse_f32_list(values)?,
            "top_p" => sweep.top_p = parse_f32_list(values)?,
            "top_k" => sweep.top_k = parse_i32_list(values)?,
            "min_p" => sweep.min_p = parse_f32_list(values)?,
            "repeat_penalty" => sweep.repeat_penalty = parse_f32_list(values)?,
            "dry_multiplier" => sweep.dry_multiplier = parse_f32_list(values)?,
            "dynatemp_range" => sweep.dynatemp_range = parse_f32_list(values)?,
            "dynatemp_exponent" => sweep.dynatemp_exponent = parse_f32_list(values)?,
            "top_n_sigma" => sweep.top_n_sigma = parse_f32_list(values)?,
            other => anyhow::bail!(
                "unknown --sweep dimension '{other}': expected one of \
                 temperature, top_p, top_k, min_p, repeat_penalty, \
                 dry_multiplier, dynatemp_range, dynatemp_exponent, top_n_sigma"
            ),
        }
    }
    Ok(sweep)
}

fn parse_f32_list(values: &str) -> Result<Vec<f32>> {
    values
        .split(',')
        .map(|v| {
            v.trim()
                .parse::<f32>()
                .map_err(|e| anyhow!("invalid numeric value '{v}': {e}"))
        })
        .collect()
}

fn parse_i32_list(values: &str) -> Result<Vec<i32>> {
    values
        .split(',')
        .map(|v| {
            v.trim()
                .parse::<i32>()
                .map_err(|e| anyhow!("invalid integer value '{v}': {e}"))
        })
        .collect()
}

/// Resolve `--task-suite` into a [`TaskSuite`].
///
/// `"default"` selects the built-in suite. Any other value is treated as a
/// file path containing a JSON array of [`TuneTask`] values — the identical
/// array shape the GUI parses from an uploaded file — which is wrapped into
/// [`TaskSuite::Custom`].
fn load_task_suite(spec: &str) -> Result<TaskSuite> {
    if spec == "default" {
        return Ok(TaskSuite::Default);
    }
    let content = std::fs::read_to_string(spec)
        .with_context(|| format!("failed to read task suite file: {spec}"))?;
    let tasks: Vec<TuneTask> = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse '{spec}' as a JSON array of task definitions"))?;
    Ok(TaskSuite::Custom { tasks })
}

// ─── benchmark list ───────────────────────────────────────────────────────────

async fn cmd_list(ctx: &CliContext, limit: i64) -> Result<()> {
    use gglib_core::ports::BenchmarkRepositoryPort as _;
    let runs = ctx
        .bench_repo
        .list_runs(limit, 0)
        .await
        .context("failed to fetch benchmark runs")?;

    if runs.is_empty() {
        println!("No benchmark runs found.");
        return Ok(());
    }

    println!(
        "{BOLD}{:>6}  {:<8}  {:<19}  {:<9}  {:<22}  Outcome{RESET}",
        "ID",
        "Type",
        "Started",
        "Status",
        "Initiator",
        BOLD = style::BOLD,
        RESET = style::RESET,
    );
    println!("{}", "─".repeat(96));
    for run in &runs {
        let run_type = format!("{:?}", run.run_type).to_lowercase();
        let started = run.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let status = format!("{:?}", run.status).to_lowercase();
        println!(
            "{:>6}  {:<8}  {:<19}  {:<9}  {:<22}  {}",
            run.id,
            run_type,
            started,
            status,
            run_initiator(run),
            run_outcome(run),
        );
    }
    Ok(())
}

/// Who started a run — the auto-tune scheduler's slug, or a person.
///
/// Read from the stored `TuneConfig`, where the scheduler stamps it; a run
/// whose config predates the field, carries none, or is not a tune run at
/// all is a person's.
fn run_initiator(run: &gglib_core::domain::benchmark::run::BenchmarkRun) -> String {
    run.config_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .and_then(|config| config.get("initiator")?.as_str().map(str::to_owned))
        .map_or_else(|| "operator".to_owned(), |slug| format!("auto ({slug})"))
}

/// The gate's outcome for a tune run, when one was recorded.
///
/// Refusals leave records too, so this reads as the activity line it is:
/// what the gate decided, with its headline number.
fn run_outcome(run: &gglib_core::domain::benchmark::run::BenchmarkRun) -> String {
    use gglib_core::domain::benchmark::tune::apply::ApplyVerdict;

    let Some(record) = run.applied_json.as_deref().and_then(|json| {
        serde_json::from_str::<gglib_core::domain::benchmark::tune::apply::ApplyRecord>(json).ok()
    }) else {
        return "—".to_owned();
    };
    match record.verdict {
        ApplyVerdict::Apply { margin, drift, .. } => {
            format!(
                "{}applied{} (margin {margin:+.3} vs drift {drift:.3})",
                style::SUCCESS,
                style::RESET
            )
        }
        ApplyVerdict::IncumbentStands { incumbent_mean } => {
            format!("refused: incumbent stands at {incumbent_mean:.3}")
        }
        ApplyVerdict::WithinDrift { margin, drift } => {
            format!("refused: margin {margin:+.3} within drift {drift:.3}")
        }
        ApplyVerdict::PairedDisagrees { wins, losses } => {
            format!("refused: pairs disagree ({wins}W-{losses}L)")
        }
        ApplyVerdict::Uncalibrated => "refused: uncalibrated run".to_owned(),
        ApplyVerdict::Contaminated { unmeasured_runs } => {
            format!("refused: {unmeasured_runs} unmeasured run(s)")
        }
    }
}

// ─── benchmark show ───────────────────────────────────────────────────────────

async fn cmd_show(ctx: &CliContext, run_id: i64) -> Result<()> {
    use gglib_core::ports::BenchmarkRepositoryPort as _;
    let run = ctx
        .bench_repo
        .get_run(run_id)
        .await
        .context("failed to fetch benchmark run")?
        .ok_or_else(|| anyhow!("benchmark run #{run_id} not found"))?;

    println!(
        "{BOLD}Run #{id}{RESET}",
        id = run.id,
        BOLD = style::BOLD,
        RESET = style::RESET
    );
    println!("  Type    : {:?}", run.run_type);
    println!("  Status  : {:?}", run.status);
    println!(
        "  Started : {}",
        run.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    if let Some(finished) = run.completed_at {
        println!("  Finished: {}", finished.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    if let Some(ref err) = run.error {
        println!("  {}Error   : {}{}", style::DANGER, err, style::RESET);
    }
    if let Some(ref prompt) = run.prompt_text {
        println!("  Prompt  : {prompt}");
    }
    Ok(())
}

// ─── benchmark model ──────────────────────────────────────────────────────────

async fn cmd_model(ctx: &CliContext, model_id: i64) -> Result<()> {
    use gglib_core::ports::BenchmarkRepositoryPort as _;

    let compare_history = ctx
        .bench_repo
        .get_model_compare_history(model_id, 20)
        .await
        .context("failed to fetch compare history")?;

    let perf_history = ctx
        .bench_repo
        .get_model_perf_history(model_id, 20)
        .await
        .context("failed to fetch perf history")?;

    let summary = ctx
        .bench_repo
        .get_model_summary(model_id)
        .await
        .context("failed to fetch model summary")?;

    println!(
        "{BOLD}Model #{model_id} benchmark history{RESET}",
        BOLD = style::BOLD,
        RESET = style::RESET
    );

    if let Some(s) = summary {
        if let (Some(tg), Some(pp)) = (s.best_tg_tps, s.best_pp_tps) {
            println!(
                "  Best:  {GREEN}{tg:.1} tok/s gen{RESET}  ·  {pp:.1} tok/s prompt",
                GREEN = style::SUCCESS,
                RESET = style::RESET
            );
        }
        println!(
            "  Runs:  {} compare,  {} perf",
            s.compare_run_count, s.perf_run_count
        );
    } else {
        println!("  No benchmark data for this model yet.");
        return Ok(());
    }

    if !compare_history.is_empty() {
        println!(
            "\n{BOLD}── Compare results ──────────────────{RESET}",
            BOLD = style::BOLD,
            RESET = style::RESET
        );
        for r in &compare_history {
            let date = r.created_at.format("%Y-%m-%d %H:%M");
            let gen_tps = r
                .generation_tps
                .map_or("—".into(), |t| format!("{t:.1} tok/s"));
            println!(
                "  {date}  gen={gen_tps}  tokens={tokens}",
                tokens = r.completion_tokens.unwrap_or(0)
            );
        }
    }

    if !perf_history.is_empty() {
        println!(
            "\n{BOLD}── Perf results ─────────────────────{RESET}",
            BOLD = style::BOLD,
            RESET = style::RESET
        );
        for r in &perf_history {
            let date = r.created_at.format("%Y-%m-%d %H:%M");
            let backend = r.backend.as_deref().unwrap_or("cpu");
            println!(
                "  {date}  tg={tg:.1} tok/s  pp={pp:.1} tok/s  [{backend}]",
                tg = r.tg_tps,
                pp = r.pp_tps,
            );
        }
    }

    Ok(())
}

// ─── Event rendering ──────────────────────────────────────────────────────────

fn render_event(event: &BenchmarkEvent) {
    match event {
        BenchmarkEvent::ModelStarted {
            model_name,
            position,
            total,
            ..
        } => {
            eprintln!(
                "\n{BOLD}[{position}/{total}]{RESET} {model_name}",
                BOLD = style::BOLD,
                RESET = style::RESET
            );
        }

        BenchmarkEvent::ModelTextDelta { text, .. } => {
            use std::io::Write as _;
            print!("{text}");
            let _ = std::io::stdout().flush();
        }

        BenchmarkEvent::ModelComplete { result, .. } => {
            println!(); // newline after streaming text
            match result {
                BenchmarkModelResult::Compare(r) => render_compare_complete(r),
                BenchmarkModelResult::Perf(r) => render_perf_complete(r),
            }
        }

        BenchmarkEvent::ModelFailed {
            model_name, error, ..
        } => {
            eprintln!(
                "\n{DANGER}✗ {model_name}: {error}{RESET}",
                DANGER = style::DANGER,
                RESET = style::RESET
            );
        }

        BenchmarkEvent::RunComplete { run_id } => {
            eprintln!(
                "\n{SUCCESS}✓ Run #{run_id} complete{RESET}",
                SUCCESS = style::SUCCESS,
                RESET = style::RESET
            );
        }

        BenchmarkEvent::RunFailed { error } => {
            eprintln!(
                "\n{DANGER}✗ Run failed: {error}{RESET}",
                DANGER = style::DANGER,
                RESET = style::RESET
            );
        }

        BenchmarkEvent::TuneCandidateStarted {
            candidate_index,
            total,
        } => {
            eprintln!(
                "\n{BOLD}[candidate {}/{total}]{RESET}",
                candidate_index + 1,
                BOLD = style::BOLD,
                RESET = style::RESET
            );
        }

        BenchmarkEvent::TuneTaskComplete {
            task_id, passed, ..
        } => {
            let mark = if *passed { "✓" } else { "✗" };
            eprintln!("  {mark} {task_id}");
        }

        BenchmarkEvent::TunePruned {
            candidate_index,
            reason,
        } => {
            eprintln!("  candidate {} pruned: {reason}", candidate_index + 1);
        }

        BenchmarkEvent::TuneCandidateComplete { result } => {
            eprintln!("  composite score: {:.3}", result.composite_score);
        }

        BenchmarkEvent::AgenticArmStarted { arm, total_tasks } => {
            eprintln!(
                "\n{BOLD}[{arm} arm]{RESET} {total_tasks} tasks",
                BOLD = style::BOLD,
                RESET = style::RESET
            );
        }

        BenchmarkEvent::AgenticTaskComplete {
            task_id, passed, ..
        } => {
            let mark = if *passed { "✓" } else { "✗" };
            eprintln!("  {mark} {task_id}");
        }

        // Rendered by cmd_agentic as the final A/B table.
        BenchmarkEvent::AgenticEvalComplete { .. } => {}
    }
}

fn render_compare_complete(r: &ModelCompareResult) {
    let gen_tps = r
        .generation_tps
        .map_or("—".into(), |t| format!("{t:.1} tok/s gen"));
    let pp = r
        .prompt_tps
        .map_or("—".into(), |t| format!("{t:.1} tok/s prompt"));
    let tokens = r.completion_tokens.unwrap_or(0);
    let ms = r
        .generation_ms
        .map_or("—".into(), |m| format!("{:.1}s", m / 1000.0));
    eprintln!(
        "{SUCCESS}✓ {gen_tps}  ·  {pp}  ·  {tokens} tokens  ·  {ms}{RESET}",
        SUCCESS = style::SUCCESS,
        RESET = style::RESET
    );
}

fn render_perf_complete(r: &ModelPerfResult) {
    let backend = r.backend.as_deref().unwrap_or("cpu");
    eprintln!(
        "{SUCCESS}✓ {tg:.1} tok/s gen  ·  {pp:.1} tok/s prompt  [{backend}]{RESET}",
        tg = r.tg_tps,
        pp = r.pp_tps,
        SUCCESS = style::SUCCESS,
        RESET = style::RESET
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_sweep_dimension() {
        let args: Vec<String> = [
            "temperature=0.2,0.8",
            "top_p=0.9",
            "top_k=20,40",
            "min_p=0,0.05",
            "repeat_penalty=1.0,1.1",
            "dry_multiplier=0,0.4,0.8",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

        let sweep = parse_sweep_args(&args).expect("all six dimensions are valid");

        assert_eq!(sweep.temperature, vec![0.2, 0.8]);
        assert_eq!(sweep.top_p, vec![0.9]);
        assert_eq!(sweep.top_k, vec![20, 40]);
        assert_eq!(sweep.min_p, vec![0.0, 0.05]);
        assert_eq!(sweep.repeat_penalty, vec![1.0, 1.1]);
        assert_eq!(sweep.dry_multiplier, vec![0.0, 0.4, 0.8]);
    }

    /// An unswept dimension is empty, which downstream reads as "don't vary
    /// this" rather than "no candidates".
    #[test]
    fn unswept_dimensions_stay_empty() {
        let args = vec!["temperature=0.5".to_owned()];
        let sweep = parse_sweep_args(&args).unwrap();

        assert_eq!(sweep.temperature, vec![0.5]);
        assert!(sweep.top_p.is_empty());
        assert!(sweep.dry_multiplier.is_empty());
    }

    /// The error names the dimension the caller got wrong *and* the valid set,
    /// since a typo is the likeliest cause.
    #[test]
    fn an_unknown_dimension_is_rejected_by_name() {
        let args = vec!["dry_base=1.75".to_owned()];
        let err = parse_sweep_args(&args).unwrap_err().to_string();

        assert!(err.contains("dry_base"), "{err}");
        assert!(err.contains("dry_multiplier"), "{err}");
    }

    #[test]
    fn a_missing_equals_is_rejected() {
        let args = vec!["temperature".to_owned()];
        let err = parse_sweep_args(&args).unwrap_err().to_string();

        assert!(err.contains("DIM=V1,V2"), "{err}");
    }

    #[test]
    fn a_non_numeric_value_is_rejected() {
        let args = vec!["temperature=0.2,hot".to_owned()];
        assert!(parse_sweep_args(&args).is_err());

        let args = vec!["top_k=20,many".to_owned()];
        assert!(parse_sweep_args(&args).is_err());
    }

    #[test]
    fn values_may_be_padded_with_spaces() {
        let sweep = parse_sweep_args(&["temperature=0.2, 0.8 ".to_owned()]).unwrap();
        assert_eq!(sweep.temperature, vec![0.2, 0.8]);
    }

    /// Negative values matter for `dry_penalty_last_n`-style integers even
    /// though it is not a dimension today, and `parse_i32_list` is shared.
    #[test]
    fn integer_lists_accept_negatives() {
        assert_eq!(parse_i32_list("-1,0,64").unwrap(), vec![-1, 0, 64]);
    }
}
