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
use gglib_core::domain::benchmark::tune::result::TuneCandidateResult;
use gglib_core::domain::benchmark::tune::task::{TaskSuite, TuneTask};
use gglib_core::domain::benchmark::{
    AgenticEvalConfig, AgenticEvalReport, ArmScores, BenchmarkEvent, BenchmarkModelResult,
    CompareConfig, ModelCompareResult, ModelPerfResult, PerfConfig,
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
            apply_best,
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
                apply_best,
            )
            .await
        }

        BenchmarkCommand::Agentic {
            model,
            task_suite,
            ctx_size,
            json,
            output,
        } => cmd_agentic(ctx, model, task_suite, ctx_size, json, output).await,

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
    apply_best: bool,
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
    };

    style::print_info_banner("Benchmark Tune", "\u{1f3af}");
    eprintln!("  Model : {model}");
    eprintln!("  Suite : {task_suite}");
    style::print_banner_close();

    let mut best: Option<TuneCandidateResult> = None;
    run_on_daemon("/api/benchmark/tune", &config, |event| {
        if let BenchmarkEvent::TuneCandidateComplete { result } = event
            && !result.pruned
            && best
                .as_ref()
                .is_none_or(|b| result.composite_score > b.composite_score)
        {
            best = Some(result.clone());
        }
        render_event(event);
    })
    .await?;

    if apply_best {
        match best {
            Some(winner) => apply_best_config(ctx, model_id, &winner).await?,
            None => eprintln!(
                "{}note:{} no surviving candidate to apply (run may have been aborted)",
                style::WARNING,
                style::RESET
            ),
        }
    }

    Ok(())
}

/// Run the raw-vs-gglib A/B agentic eval on the daemon and render the delta.
async fn cmd_agentic(
    ctx: &CliContext,
    model: String,
    task_suite: String,
    ctx_size: Option<u64>,
    json: bool,
    output: Option<std::path::PathBuf>,
) -> Result<()> {
    let model_id = resolve_model_ids(ctx, std::slice::from_ref(&model))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("model not found: {model}"))?;

    let config = AgenticEvalConfig {
        model_id,
        task_suite: load_task_suite(&task_suite)?,
        weights: ScoreWeights::default(),
        ctx_size,
    };

    style::print_info_banner("Agentic A/B Eval", "\u{2696}\u{fe0f}");
    eprintln!("  Model : {model}");
    eprintln!("  Suite : {task_suite}");
    eprintln!("  Arms  : raw (pipeline bypassed) vs gglib (full pipeline)");
    style::print_banner_close();

    let mut report: Option<AgenticEvalReport> = None;
    run_on_daemon("/api/benchmark/agentic", &config, |event| {
        if let BenchmarkEvent::AgenticEvalComplete { report: r } = event {
            report = Some(r.clone());
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

    render_efficiency_block(report);
    eprintln!();
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
            other => anyhow::bail!(
                "unknown --sweep dimension '{other}': expected one of \
                 temperature, top_p, top_k, min_p, repeat_penalty"
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

/// Write the winning candidate's sampling settings to the model's
/// `inference_defaults`, mirroring `gglib model update <id> --temperature ...`.
async fn apply_best_config(
    ctx: &CliContext,
    model_id: i64,
    winner: &TuneCandidateResult,
) -> Result<()> {
    let mut model = ctx
        .model_repo
        .get_by_id(model_id)
        .await
        .with_context(|| format!("failed to load model {model_id} to apply tune result"))?;
    model.inference_defaults = Some(winner.config.clone());
    ctx.model_repo
        .update(&model)
        .await
        .context("failed to save tuned inference defaults")?;

    println!(
        "{SUCCESS}\u{2713} Applied best config (score {:.3}) to model {model_id}'s inference_defaults{RESET}",
        winner.composite_score,
        SUCCESS = style::SUCCESS,
        RESET = style::RESET
    );
    Ok(())
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
        "{BOLD}{:>6}  {:<8}  {:<19}  Status{RESET}",
        "ID",
        "Type",
        "Started",
        BOLD = style::BOLD,
        RESET = style::RESET,
    );
    println!("{}", "─".repeat(52));
    for run in &runs {
        let run_type = format!("{:?}", run.run_type).to_lowercase();
        let started = run.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        println!(
            "{:>6}  {:<8}  {:<19}  {:?}",
            run.id, run_type, started, run.status
        );
    }
    Ok(())
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
