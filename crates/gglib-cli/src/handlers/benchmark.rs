//! CLI handler for `gglib benchmark`.
//!
//! Routes to a local `BenchmarkOps` instance when no daemon is reachable, or
//! proxies HTTP requests to the daemon's `/api/benchmark/…` routes otherwise.
//!
//! # Daemon detection
//!
//! A lightweight probe is sent to `GET http://127.0.0.1:{proxy_port}/health`
//! with a 500 ms timeout.  A response whose JSON body contains
//! `{"service":"gglib-daemon","status":"ok"}` is treated as a live daemon.
//! Any other response (timeout, non-200, wrong JSON shape) falls back to the
//! standalone local path.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use gglib_app_services::{BenchmarkDeps, BenchmarkOps};
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::benchmark::{
    BenchmarkEvent, BenchmarkModelResult, CompareConfig, ModelCompareResult, ModelPerfResult,
    PerfConfig,
};
use gglib_runtime::process::ProcessManager;
use gglib_runtime::{CatalogPortImpl, RuntimePortImpl};

use crate::benchmark_commands::BenchmarkCommand;
use crate::bootstrap::CliContext;
use crate::presentation::style;

// ─── Public entry point ──────────────────────────────────────────────────────

/// Route a `BenchmarkCommand` to the appropriate local or daemon handler.
pub async fn dispatch(ctx: &CliContext, cmd: BenchmarkCommand) -> Result<()> {
    // Read-only subcommands never need daemon proxy — serve locally always.
    match &cmd {
        BenchmarkCommand::List { limit } => return cmd_list(ctx, *limit).await,
        BenchmarkCommand::Show { run_id } => return cmd_show(ctx, *run_id).await,
        BenchmarkCommand::Model { model_id } => return cmd_model(ctx, *model_id).await,
        _ => {}
    }

    // For mutating subcommands (compare, perf) check for a live daemon first.
    if let Some(port) = detect_daemon(ctx).await {
        tracing::debug!("daemon detected on port {port}; proxying benchmark request");
        return proxy_to_daemon(ctx, port, &cmd).await;
    }

    // No daemon — run locally.
    local_dispatch(ctx, cmd).await
}

// ─── Daemon detection ─────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct HealthResponse {
    service: String,
    status: String,
}

/// Probe `GET http://127.0.0.1:{port}/health`.
///
/// Returns `Some(port)` only if the JSON response confirms a live
/// `gglib-daemon` with `status == "ok"`.  Any failure returns `None`.
async fn detect_daemon(ctx: &CliContext) -> Option<u16> {
    let settings = ctx.app.settings().get().await.ok()?;
    let port = settings.effective_proxy_port();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .ok()?;
    let resp = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let health: HealthResponse = resp.json().await.ok()?;
    if health.service == "gglib-daemon" && health.status == "ok" {
        Some(port)
    } else {
        None
    }
}

// ─── Daemon proxy ─────────────────────────────────────────────────────────────

/// Proxy a mutating benchmark command to the running daemon (Phase 4 routes).
///
/// The daemon's HTTP benchmark endpoints are added in Phase 4.  Until then
/// this path warns and falls back to the local runner so the CLI is always
/// usable regardless of Phase 4 status.
async fn proxy_to_daemon(ctx: &CliContext, port: u16, cmd: &BenchmarkCommand) -> Result<()> {
    tracing::debug!("proxy_to_daemon port={port}");
    // Phase 4 will replace this body with proper HTTP streaming.
    // For now fall through to the local path so the CLI works end-to-end.
    eprintln!(
        "{}note:{} daemon detected but HTTP benchmark proxy not yet implemented — \
         running locally",
        style::WARNING,
        style::RESET
    );
    let _ = port;
    local_dispatch(ctx, cmd.clone()).await
}

// ─── Local execution ──────────────────────────────────────────────────────────

async fn local_dispatch(ctx: &CliContext, cmd: BenchmarkCommand) -> Result<()> {
    // Cleanup any zombie runs from a prior crash.
    if let Err(e) = ctx.bench_repo.cleanup_zombie_runs().await {
        tracing::warn!("zombie benchmark run cleanup failed: {e}");
    }

    let ops = build_ops(ctx)?;

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
                ops,
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
        } => cmd_perf(ctx, ops, models, pp, tg, reps).await,

        // Read-only commands are handled before reaching this path.
        BenchmarkCommand::List { limit } => cmd_list(ctx, limit).await,
        BenchmarkCommand::Show { run_id } => cmd_show(ctx, run_id).await,
        BenchmarkCommand::Model { model_id } => cmd_model(ctx, model_id).await,
    }
}

/// Build `BenchmarkOps` for the local path.
fn build_ops(ctx: &CliContext) -> Result<BenchmarkOps> {
    let catalog = Arc::new(CatalogPortImpl::new(ctx.model_repo.clone()));
    let process_mgr = Arc::new(ProcessManager::new_single_swap(
        ctx.base_port,
        ctx.llama_server_path.to_string_lossy().into_owned(),
        catalog,
    ));
    let runtime = Arc::new(RuntimePortImpl::new(process_mgr));
    let http_client =
        BenchmarkDeps::build_http_client().context("failed to build benchmark HTTP client")?;

    Ok(BenchmarkOps::new(BenchmarkDeps {
        model_repo: ctx.model_repo.clone(),
        runtime,
        bench_repo: ctx.bench_repo.clone(),
        http_client,
        settings_repo: ctx.settings_repo.clone(),
    }))
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
    ops: BenchmarkOps,
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

    let cancel = CancellationToken::new();
    let (tx, mut rx) = mpsc::channel::<BenchmarkEvent>(256);

    // Ctrl-C → cancel
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel_clone.cancel();
        }
    });

    let run_task = tokio::spawn(async move { ops.run_compare(config, tx, cancel).await });

    while let Some(event) = rx.recv().await {
        render_event(&event);
    }

    run_task
        .await
        .map_err(|e| anyhow!("benchmark task panicked: {e}"))?
        .context("benchmark compare failed")?;

    Ok(())
}

// ─── benchmark perf ───────────────────────────────────────────────────────────

async fn cmd_perf(
    ctx: &CliContext,
    ops: BenchmarkOps,
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

    let cancel = CancellationToken::new();
    let (tx, mut rx) = mpsc::channel::<BenchmarkEvent>(64);

    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel_clone.cancel();
        }
    });

    let run_task = tokio::spawn(async move { ops.run_perf(config, tx, cancel).await });

    while let Some(event) = rx.recv().await {
        render_event(&event);
    }

    run_task
        .await
        .map_err(|e| anyhow!("benchmark task panicked: {e}"))?
        .context("benchmark perf failed")?;

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
