//! List command handler.
//!
//! Fetches and displays GGUF models with optional sort / filter flags.
//! Two code paths are supported:
//!
//! * **Proxy mode** – when a live daemon is detected on the configured port,
//!   the request is forwarded to `GET /api/models?...` so filtering happens
//!   on the backend using the exact same canonical logic.
//! * **Direct mode** – models are loaded from the local SQLite database and
//!   filtered in-process via [`gglib_core::domain::apply_query`].
//!
//! Both paths produce a `Vec<GuiModel>` that is rendered by a single table
//! function.  A speed column (`⚡ t/s`) is shown only when at least one
//! returned model has benchmark data.

use std::time::Duration;

use anyhow::Result;
use gglib_app_services::types::GuiModel;
use gglib_core::domain::{apply_query, ModelListQuery};

use crate::bootstrap::CliContext;
use crate::model_commands::{CliModelSortBy, CliSortOrder};
use crate::presentation::{print_separator, truncate_string};

// ─────────────────────────────────────────────────────────────────────────────
// Public surface
// ─────────────────────────────────────────────────────────────────────────────

/// Arguments forwarded from the `List` CLI variant.
pub struct ListArgs {
    pub sort: CliModelSortBy,
    pub order: CliSortOrder,
    pub min_params: Option<f64>,
    pub max_params: Option<f64>,
    pub min_speed: Option<f64>,
    pub max_speed: Option<f64>,
    pub tags: Vec<String>,
}

/// Execute the list command.
pub async fn execute(ctx: &CliContext, args: ListArgs) -> Result<()> {
    let models = fetch_models(ctx, &args).await?;

    if models.is_empty() {
        println!("No models found.");
        println!("Use 'gglib model add <file_path>' to add your first model.");
        return Ok(());
    }

    println!("Found {} model(s):\n", models.len());
    render_table(&models);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch helpers
// ─────────────────────────────────────────────────────────────────────────────

async fn fetch_models(ctx: &CliContext, args: &ListArgs) -> Result<Vec<GuiModel>> {
    // Prefer the live daemon so both CLI and GUI use the same HTTP path.
    if let Some(port) = detect_daemon(ctx).await {
        return fetch_from_daemon(port, args).await;
    }

    // Direct mode: query local DB and filter in-process.
    let query = build_query(args);
    let all = ctx.app.models().list().await?;
    let filtered = apply_query(all, &query);
    Ok(filtered.into_iter().map(GuiModel::from_domain).collect())
}

async fn fetch_from_daemon(port: u16, args: &ListArgs) -> Result<Vec<GuiModel>> {
    let mut url = format!(
        "http://127.0.0.1:{port}/api/models?sort={}&order={}",
        args.sort.api_value(),
        args.order.api_value(),
    );
    if let Some(v) = args.min_params {
        url.push_str(&format!("&min_params={v}"));
    }
    if let Some(v) = args.max_params {
        url.push_str(&format!("&max_params={v}"));
    }
    if let Some(v) = args.min_speed {
        url.push_str(&format!("&min_speed={v}"));
    }
    if let Some(v) = args.max_speed {
        url.push_str(&format!("&max_speed={v}"));
    }
    if !args.tags.is_empty() {
        url.push_str(&format!("&tags={}", args.tags.join(",")));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let models: Vec<GuiModel> = client.get(&url).send().await?.json().await?;
    Ok(models)
}

fn build_query(args: &ListArgs) -> ModelListQuery {
    ModelListQuery {
        sort_by: args.sort.into(),
        order: args.order.into(),
        min_params: args.min_params,
        max_params: args.max_params,
        min_speed: args.min_speed,
        max_speed: args.max_speed,
        tags: if args.tags.is_empty() {
            None
        } else {
            Some(args.tags.clone())
        },
        ..Default::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Daemon detection (same pattern as benchmark handler)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct HealthResponse {
    service: String,
    status: String,
}

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

// ─────────────────────────────────────────────────────────────────────────────
// Table rendering
// ─────────────────────────────────────────────────────────────────────────────

fn render_table(models: &[GuiModel]) {
    let show_speed = models.iter().any(|m| m.benchmark_summary.is_some());

    if show_speed {
        println!(
            "{:<3} {:<25} {:<8} {:<10} {:<12} {:<8} {:<10} {:<20} File Path",
            "ID", "Name", "Params", "⚡ t/s", "Arch", "Quant", "Context", "Added"
        );
        print_separator(128);
    } else {
        println!(
            "{:<3} {:<25} {:<8} {:<12} {:<8} {:<10} {:<20} File Path",
            "ID", "Name", "Params", "Arch", "Quant", "Context", "Added"
        );
        print_separator(115);
    }

    for model in models {
        let arch = model.architecture.as_deref().unwrap_or("--");
        let quant = model.quantization.as_deref().unwrap_or("--");
        let context = model
            .context_length
            .map(|c| c.to_string())
            .unwrap_or_else(|| "--".to_string());

        if show_speed {
            let speed = model
                .benchmark_summary
                .as_ref()
                .and_then(|s| s.latest_tg_tps)
                .map(|t| format!("{t:.1}"))
                .unwrap_or_else(|| "--".to_string());
            println!(
                "{:<3} {:<25} {:<8.1} {:<10} {:<12} {:<8} {:<10} {:<20} {}",
                model.id,
                truncate_string(&model.name, 24),
                model.param_count_b,
                truncate_string(&speed, 9),
                truncate_string(arch, 11),
                truncate_string(quant, 7),
                truncate_string(&context, 9),
                model.added_at,
                model.file_path,
            );
        } else {
            println!(
                "{:<3} {:<25} {:<8.1} {:<12} {:<8} {:<10} {:<20} {}",
                model.id,
                truncate_string(&model.name, 24),
                model.param_count_b,
                truncate_string(arch, 11),
                truncate_string(quant, 7),
                truncate_string(&context, 9),
                model.added_at,
                model.file_path,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_string_no_truncation_needed() {
        let result = truncate_string("short", 10);
        assert_eq!(result, "short");
    }

    #[test]
    fn test_truncate_string_exact_length() {
        let result = truncate_string("exactly10c", 10);
        assert_eq!(result, "exactly10c");
    }

    #[test]
    fn test_truncate_string_needs_truncation() {
        let result = truncate_string("this is a very long string", 10);
        // 9 chars of content + single-char ellipsis = 10 chars total
        assert_eq!(result, "this is a\u{2026}");
    }
}
