//! `gglib serve <image model>`: pin the proxy to a model that draws, and
//! load it once.
//!
//! A `#[path]` child of `serve.rs`. An image model is served by
//! stable-diffusion.cpp's `sd-server`, so what this path ensures is that
//! runtime, not llama.cpp, and it has none of a chat model's launch to set:
//! a context flag, `--mlock`, `--jinja`, MTP, sampling or a profile is
//! refused by name rather than dropped without a word. It starts the same
//! pinned proxy as a chat model's `serve`, then asks it once, `POST
//! /v1/models/{name}/load`, so the model is resident before the first
//! request and a refusal (the runtime not installed, a missing component, no
//! room beside a held model) is read here, in the proxy's own words. Drawing
//! through the endpoint is a later change; this one loads.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use gglib_app_services::launch_options::{ProxyGlobals, plan_pinned_launch};
use gglib_app_services::types::StartServerRequest;
use gglib_core::contracts::http::path_segment;
use gglib_core::domain::{LaunchNarration, Model};
use gglib_core::ports::PinnedSpec;
use gglib_core::server_config::ServerConfigOptions;
use gglib_proxy::LoadResponse;
use gglib_proxy::models::ErrorResponse;

use super::start_body;
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, auth};
use crate::shared_args::{AccessArgs, CacheArgs, ContextArgs, MtpArgs, SamplingArgs, ServeOptions};

/// How long one load may take: a queue wait behind other launches, then
/// `sd-server` reading its weights. The paired machine's load allows the same.
const LOAD_TIMEOUT: Duration = Duration::from_mins(5);

/// The flags `serve` takes for a chat model's launch, as typed.
pub(super) struct ChatFlags<'a> {
    pub(super) context: &'a ContextArgs,
    pub(super) options: &'a ServeOptions,
    pub(super) sampling: &'a SamplingArgs,
    pub(super) mtp: &'a MtpArgs,
    /// Whether a profile was named, by `--profile` or a `:profile` suffix.
    pub(super) profile: bool,
}

impl ChatFlags<'_> {
    /// The ones that were typed, by name, in the order `--help` lists them.
    pub(super) fn typed(&self) -> Vec<&'static str> {
        let mut typed = Vec::new();
        if self.context.ctx_size.is_some() {
            typed.push("--ctx-size");
        }
        if self.context.mlock {
            typed.push("--mlock");
        }
        if self.options.jinja {
            typed.push("--jinja");
        }
        if self.sampling.clone().into_override().is_some() {
            typed.push("sampling flags");
        }
        if self.profile {
            typed.push("a profile");
        }
        if self.mtp.mtp_draft_n_max.is_some() {
            typed.push("--mtp-draft-n-max");
        }
        if self.mtp.mtp_draft_p_min.is_some() {
            typed.push("--mtp-draft-p-min");
        }
        typed
    }
}

/// What is said of `typed` chat flags on the image model `name`.
pub(super) fn chat_flags_refusal(name: &str, typed: &[&str]) -> String {
    format!(
        "'{name}' is an image model: {} set a chat model's launch and do not apply to it. \
         Run 'gglib serve {name}' without them.",
        typed.join(", ")
    )
}

/// Serve the image model `model`: refuse chat flags, ensure `sd-server`,
/// start the proxy pinned to it, load it once and attach the dashboard.
pub(super) async fn serve_image(
    ctx: &CliContext,
    model: &Model,
    flags: &ChatFlags<'_>,
    cache: &CacheArgs,
    access: AccessArgs,
) -> Result<()> {
    let typed = flags.typed();
    if !typed.is_empty() {
        bail!(chat_flags_refusal(&model.name, &typed));
    }
    crate::handlers::config::sd_ensure::ensure_installed().await?;

    crate::presentation::style::print_info_banner("Info", "\u{2139}\u{fe0f}");
    for line in banner_lines(model) {
        eprintln!("{line}");
    }
    crate::presentation::style::print_banner_close();

    let settings = ctx.app.settings().get().await?;
    let options = flags.options;
    // The cascade runs for the proxy's own settings (host, port, cache,
    // access); the model's launch options it would resolve are a chat
    // model's, so the pin carries none.
    let plan = plan_pinned_launch(
        model,
        &settings,
        &StartServerRequest::default(),
        ProxyGlobals {
            host: Some(options.host.clone()),
            proxy_port: options.port,
            cache_enabled: cache.cache,
            slot_dir: cache.slot_dir.clone(),
            api_key: access.api_key.clone(),
            allowed_hosts: access.allowed_hosts.clone(),
            ..ProxyGlobals::default()
        },
    );
    let pinned = PinnedSpec {
        launch_overrides: ServerConfigOptions::default(),
        ..plan.pinned
    };
    let body = start_body(
        plan.unified.to_proxy_config(),
        pinned,
        options.port,
        cache,
        None,
    );

    let handle = daemon_client::ensure_daemon(ctx).await?;
    let proxy_port = crate::handlers::inference::proxy::start_on(&handle, &body, &settings).await?;
    let key = auth::proxy_key(ctx, access.api_key.clone()).await;
    let client = gglib_proxy::loopback::client();

    eprintln!("  Loading {} on the daemon\u{2026}", model.name);
    let loaded = match load(&client, proxy_port, &model.name, key.as_deref()).await {
        Ok(loaded) => loaded,
        Err(refused) => {
            eprintln!(
                "  The proxy is still serving on port {proxy_port}; `gglib proxy stop` stops it."
            );
            return Err(refused);
        }
    };
    eprintln!("{}", loaded_line(&loaded));
    if let Some(narration) = launch_narration(&client, proxy_port, key.as_deref()).await
        && narration.model_name == loaded.model
    {
        for line in narration_lines(&narration) {
            eprintln!("{line}");
        }
    }

    crate::handlers::inference::proxy::attach_dashboard(ctx, proxy_port, access.api_key).await
}

/// What the banner says of an image model: the model, its runtime, its
/// family, and each component its family's recipe names, in the recipe's
/// order, with the file linked or `missing`.
pub(super) fn banner_lines(model: &Model) -> Vec<String> {
    let mut lines = vec![
        format!("  Using model: {} (ID: {})", model.name, model.id),
        format!("  File: {}", model.file_path.display()),
        format!("  Runtime: {}", model.runtime().label()),
    ];
    let Some(family) = model.image_family else {
        return lines;
    };
    lines.push(format!("  Family: {}", family.label()));
    lines.extend(family.recipe().components.iter().map(|spec| {
        let file = model
            .components
            .iter()
            .find(|c| c.role == spec.role)
            .map_or_else(|| "missing".to_owned(), |c| c.path.display().to_string());
        format!("    {:<13}: {file}", spec.role.label())
    }));
    lines
}

/// Ask the proxy on `port` to load `name` once. A refusal is the proxy's
/// message, the words its error code stands for, and never the code.
pub(super) async fn load(
    client: &reqwest::Client,
    port: u16,
    name: &str,
    key: Option<&str>,
) -> Result<LoadResponse> {
    let url = format!(
        "http://127.0.0.1:{port}/v1/models/{}/load",
        path_segment(name)
    );
    let mut request = client
        .post(&url)
        .json(&serde_json::json!({}))
        .timeout(LOAD_TIMEOUT);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|e| anyhow!("Could not ask the proxy on port {port} to load '{name}': {e}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        return serde_json::from_str(&body)
            .map_err(|e| anyhow!("The proxy loaded '{name}' but its answer did not read: {e}"));
    }
    Err(serde_json::from_str::<ErrorResponse>(&body).map_or_else(
        |_| anyhow!("The proxy refused to load '{name}' ({status}): {body}"),
        |refusal| anyhow!("{}", refusal.error.message),
    ))
}

/// The line a load ends with.
pub(super) fn loaded_line(loaded: &LoadResponse) -> String {
    let how = if loaded.started {
        "loaded"
    } else {
        "already running"
    };
    format!("  \u{2705} {} is {how}", loaded.model)
}

/// The launch the proxy narrates on `/v1/proxy/status`, when it has one.
async fn launch_narration(
    client: &reqwest::Client,
    port: u16,
    key: Option<&str>,
) -> Option<LaunchNarration> {
    let mut request = client
        .get(format!("http://127.0.0.1:{port}/v1/proxy/status"))
        .timeout(Duration::from_secs(5));
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    let status: serde_json::Value = request.send().await.ok()?.json().await.ok()?;
    serde_json::from_value(status.get("launch")?.clone()).ok()
}

/// A launch's narration as lines: its headline, then each decision with
/// where it came from.
pub(super) fn narration_lines(narration: &LaunchNarration) -> Vec<String> {
    let mut lines = vec![format!("  {}", narration.headline())];
    lines.extend(narration.decisions.iter().map(|d| {
        let row = format!("    {:<9} {}", d.label, d.value);
        d.source
            .as_ref()
            .map_or_else(|| row.clone(), |source| format!("{row} ({source})"))
    }));
    lines
}

#[cfg(test)]
#[path = "serve_image_tests.rs"]
mod tests;
