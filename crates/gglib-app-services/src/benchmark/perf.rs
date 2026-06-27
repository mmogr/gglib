//! Perf-mode benchmark loop.
//!
//! Runs `llama-bench` on each model sequentially and emits
//! [`BenchmarkEvent`]s on `tx`.
//!
//! # VRAM Drain
//!
//! Before launching `llama-bench`, [`super::BenchmarkDeps::runtime`] is used
//! to stop any currently-running llama-server via `stop_current()`.  Without
//! this drain, the llama-server and `llama-bench` would compete for GPU
//! memory, likely causing an OOM or silent performance degradation.  Because
//! the [`ModelRuntimePort`] is the same `SingleSwap` instance shared with
//! `ProxyOps`, stopping the proxy server here is both safe and intentional.
//!
//! # Process Spawning
//!
//! `tokio::process::Command` is used to spawn `llama-bench` with `Stdio::piped()`
//! on both stdout and stderr.  The exit code is checked **before** attempting
//! to parse stdout: a non-zero exit produces a `ModelFailed` event (with the
//! stderr contents as the error message) and the model is skipped.  This
//! prevents confusing "invalid JSON" errors when `llama-bench` writes an error
//! message to stdout.
//!
//! # Cancellation
//!
//! The outer model loop checks the [`CancellationToken`] at the top of each
//! iteration via `tokio::select!`, identical to `compare.rs`.  The `llama-bench`
//! child process is not killed on cancellation — the current model finishes,
//! and the *next* model check is where cancellation is honoured.

use anyhow::{Context as _, Result};
use chrono::Utc;
use tokio::process::Command;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use gglib_core::domain::benchmark::{
    BenchmarkEvent, BenchmarkModelResult, BenchmarkRunType, ModelPerfResult, PerfConfig,
};
use gglib_core::paths::llama_bench_path;

use super::BenchmarkDeps;
use super::mapper::{PerfBenchOutput, parse_perf_output};

/// Entry point called by [`super::BenchmarkOps::run_perf`].
pub async fn run_perf(
    deps: &BenchmarkDeps,
    config: PerfConfig,
    tx: Sender<BenchmarkEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    let config_json = serde_json::to_string(&config).ok();
    let run_id = deps
        .bench_repo
        .create_run(
            BenchmarkRunType::Perf,
            &config.model_ids,
            None,
            None,
            config_json.as_deref(),
        )
        .await
        .context("failed to create benchmark run record")?;

    let total = config.model_ids.len();
    let bench_path = llama_bench_path().context("failed to resolve llama-bench path")?;

    for (idx, &model_id) in config.model_ids.iter().enumerate() {
        // ── Cooperative cancellation check ───────────────────────────────
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                deps.bench_repo.fail_run(run_id, "Aborted by user").await.ok();
                deps.runtime.stop_current().await.ok();
                return Ok(());
            }
            _ = std::future::ready(()) => {}
        }

        let model = match deps.model_repo.get_by_id(model_id).await {
            Ok(m) => m,
            Err(e) => {
                let _ = tx
                    .send(BenchmarkEvent::ModelFailed {
                        model_id,
                        model_name: format!("model #{model_id}"),
                        error: format!("model not found: {e}"),
                    })
                    .await;
                continue;
            }
        };

        // ── Binary existence check ────────────────────────────────────────
        if !bench_path.exists() {
            let _ = tx
                .send(BenchmarkEvent::ModelFailed {
                    model_id,
                    model_name: model.name.clone(),
                    error: format!(
                        "llama-bench not found at {}. Run: gglib config install-llama",
                        bench_path.display()
                    ),
                })
                .await;
            continue;
        }

        // ── VRAM drain ────────────────────────────────────────────────────
        // Stop any running llama-server before loading the model into VRAM
        // with llama-bench. The two processes cannot share GPU memory.
        if let Err(e) = deps.runtime.stop_current().await {
            warn!("benchmark: stop_current() before llama-bench failed: {e}");
        }

        let _ = tx
            .send(BenchmarkEvent::ModelStarted {
                model_id,
                model_name: model.name.clone(),
                position: idx + 1,
                total,
            })
            .await;

        match run_single_perf(deps, model_id, &model, &config, run_id, &bench_path).await {
            Ok(result) => {
                if let Err(e) = deps.bench_repo.save_perf_result(&result, run_id).await {
                    warn!("benchmark: failed to save perf result for model {model_id}: {e}");
                }
                let _ = tx
                    .send(BenchmarkEvent::ModelComplete {
                        model_id,
                        result: BenchmarkModelResult::Perf(result),
                    })
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(BenchmarkEvent::ModelFailed {
                        model_id,
                        model_name: model.name.clone(),
                        error: e.to_string(),
                    })
                    .await;
            }
        }
    }

    if let Err(e) = deps.bench_repo.complete_run(run_id).await {
        warn!("benchmark: failed to complete run {run_id}: {e}");
    }
    let _ = tx.send(BenchmarkEvent::RunComplete { run_id }).await;
    Ok(())
}

/// Spawn `llama-bench` for one model, capture output, and build a result.
async fn run_single_perf(
    _deps: &BenchmarkDeps,
    model_id: i64,
    model: &gglib_core::domain::Model,
    config: &PerfConfig,
    run_id: i64,
    bench_path: &std::path::Path,
) -> Result<ModelPerfResult> {
    use std::process::Stdio;

    let output = Command::new(bench_path)
        .arg("-m")
        .arg(model.file_path.as_os_str())
        .arg("-p")
        .arg(config.pp_tokens.to_string())
        .arg("-n")
        .arg(config.tg_tokens.to_string())
        .arg("-r")
        .arg(config.repetitions.to_string())
        .arg("-o")
        .arg("json")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("failed to spawn llama-bench")?;

    // Check exit code first — do NOT attempt to parse stdout on failure.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        warn!("llama-bench failed for model '{}': {}", model.name, stderr);
        anyhow::bail!("{}", stderr);
    }

    let parsed = parse_perf_output(&output.stdout).ok_or_else(|| {
        anyhow::anyhow!(
            "llama-bench produced empty or unparseable JSON output for '{}'",
            model.name
        )
    })?;

    let PerfBenchOutput {
        tg_tps,
        pp_tps,
        n_gen,
        n_prompt,
        backend,
        ngl,
    } = parsed;

    let tg_tps = tg_tps.ok_or_else(|| {
        anyhow::anyhow!("llama-bench output missing t_avg_tg for '{}'", model.name)
    })?;
    let pp_tps = pp_tps.ok_or_else(|| {
        anyhow::anyhow!("llama-bench output missing t_avg_pp for '{}'", model.name)
    })?;

    Ok(ModelPerfResult {
        id: None,
        model_id,
        run_id: Some(run_id),
        pp_tps,
        tg_tps,
        pp_tokens: n_prompt.unwrap_or(i64::from(config.pp_tokens)),
        tg_tokens: n_gen.unwrap_or(i64::from(config.tg_tokens)),
        backend,
        ngl,
        context_size: None, // not reported by llama-bench -o json
        repetitions: i64::from(config.repetitions),
        created_at: Utc::now(),
    })
}
