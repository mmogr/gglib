//! Compare-mode benchmark loop.
//!
//! Runs the same prompt through N models **sequentially** (one at a time) and
//! emits [`BenchmarkEvent`]s on `tx`.  Sequential execution is intentional:
//! a single GPU cannot run two models in VRAM simultaneously, and the shared
//! [`ModelRuntimePort`] (`SingleSwap`) enforces this at the process level.
//!
//! # Cancellation
//!
//! The outer loop checks the [`CancellationToken`] at the top of **every**
//! model iteration via `tokio::select!`.  On cancellation the task:
//! 1. Marks the run as `Failed("Aborted by user")` in the DB.
//! 2. Calls `runtime.stop_current()` to free GPU memory.
//! 3. Returns immediately — no further models are processed.
//!
//! The token is fired by [`super::guard::BenchmarkTaskGuard`] on SSE stream
//! drop (HTTP client disconnect) or by a CLI `Ctrl+C` handler.
//!
//! # Defensive SSE Parsing
//!
//! All timing fields are extracted via [`super::mapper`] functions that return
//! `Option<f64>`.  If the llama-server build omits the `timings` object, all
//! timing fields in the saved result are `None` — no panic, no hard error.

use anyhow::{Context as _, Result};
use chrono::Utc;
use futures_util::StreamExt;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use gglib_core::domain::InferenceConfig;
use gglib_core::domain::benchmark::{
    BenchmarkEvent, BenchmarkModelResult, BenchmarkRunType, CompareConfig, ModelCompareResult,
};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;

use super::BenchmarkDeps;
use super::mapper::{
    extract_compare_timings, extract_finish_reason, extract_text_delta, extract_usage,
};

/// Entry point called by [`super::BenchmarkOps::run_compare`].
pub async fn run_compare(
    deps: &BenchmarkDeps,
    config: CompareConfig,
    tx: Sender<BenchmarkEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    // ── Load settings once per run (mirrors the proxy's per-request read) ──
    let settings = deps.settings_repo.load().await.ok();
    let default_ctx = settings
        .as_ref()
        .and_then(|s| s.default_context_size)
        .unwrap_or(DEFAULT_CONTEXT_SIZE);
    let global_inf = settings.and_then(|s| s.inference_defaults);
    let config_json = serde_json::to_string(&config).ok();
    let run_id = deps
        .bench_repo
        .create_run(
            BenchmarkRunType::Compare,
            &config.model_ids,
            Some(config.prompt.as_str()),
            config.system_prompt.as_deref(),
            config_json.as_deref(),
        )
        .await
        .context("failed to create benchmark run record")?;

    let total = config.model_ids.len();

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

        // 4-level fallback chain: config override → per-model server_defaults → global setting → hardcoded default
        let resolved_ctx = resolve_context_size(&ServerConfigOptions {
            context_size: config.ctx_size,
            model_server_ctx: model
                .server_defaults
                .as_ref()
                .and_then(|s| s.context_length),
            global_default_ctx: Some(default_ctx),
            ..Default::default()
        });

        let _ = tx
            .send(BenchmarkEvent::ModelStarted {
                model_id,
                model_name: model.name.clone(),
                position: idx + 1,
                total,
            })
            .await;

        match run_single_compare(
            deps,
            model_id,
            &model,
            &config,
            run_id,
            &tx,
            resolved_ctx,
            global_inf.as_ref(),
        )
        .await
        {
            Ok(result) => {
                if let Err(e) = deps.bench_repo.save_compare_result(&result, run_id).await {
                    warn!("benchmark: failed to save compare result for model {model_id}: {e}");
                }
                let _ = tx
                    .send(BenchmarkEvent::ModelComplete {
                        model_id,
                        result: BenchmarkModelResult::Compare(result),
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

/// Run the compare prompt through one model and collect results.
///
/// `resolved_ctx` is the fully-resolved context size (4-level fallback:
/// config override → per-model server_defaults → global setting → hardcoded default),
/// computed per-model by the caller ([`run_compare`]) to avoid redundant
/// settings reads. `global_inf` is also resolved once per run.
#[allow(clippy::too_many_arguments)]
async fn run_single_compare(
    deps: &BenchmarkDeps,
    model_id: i64,
    model: &gglib_core::domain::Model,
    config: &CompareConfig,
    run_id: i64,
    tx: &Sender<BenchmarkEvent>,
    resolved_ctx: u64,
    global_inf: Option<&InferenceConfig>,
) -> Result<ModelCompareResult> {
    // Start (or keep running) the model server via SingleSwap.
    // Pre-resolved per-model context size (4-level fallback).
    // Both args are the same because resolution already happened above with
    // per-model granularity — the runtime's internal resolution is a no-op
    // when context_size is Some.
    let target = deps
        .runtime
        .ensure_model_running(&model.name, Some(resolved_ctx), resolved_ctx)
        .await
        .with_context(|| format!("failed to start model '{}'", model.name))?;

    // Build the request body with fully-resolved inference parameters.
    let req_body = build_compare_request_body(config, model, global_inf);

    let response = deps
        .http_client
        .post(format!("{}/v1/chat/completions", target.base_url))
        .json(&req_body)
        .send()
        .await
        .context("failed to POST to chat completions endpoint")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|e| format!("<body read error: {e}>"));
        anyhow::bail!("llama-server returned {status}: {body}");
    }

    // ── Defensive SSE stream parsing ─────────────────────────────────────
    let mut response_text = String::new();
    let mut was_truncated = false;
    let mut prompt_ms: Option<f64> = None;
    let mut generation_ms: Option<f64> = None;
    let mut prompt_tps: Option<f64> = None;
    let mut generation_tps: Option<f64> = None;
    let mut prompt_tokens: Option<i64> = None;
    let mut completion_tokens: Option<i64> = None;

    let mut byte_stream = response.bytes_stream();
    let mut line_buf = Vec::<u8>::new();

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "benchmark: SSE byte-stream error for model '{}': {e}",
                    model.name
                );
                break;
            }
        };

        for byte in chunk {
            if byte == b'\n' {
                if !line_buf.is_empty() {
                    let line = String::from_utf8_lossy(&line_buf);
                    if let Some(data) = line.strip_prefix("data: ") {
                        let data = data.trim();
                        if data == "[DONE]" {
                            line_buf.clear();
                            // Stream ended cleanly — exit the outer loop.
                            // We'll handle this by breaking out of byte
                            // iteration; the outer while exits naturally
                            // when the stream is exhausted.
                        } else {
                            match serde_json::from_str::<serde_json::Value>(data) {
                                Ok(val) => {
                                    // Text delta
                                    if let Some(delta) = extract_text_delta(&val) {
                                        response_text.push_str(&delta);
                                        let _ = tx
                                            .send(BenchmarkEvent::ModelTextDelta {
                                                model_id,
                                                text: delta,
                                            })
                                            .await;
                                    }
                                    // Finish reason
                                    if matches!(
                                        extract_finish_reason(&val).as_deref(),
                                        Some("length")
                                    ) {
                                        was_truncated = true;
                                    }
                                    // Timings (update only when present)
                                    let (pm, gm, pt, gt) = extract_compare_timings(&val);
                                    if pm.is_some() {
                                        prompt_ms = pm;
                                    }
                                    if gm.is_some() {
                                        generation_ms = gm;
                                    }
                                    if pt.is_some() {
                                        prompt_tps = pt;
                                    }
                                    if gt.is_some() {
                                        generation_tps = gt;
                                    }
                                    // Usage
                                    let (ptu, ctu) = extract_usage(&val);
                                    if ptu.is_some() {
                                        prompt_tokens = ptu;
                                    }
                                    if ctu.is_some() {
                                        completion_tokens = ctu;
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "benchmark: failed to parse SSE chunk for '{}': {e}",
                                        model.name
                                    );
                                }
                            }
                        }
                    }
                    line_buf.clear();
                }
            } else {
                line_buf.push(byte);
            }
        }
    }

    // If the stream ended mid-response (no finish_reason seen), treat as truncated.
    if !was_truncated && response_text.is_empty() {
        was_truncated = true;
    }

    Ok(ModelCompareResult {
        id: None,
        model_id,
        run_id: Some(run_id),
        prompt_text: config.prompt.clone(),
        system_prompt: config.system_prompt.clone(),
        response_text,
        was_truncated,
        prompt_tokens,
        completion_tokens,
        prompt_ms,
        generation_ms,
        prompt_tps,
        generation_tps,
        created_at: Utc::now(),
    })
}

/// Build `messages` array for the chat completions body.
fn build_messages(config: &CompareConfig) -> serde_json::Value {
    let mut messages = Vec::new();
    if let Some(sys) = &config.system_prompt {
        messages.push(serde_json::json!({ "role": "system", "content": sys }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": config.prompt }));
    serde_json::json!(messages)
}

/// Build the full chat-completions request body with resolved inference params.
///
/// Applies the 4-level inference defaults hierarchy to ALL sampling parameters:
///   request (`config.inference`) → model defaults → global settings defaults
///   → hardcoded fallback (`InferenceConfig::with_hardcoded_defaults`).
///
/// This mirrors the resolution the proxy performs for every routed request.
fn build_compare_request_body(
    config: &CompareConfig,
    model: &gglib_core::domain::Model,
    global_inf: Option<&InferenceConfig>,
) -> serde_json::Value {
    let resolved = config
        .inference
        .clone()
        .unwrap_or_default()
        .resolve_with_defaults(model.inference_defaults.as_ref(), global_inf);

    let mut body = serde_json::json!({
        "model": model.name,
        "messages": build_messages(config),
        "stream": true
    });

    if let Some(v) = resolved.temperature {
        body["temperature"] = serde_json::json!(v);
    }
    if let Some(v) = resolved.max_tokens {
        body["max_tokens"] = serde_json::json!(v);
    }
    if let Some(v) = resolved.top_p {
        body["top_p"] = serde_json::json!(v);
    }
    if let Some(v) = resolved.top_k {
        body["top_k"] = serde_json::json!(v);
    }
    if let Some(v) = resolved.repeat_penalty {
        body["repeat_penalty"] = serde_json::json!(v);
    }
    if let Some(v) = resolved.presence_penalty {
        body["presence_penalty"] = serde_json::json!(v);
    }
    if let Some(v) = resolved.min_p {
        body["min_p"] = serde_json::json!(v);
    }

    body
}
