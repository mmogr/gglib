//! Startup banners for [`start_proxy_standalone`](super::start_proxy_standalone).
//!
//! Split out purely for size: `start_proxy_standalone` is the sequencing
//! (config → manager → supervisor), and this ~90-line block of `println!`
//! calls was by far its largest single piece. Free functions taking plain
//! values rather than the runtime's own types, so the banner has no opinion
//! on where its inputs come from and stays trivial to read top to bottom.

use std::net::SocketAddr;
use std::path::Path;

use gglib_core::domain::InferenceConfig;

use super::params::PinnedModel;

/// Printed once, before the supervisor binds — everything known ahead of the
/// actual listen address.
#[allow(clippy::too_many_arguments)]
pub(super) fn print_starting(
    pinned: Option<&PinnedModel>,
    host: &str,
    port: u16,
    llama_base_port: u16,
    default_context: u64,
    inference_override: Option<&InferenceConfig>,
    cache_enabled: bool,
    resolved_slot_dir: Option<&Path>,
    mcp_server_count: usize,
    mcp_eager_count: usize,
    mcp_lazy_count: usize,
    mcp_manual_count: usize,
    mcp_tool_count: usize,
) {
    println!();
    match pinned {
        Some(_) => println!("  🚀 gglib serve starting (pinned)..."),
        None => println!("  🚀 gglib proxy starting..."),
    }
    println!();
    println!("  Host:            {host}");
    println!("  Port:            {port}");
    println!("  Llama base port: {llama_base_port}");
    println!("  Default context: {default_context}");
    if let Some(model) = pinned {
        // Stated up front because it changes what the endpoint will accept:
        // every other model is refused rather than swapped in.
        println!(
            "  Pinned model:    {} (id {}) — other models will be refused",
            model.name, model.id
        );
    }
    if let Some(ic) = inference_override {
        println!("  Inference override: {}", format_inference_override(ic));
    }
    print_cache_state(cache_enabled, resolved_slot_dir);
    println!(
        "  MCP servers:     {mcp_server_count} (eager: {mcp_eager_count}, lazy: {mcp_lazy_count}, manual: {mcp_manual_count})"
    );
    println!("  MCP tools:       {mcp_tool_count} (eager-started)");
    println!();
}

/// Render the sampling overrides a caller supplied on the command line.
///
/// Only the fields actually set are listed — an all-`None` config never
/// reaches here, since [`Option::is_some`] gates the call at the print site.
fn format_inference_override(ic: &InferenceConfig) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = ic.temperature {
        parts.push(format!("temperature={v}"));
    }
    if let Some(v) = ic.top_p {
        parts.push(format!("top_p={v}"));
    }
    if let Some(v) = ic.top_k {
        parts.push(format!("top_k={v}"));
    }
    if let Some(v) = ic.max_tokens {
        parts.push(format!("max_tokens={v}"));
    }
    if let Some(v) = ic.repeat_penalty {
        parts.push(format!("repeat_penalty={v}"));
    }
    if let Some(v) = ic.presence_penalty {
        parts.push(format!("presence_penalty={v}"));
    }
    if let Some(v) = ic.min_p {
        parts.push(format!("min_p={v}"));
    }
    parts.join(", ")
}

/// A `slots/` directory appears on disk the moment caching is on — worth
/// stating up front rather than letting a user discover it by accident,
/// especially in a source checkout where the default resolves inside the
/// repo itself.
fn print_cache_state(cache_enabled: bool, resolved_slot_dir: Option<&Path>) {
    match (cache_enabled, resolved_slot_dir) {
        (true, Some(dir)) => println!("  KV slot cache:   enabled ({})", dir.display()),
        (true, None) => println!("  KV slot cache:   enabled"),
        (false, _) => println!("  KV slot cache:   disabled (--cache to enable)"),
    }
}

/// Printed once the supervisor has actually bound — the pieces that depend
/// on the real listen address.
///
/// Framing is mode-aware: `gglib serve` exists for clients that *cannot*
/// switch models via `/v1/models`, so "Configure OpenWebUI" — a client that
/// can — is the wrong invitation for a pinned endpoint.
pub(super) fn print_ready(addr: SocketAddr, pinned: Option<&PinnedModel>) {
    println!("  ✓ Proxy started successfully on {addr}");
    println!();
    if pinned.is_some() {
        println!("  Point your OpenAI-compatible client at:");
    } else {
        println!("  Configure OpenWebUI:");
    }
    println!("    OpenAI API: http://{addr}/v1");
    println!("    MCP Tools:  http://{addr}/mcp");
    println!("    Dashboard:  http://{addr}/v1/proxy/status");
    println!();
    println!("  Press Ctrl+C to stop");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a snapshot test — `println!` output isn't worth pinning byte for
    /// byte — but `format_inference_override` is the one piece with real
    /// branching (which fields print, in what order), so it gets a direct
    /// assertion rather than relying on someone eyeballing terminal output.
    #[test]
    fn format_inference_override_lists_only_set_fields_in_declared_order() {
        let ic = InferenceConfig {
            temperature: Some(0.7),
            min_p: Some(0.05),
            ..Default::default()
        };
        assert_eq!(format_inference_override(&ic), "temperature=0.7, min_p=0.05");
    }

    #[test]
    fn format_inference_override_empty_config_yields_empty_string() {
        assert_eq!(format_inference_override(&InferenceConfig::default()), "");
    }
}
