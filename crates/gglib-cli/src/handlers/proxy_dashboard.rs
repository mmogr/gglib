//! `gglib proxy dashboard` — live terminal view of an already-running proxy.
//!
//! Connects to `GET /v1/proxy/status/stream` on a running `gglib proxy` (or
//! `gglib web`) instance and redraws a compact text dashboard in place: active
//! `/v1/chat/completions` connections, per-slot context-window usage from
//! llama.cpp's `/slots` endpoint, and a running request count.
//!
//! ## Shared `SlotSnapshot`, local `DashboardSnapshot`
//!
//! [`DashboardSnapshot`] and friends stay a local, `Deserialize`-only mirror
//! of the JSON shape produced by `gglib_proxy::dashboard::DashboardSnapshot`
//! — the same relationship the TypeScript frontend has to that same
//! endpoint. Unknown fields are ignored by default (no `deny_unknown_fields`),
//! so this client tolerates additive changes to the server-side contract.
//!
//! `slots`, though, reuses [`gglib_proxy::slots::SlotSnapshot`] directly
//! rather than a hand-copied mirror: llama.cpp's `/slots` schema has shifted
//! shape more than once, and every shift previously meant editing the same
//! `tokens_in_use()` fallback chain in two crates. `gglib-cli` already
//! depends on `gglib-axum` — the one documented exception to
//! CONTRIBUTING.md's surface-crate isolation rule, needed for `gglib web` —
//! and was already pulling in `gglib-proxy` transitively via `gglib-runtime`
//! and `gglib-app-services`, so a direct `gglib-proxy` dependency here adds
//! nothing new to the build graph; it just lets this module use the
//! canonical parser instead of maintaining its own.
//!
//! ## Redraw strategy: cursor movement, not raw mode
//!
//! Earlier CLI work in this crate (see
//! [`crate::handlers::model::download::interactive`]) already established
//! that `crossterm::terminal::enable_raw_mode()` breaks `println!`-based
//! redraws (it disables `OPOST`, so `\n` stops returning the cursor to column
//! 0). This module never touches raw mode. Instead, each frame after the
//! first moves the cursor up by the previous frame's *physical row* count
//! (see [`visual_row_count`]) and clears everything below before printing
//! the next frame — plain `crossterm::cursor`/`terminal` commands in normal
//! (cooked) mode, which compose fine with ordinary `print!`/`println!`.
//! Cooked mode means a line longer than the terminal's width auto-wraps onto
//! an extra physical row, which is exactly what `visual_row_count` accounts
//! for when computing how far to move the cursor up on the next tick. When
//! stdout is not a TTY (piped output, CI), frames are printed sequentially
//! instead, since there is no cursor to move.
//!
//! ## Shutdown
//!
//! `Ctrl+C` is raced directly against each stream-chunk read via
//! `tokio::select!`, so it is handled between chunks rather than only after a
//! full frame arrives. [`TerminalGuard`] hides the cursor for the duration of
//! the dashboard and unconditionally restores it (and prints a trailing
//! newline) on drop — including on the `Ctrl+C` path, an early `?` return, or
//! a panic — so the terminal is never left in a half-drawn state. Dropping
//! the `reqwest` response stream (which happens automatically once
//! `execute()` returns) closes the underlying SSE connection.

use std::io::{IsTerminal, Write, stdout};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use crossterm::{cursor, execute, terminal};
use futures_util::StreamExt;
use gglib_proxy::slots::SlotSnapshot;
use serde::Deserialize;

/// Width (in bar cells) of every progress bar drawn by this dashboard.
const BAR_WIDTH: usize = 20;

/// Fallback terminal width (columns) used when stdout isn't a TTY or
/// `crossterm::terminal::size()` fails to report one. Matches the common
/// default terminal width so output still looks reasonable when piped.
const DEFAULT_TERM_WIDTH: u16 = 80;

// =============================================================================
// Local mirror of the server's JSON contract (see module docs)
// =============================================================================

#[derive(Debug, Deserialize)]
struct DashboardSnapshot {
    active_connections: Vec<ActiveConnectionSnapshot>,
    slots_available: bool,
    #[serde(default)]
    slots: Vec<SlotSnapshot>,
    #[serde(default)]
    slots_status: Option<String>,
    total_requests: u64,
    /// Prompt-cache configuration and reuse. `None` until the first request
    /// resolves a model, and on a proxy older than this field.
    #[serde(default)]
    cache: Option<CacheStatus>,
    /// Agent-path prompt-cache reuse (GUI chat) — a separate
    /// population from [`CacheStatus::usage`]. Top-level and always present,
    /// since it does not depend on a resolved model; `default` on a proxy older
    /// than this field.
    #[serde(default)]
    agent_usage: CacheUsage,
    /// VRAM residency and the admission queue. `default` on a proxy older than
    /// this field, which renders as an empty resident set.
    #[serde(default)]
    admission: AdmissionSnapshot,
}

/// Mirror of `gglib_core::domain::AdmissionSnapshot`.
#[derive(Debug, Default, Deserialize)]
struct AdmissionSnapshot {
    #[serde(default)]
    slots: Vec<ResidentSlotSnapshot>,
    #[serde(default)]
    queued: Vec<QueuedModelSnapshot>,
    #[serde(default)]
    total_swaps: u64,
    #[serde(default)]
    secondary_slot: SecondarySlotStatus,
}

/// Mirror of `gglib_core::domain::ResidentSlotSnapshot`.
#[derive(Debug, Deserialize)]
struct ResidentSlotSnapshot {
    model_name: String,
    #[serde(default)]
    inflight: u32,
    #[serde(default)]
    is_primary: bool,
    #[serde(default)]
    resident_for_secs: u64,
}

/// Mirror of `gglib_core::domain::QueuedModelSnapshot`.
#[derive(Debug, Deserialize)]
struct QueuedModelSnapshot {
    model_name: String,
    #[serde(default)]
    waiting: usize,
    #[serde(default)]
    oldest_wait_ms: u64,
}

/// Mirror of `gglib_core::domain::SecondarySlotStatus`.
#[derive(Debug, Default, Deserialize)]
struct SecondarySlotStatus {
    #[serde(default)]
    detail: String,
}

/// Mirror of `gglib_proxy::dashboard::CacheStatus`.
#[derive(Debug, Deserialize)]
struct CacheStatus {
    #[serde(default)]
    disk_enabled: bool,
    #[serde(default)]
    disk_suppressed_for_model: bool,
    #[serde(default)]
    ram_budget_mb: Option<u64>,
    #[serde(default)]
    ram_state: String,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default)]
    usage: CacheUsage,
}

/// Mirror of `gglib_core::cache_metrics::CacheUsage`.
///
/// Raw counts only — the server publishes no derived "time saved" figure, so
/// there is none to render here either.
#[derive(Debug, Default, Deserialize)]
struct CacheUsage {
    #[serde(default)]
    reporting_requests: u64,
    #[serde(default)]
    unreported_requests: u64,
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    cached_tokens: u64,
    #[serde(default)]
    last_prompt_tokens: Option<u32>,
    #[serde(default)]
    last_cached_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ActiveConnectionSnapshot {
    model_name: String,
    started_at_secs: u64,
    phase: ConnectionPhase,
    #[serde(default)]
    prompt_processed: Option<u32>,
    #[serde(default)]
    prompt_total: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionPhase {
    Queued,
    ProcessingPrompt,
    Generating,
}

impl ConnectionPhase {
    fn label(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::ProcessingPrompt => "prompt",
            Self::Generating => "generating",
        }
    }
}

// =============================================================================
// Pure rendering helpers (unit-tested below, no IO)
// =============================================================================

/// Render a `[███░░░] NN%` bar. `total == 0` renders an empty bar at 0%
/// rather than dividing by zero — used for every gauge in this dashboard so
/// the bar-drawing logic exists in exactly one place.
fn progress_bar(filled: u64, total: u64, width: usize) -> String {
    let fraction = if total == 0 {
        0.0
    } else {
        (filled as f64 / total as f64).clamp(0.0, 1.0)
    };
    let filled_cells = ((fraction * width as f64).round() as usize).min(width);
    let empty_cells = width - filled_cells;
    format!(
        "[{}{}] {:>3}%",
        "\u{2588}".repeat(filled_cells),
        "\u{2591}".repeat(empty_cells),
        (fraction * 100.0).round() as u32
    )
}

/// A span of seconds as `Ns`, or `Nm Ss` past one minute.
fn format_duration_secs(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

/// Seconds elapsed since a Unix timestamp, formatted as `Ns` (or `Nm Ss` past
/// one minute). Never panics: a clock skew that makes `started_at_secs` look
/// like it's in the future just renders as `0s`.
fn format_elapsed_secs(started_at_secs: u64) -> String {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(started_at_secs);
    format_duration_secs(now_secs.saturating_sub(started_at_secs))
}

/// Build the full multi-line dashboard frame for one snapshot. Pure text
/// generation — no IO — so it's testable without a terminal or network.
///
/// `term_width` is used to pre-truncate the two fields whose content is
/// otherwise unbounded (the `/slots` unreachable reason and cache warnings,
/// both server-phrased strings that can easily exceed 100 characters), so
/// the frame stays legible even before wrapping is accounted for. Every
/// other line in the frame is fixed-width by construction, but may still
/// wrap on a narrow enough terminal — the caller does not rely on `frame`
/// having exactly one physical row per logical line; see
/// [`visual_row_count`], which is what the redraw loop actually uses to
/// compute how far to move the cursor up.
fn render_frame(url: &str, snapshot: &DashboardSnapshot, term_width: u16) -> String {
    let mut out = String::new();
    out.push_str(&format!("gglib proxy dashboard — {url}\n"));
    out.push_str("(Ctrl+C to exit)\n\n");

    out.push_str(&format!(
        "Active connections ({})\n",
        snapshot.active_connections.len()
    ));
    if snapshot.active_connections.is_empty() {
        out.push_str("  (none)\n");
    }
    for conn in &snapshot.active_connections {
        let bar = match (conn.prompt_processed, conn.prompt_total) {
            (Some(processed), Some(total)) => {
                progress_bar(u64::from(processed), u64::from(total), BAR_WIDTH)
            }
            _ => progress_bar(0, 0, BAR_WIDTH),
        };
        out.push_str(&format!(
            "  {:<24} {:<11} {}  {}\n",
            truncate(&conn.model_name, 24),
            conn.phase.label(),
            bar,
            format_elapsed_secs(conn.started_at_secs)
        ));
    }

    out.push('\n');
    out.push_str("Slots (llama.cpp /slots)\n");
    if !snapshot.slots_available {
        let reason = snapshot.slots_status.as_deref().unwrap_or("unavailable");
        // "  " prefix takes 2 columns — clip so the whole line fits in
        // one physical row regardless of terminal width.
        let max_reason_chars = usize::from(term_width.saturating_sub(2));
        out.push_str(&format!("  {}\n", truncate(reason, max_reason_chars)));
    } else if snapshot.slots.is_empty() {
        out.push_str("  (no slots reported)\n");
    } else {
        for slot in &snapshot.slots {
            let bar = match (slot.tokens_in_use(), slot.n_ctx) {
                (Some(used), Some(ctx)) => progress_bar(used, ctx, BAR_WIDTH),
                _ => progress_bar(0, 0, BAR_WIDTH),
            };
            out.push_str(&format!("  slot {:<3} {}\n", slot.id, bar));
        }
    }

    out.push('\n');
    out.push_str(&render_admission_section(&snapshot.admission, term_width));

    out.push('\n');
    out.push_str("Prompt cache\n");
    match &snapshot.cache {
        None => out.push_str("  (no model resolved yet)\n"),
        Some(cache) => out.push_str(&render_cache_section(cache, term_width)),
    }

    // A separate population from the proxied figure above: GUI-chat
    // runs talk to llama-server directly, so their reuse profile is nothing
    // like a user's conversation and must not be averaged into it.
    out.push('\n');
    out.push_str("Agent cache (GUI chat)\n");
    out.push_str(&render_usage_rows(&snapshot.agent_usage));

    out.push('\n');
    out.push_str(&format!(
        "Total requests served: {}\n",
        snapshot.total_requests
    ));
    out
}

/// Render the VRAM residency and admission-queue section.
///
/// Placed directly after the connection list because it answers the question
/// that list raises: a request sitting at `queued` is waiting either on
/// llama.cpp or on a model swap, and only this section can say which.
///
/// The second-slot line is always printed, even when nothing is co-loaded. An
/// idle second slot on a machine with free VRAM is the case a user is most
/// likely to read as a bug, so the server sends the reason and this prints it.
fn render_admission_section(admission: &AdmissionSnapshot, term_width: u16) -> String {
    let mut out = String::new();
    out.push_str("VRAM residency\n");

    if admission.slots.is_empty() {
        out.push_str("  (no model loaded)\n");
    }
    for slot in &admission.slots {
        let role = if slot.is_primary {
            "primary"
        } else {
            "secondary"
        };
        let activity = if slot.inflight > 0 {
            format!("{} in flight", slot.inflight)
        } else {
            "idle".to_string()
        };
        out.push_str(&format!(
            "  {:<24} {:<10} {:<14} {}\n",
            truncate(&slot.model_name, 24),
            role,
            activity,
            format_duration_secs(slot.resident_for_secs),
        ));
    }

    if !admission.secondary_slot.detail.is_empty() {
        // "  " prefix takes 2 columns; clip so the explanation stays on one
        // physical row whatever the terminal width, matching how the slots
        // error line is handled above.
        let max_chars = usize::from(term_width.saturating_sub(2));
        out.push_str(&format!(
            "  {}\n",
            truncate(&admission.secondary_slot.detail, max_chars)
        ));
    }

    for queued in &admission.queued {
        out.push_str(&format!(
            "  {:<24} {} waiting, oldest {}\n",
            truncate(&queued.model_name, 24),
            queued.waiting,
            format_duration_secs(queued.oldest_wait_ms / 1000),
        ));
    }

    // Printed unconditionally, and next to nothing else: a swap count on its
    // own reads as a cost, but next to a queue that is draining it reads as
    // how much the batching saved.
    out.push_str(&format!(
        "  {:<24} {}\n",
        "Model swaps", admission.total_swaps
    ));
    out
}

/// Number of physical terminal rows `frame` will occupy when printed at
/// `term_width` columns, accounting for lines that auto-wrap. Mirrors the
/// terminal's own wrapping behavior in cooked mode: a line of `w` columns
/// takes `ceil(w / term_width)` rows (minimum 1, even for an empty line).
///
/// Used instead of a bare logical-line count (`frame.lines().count()`) when
/// tracking how far to move the cursor up on the next redraw — see the
/// module's "Redraw strategy" doc comment for why an undercount there
/// corrupts the display.
fn visual_row_count(frame: &str, term_width: u16) -> u16 {
    let cols = term_width.max(1);
    frame
        .lines()
        .map(|line| {
            let width = line.chars().count() as u16;
            width.div_ceil(cols).max(1)
        })
        .fold(0u16, |acc, rows| acc.saturating_add(rows))
}

/// Render the reuse rows shared by the proxied and agent-path cache sections.
///
/// Every figure is one the upstream measured. There is deliberately no
/// "time saved" line: reuse is exact, but what it saved depends on a prefill
/// that never ran — see `gglib_core::cache_metrics` for the same reasoning
/// on the server side.
fn render_usage_rows(usage: &CacheUsage) -> String {
    let mut out = String::new();

    if usage.reporting_requests == 0 {
        out.push_str("  (no cache activity recorded yet)\n");
    } else {
        out.push_str(&format!(
            "  {:<14} {} of {} prompt tokens ({} requests)\n",
            "Reused",
            thousands(usage.cached_tokens),
            thousands(usage.prompt_tokens),
            thousands(usage.reporting_requests),
        ));
        if let (Some(last_cached), Some(last_prompt)) =
            (usage.last_cached_tokens, usage.last_prompt_tokens)
        {
            out.push_str(&format!(
                "  {:<14} {} of {} tokens from cache\n",
                "Last request",
                thousands(u64::from(last_cached)),
                thousands(u64::from(last_prompt)),
            ));
        }
    }

    // Only shown when it's non-zero: on a current llama.cpp every request
    // reports, so a permanent "0" row would be noise.
    if usage.unreported_requests > 0 {
        out.push_str(&format!(
            "  {:<14} {}\n",
            "No cache data",
            thousands(usage.unreported_requests),
        ));
    }

    out
}

/// Render the body of the prompt-cache section (rows only, no header): the
/// proxy's cache warnings and config framing the shared reuse rows.
fn render_cache_section(cache: &CacheStatus, term_width: u16) -> String {
    let mut out = String::new();

    // Warnings are pre-phrased for display by the server; clip each to one
    // physical row, matching how `slots_status` is handled above.
    let max_warning_chars = usize::from(term_width.saturating_sub(4));
    for warning in &cache.warnings {
        out.push_str(&format!("  ! {}\n", truncate(warning, max_warning_chars)));
    }

    out.push_str(&render_usage_rows(&cache.usage));

    let disk = if !cache.disk_enabled {
        "off"
    } else if cache.disk_suppressed_for_model {
        "off for this model"
    } else {
        "on"
    };
    match ram_budget_label(cache) {
        Some(budget) => out.push_str(&format!("  RAM budget: {budget} · disk: {disk}\n")),
        None => out.push_str(&format!("  disk: {disk}\n")),
    }

    out
}

/// Human-readable summary of how the `--cache-ram` budget resolved.
///
/// `None` for `llama_default`, where gglib emitted no flag and so has no
/// figure of its own to report.
fn ram_budget_label(cache: &CacheStatus) -> Option<String> {
    match cache.ram_state.as_str() {
        "healthy" | "low" => cache
            .ram_budget_mb
            .map(|mb| format!("{} MiB", thousands(mb))),
        "disabled_by_user" => Some("disabled".to_string()),
        "disabled_insufficient_ram" => Some("unavailable (not enough memory)".to_string()),
        // Covers `llama_default` and any state a newer server adds.
        _ => None,
    }
}

/// Format an integer with `,` thousands separators, so six-figure token
/// counts stay readable in a dense terminal frame.
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Truncate to at most `max_chars` characters, appending `…` when cut short.
/// Keeps model-name columns from wrapping the frame onto extra lines.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        truncated.push('\u{2026}');
        truncated
    }
}

/// Extract complete SSE `data:` payloads from a growing byte buffer.
///
/// Splits on the blank-line event terminator (`"\n\n"`), joining any
/// `data:`-prefixed lines within an event (gglib-sse always emits single-line
/// JSON, but multi-line `data:` framing is handled per spec anyway). Comment
/// lines (leading `:`, used for SSE keep-alives) and events with no `data:`
/// line are silently skipped. Any trailing partial event is left in `buffer`
/// for the next call once more bytes arrive.
fn drain_sse_events(buffer: &mut String) -> Vec<String> {
    let mut payloads = Vec::new();
    while let Some(idx) = buffer.find("\n\n") {
        let event: String = buffer.drain(..idx + 2).collect();
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if !data.is_empty() {
            payloads.push(data);
        }
    }
    payloads
}

// =============================================================================
// Terminal state guard
// =============================================================================

/// Hides the cursor for the lifetime of the dashboard and unconditionally
/// restores it (plus a trailing newline so the shell prompt doesn't land mid-
/// line) on drop — covering the `Ctrl+C` path, an early `?` return, and an
/// unwinding panic alike. A no-op when stdout isn't a TTY.
struct TerminalGuard {
    is_tty: bool,
}

impl TerminalGuard {
    fn new(is_tty: bool) -> Self {
        if is_tty {
            let _ = execute!(stdout(), cursor::Hide);
        }
        Self { is_tty }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.is_tty {
            let _ = execute!(stdout(), cursor::Show);
            println!();
        }
    }
}

// =============================================================================
// Entry point
// =============================================================================

/// Execute `gglib proxy dashboard`.
///
/// Connects to `http://{host}:{port}/v1/proxy/status/stream`, prints the
/// hydration snapshot immediately, then redraws in place on every subsequent
/// tick until `Ctrl+C` is pressed or the connection is closed by the server.
pub async fn execute(host: String, port: u16, api_key: Option<&str>) -> Result<()> {
    let url = format!("http://{host}:{port}/v1/proxy/status/stream");

    let mut request = reqwest::Client::new().get(&url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to connect to {url} — is the proxy running?"))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!(
            "the proxy at {host}:{port} requires an API key. Pass --api-key, set \
             GGLIB_API_KEY, or store one with `gglib config settings set --proxy-api-key <key>`."
        );
    }
    if !response.status().is_success() {
        anyhow::bail!(
            "proxy dashboard stream at {url} returned HTTP {}",
            response.status()
        );
    }

    let is_tty = stdout().is_terminal();
    let _terminal_guard = TerminalGuard::new(is_tty);

    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut previous_frame_lines = 0u16;

    loop {
        tokio::select! {
            // Checked first on every loop iteration (top-to-bottom `select!`
            // polling order) so a pending Ctrl+C is never left behind an
            // in-flight chunk read — instant response as required.
            biased;

            _ = tokio::signal::ctrl_c() => {
                return Ok(());
            }

            chunk = byte_stream.next() => {
                let Some(chunk) = chunk else {
                    // Server closed the connection.
                    return Ok(());
                };
                let chunk = chunk.context("error reading proxy dashboard stream")?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                for payload in drain_sse_events(&mut buffer) {
                    let snapshot: DashboardSnapshot = match serde_json::from_str(&payload) {
                        Ok(snapshot) => snapshot,
                        Err(e) => {
                            tracing::debug!("skipping unparseable dashboard event: {e}");
                            continue;
                        }
                    };

                    // Re-check on every tick (not just once) so a mid-session
                    // terminal resize is picked up rather than rendering
                    // against a stale width.
                    let term_width = terminal::size()
                        .map(|(cols, _rows)| cols)
                        .unwrap_or(DEFAULT_TERM_WIDTH);
                    let frame = render_frame(&url, &snapshot, term_width);
                    if is_tty {
                        let mut out = stdout();
                        execute!(
                            out,
                            cursor::MoveUp(previous_frame_lines),
                            terminal::Clear(terminal::ClearType::FromCursorDown)
                        )?;
                        write!(out, "{frame}")?;
                        out.flush()?;
                        previous_frame_lines = visual_row_count(&frame, term_width);
                    } else {
                        print!("{frame}");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_renders_full_and_empty() {
        assert_eq!(
            progress_bar(0, 100, 10),
            "[\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}\u{2591}]   0%"
        );
        assert_eq!(
            progress_bar(100, 100, 10),
            "[\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}] 100%"
        );
    }

    #[test]
    fn progress_bar_zero_total_is_empty_not_a_panic() {
        assert_eq!(progress_bar(5, 0, 10), progress_bar(0, 100, 10));
    }

    #[test]
    fn progress_bar_rounds_to_nearest_cell() {
        // 5/10 = 50% of a 4-cell bar -> 2 filled cells.
        assert_eq!(
            progress_bar(5, 10, 4),
            "[\u{2588}\u{2588}\u{2591}\u{2591}]  50%"
        );
    }

    #[test]
    fn format_elapsed_secs_under_a_minute() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_elapsed_secs(now - 5), "5s");
    }

    #[test]
    fn format_elapsed_secs_over_a_minute() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(format_elapsed_secs(now - 125), "2m 5s");
    }

    #[test]
    fn truncate_leaves_short_strings_unchanged() {
        assert_eq!(truncate("qwen3", 24), "qwen3");
    }

    #[test]
    fn truncate_cuts_long_strings_with_ellipsis() {
        let result = truncate("a-very-long-model-name-that-overflows", 10);
        assert_eq!(result.chars().count(), 10);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn drain_sse_events_extracts_single_complete_event() {
        let mut buffer = String::from("data: {\"a\":1}\n\n");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}"]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn drain_sse_events_leaves_partial_event_buffered() {
        let mut buffer = String::from("data: {\"a\":1}\n\ndata: {\"a\":2}");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}"]);
        assert_eq!(buffer, "data: {\"a\":2}");
    }

    #[test]
    fn drain_sse_events_skips_keepalive_comments() {
        let mut buffer = String::from(": ping\n\ndata: {\"a\":1}\n\n");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}"]);
    }

    #[test]
    fn drain_sse_events_handles_multiple_events_in_one_chunk() {
        let mut buffer = String::from("data: {\"a\":1}\n\ndata: {\"a\":2}\n\n");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}", "{\"a\":2}"]);
    }

    #[test]
    fn render_frame_shows_placeholder_when_no_connections() {
        let snapshot = DashboardSnapshot {
            active_connections: vec![],
            slots_available: false,
            slots: vec![],
            slots_status: Some("disabled upstream (--no-slots)".to_string()),
            total_requests: 0,
            cache: None,
            agent_usage: CacheUsage::default(),
            admission: AdmissionSnapshot::default(),
        };
        let frame = render_frame(
            "http://127.0.0.1:8080/v1/proxy/status/stream",
            &snapshot,
            DEFAULT_TERM_WIDTH,
        );
        assert!(frame.contains("(none)"));
        assert!(frame.contains("disabled upstream (--no-slots)"));
        assert!(frame.contains("Total requests served: 0"));
    }

    #[test]
    fn render_frame_shows_connection_and_slot_bars() {
        let snapshot = DashboardSnapshot {
            active_connections: vec![ActiveConnectionSnapshot {
                model_name: "qwen3-30b".to_string(),
                started_at_secs: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                phase: ConnectionPhase::ProcessingPrompt,
                prompt_processed: Some(50),
                prompt_total: Some(100),
            }],
            slots_available: true,
            slots: vec![
                serde_json::from_str(r#"{"id": 0, "n_ctx": 4096, "n_past": 2048}"#)
                    .expect("should parse"),
            ],
            slots_status: None,
            total_requests: 3,
            cache: None,
            agent_usage: CacheUsage::default(),
            admission: AdmissionSnapshot::default(),
        };
        let frame = render_frame(
            "http://127.0.0.1:8080/v1/proxy/status/stream",
            &snapshot,
            DEFAULT_TERM_WIDTH,
        );
        assert!(frame.contains("qwen3-30b"));
        assert!(frame.contains("prompt"));
        assert!(frame.contains("50%")); // 50/100 prompt progress
        assert!(frame.contains("slot 0"));
        assert!(frame.contains("Total requests served: 3"));
    }

    #[test]
    fn render_frame_truncates_long_slots_error_to_fit_terminal_width() {
        // A realistic reqwest connect-error string easily exceeds 100 chars
        // — e.g. "error sending request for url (http://127.0.0.1:5500/slots):
        // error trying to connect: tcp connect error: Connection refused (os
        // error 61)". This still confirms the pre-truncation keeps the line
        // within one row, on top of the general wrap-aware row counting in
        // `visual_row_count`.
        let long_reason = "error sending request for url (http://127.0.0.1:5500/slots): "
            .to_string()
            + &"error trying to connect: tcp connect error: Connection refused ".repeat(3);
        let snapshot = DashboardSnapshot {
            active_connections: vec![],
            slots_available: false,
            slots: vec![],
            slots_status: Some(long_reason.clone()),
            total_requests: 0,
            cache: None,
            agent_usage: CacheUsage::default(),
            admission: AdmissionSnapshot::default(),
        };
        let width = 80u16;
        let frame = render_frame(
            "http://127.0.0.1:8080/v1/proxy/status/stream",
            &snapshot,
            width,
        );

        assert!(
            long_reason.chars().count() as u16 > width,
            "test fixture must actually exceed the terminal width"
        );
        for line in frame.lines() {
            assert!(
                line.chars().count() <= width as usize,
                "line exceeds terminal width ({} > {width}): {line:?}",
                line.chars().count()
            );
        }
        assert!(
            frame.contains('\u{2026}'),
            "long reason should be truncated with an ellipsis"
        );
    }

    #[test]
    fn visual_row_count_matches_logical_lines_when_nothing_wraps() {
        let frame = "gglib proxy dashboard\n(Ctrl+C to exit)\n\nTotal requests served: 0\n";
        assert_eq!(
            visual_row_count(frame, DEFAULT_TERM_WIDTH),
            frame.lines().count() as u16
        );
    }

    #[test]
    fn visual_row_count_counts_a_wrapped_line_as_multiple_rows() {
        let frame = format!("{}\n", "x".repeat(150));
        assert_eq!(visual_row_count(&frame, 80), 2);
    }

    /// Reproduces the reported bug directly: at a narrow terminal width the
    /// unguarded header line (fixed content, no truncation applied to it)
    /// is long enough to wrap onto a second physical row. The old
    /// `frame.lines().count()` redraw math would undercount here — proving
    /// exactly the undershoot that made the dashboard drift/repeat down a
    /// narrow terminal instead of redrawing in place.
    #[test]
    fn visual_row_count_exceeds_naive_line_count_on_a_narrow_terminal() {
        let snapshot = DashboardSnapshot {
            active_connections: vec![],
            slots_available: false,
            slots: vec![],
            slots_status: None,
            total_requests: 0,
            cache: None,
            agent_usage: CacheUsage::default(),
            admission: AdmissionSnapshot::default(),
        };
        let term_width = 40u16;
        let frame = render_frame(
            "http://127.0.0.1:8080/v1/proxy/status/stream",
            &snapshot,
            term_width,
        );

        let naive_count = frame.lines().count() as u16;
        let accurate_count = visual_row_count(&frame, term_width);
        assert!(
            accurate_count > naive_count,
            "expected wrapping to be detected: naive={naive_count} accurate={accurate_count}"
        );
    }

    // ── Prompt cache section ─────────────────────────────────────────────

    fn cache_status(usage: CacheUsage) -> CacheStatus {
        CacheStatus {
            disk_enabled: true,
            disk_suppressed_for_model: false,
            ram_budget_mb: Some(70_008),
            ram_state: "healthy".to_string(),
            warnings: vec![],
            usage,
        }
    }

    fn frame_with_cache(cache: Option<CacheStatus>) -> String {
        let snapshot = DashboardSnapshot {
            active_connections: vec![],
            slots_available: false,
            slots: vec![],
            slots_status: None,
            total_requests: 3,
            cache,
            agent_usage: CacheUsage::default(),
            admission: AdmissionSnapshot::default(),
        };
        render_frame("http://127.0.0.1:8080", &snapshot, DEFAULT_TERM_WIDTH)
    }

    fn frame_with_agent_usage(agent_usage: CacheUsage) -> String {
        let snapshot = DashboardSnapshot {
            active_connections: vec![],
            slots_available: false,
            slots: vec![],
            slots_status: None,
            total_requests: 0,
            cache: None,
            agent_usage,
            admission: AdmissionSnapshot::default(),
        };
        render_frame("http://127.0.0.1:8080", &snapshot, DEFAULT_TERM_WIDTH)
    }

    fn frame_with_admission(admission: AdmissionSnapshot) -> String {
        let snapshot = DashboardSnapshot {
            active_connections: vec![],
            slots_available: false,
            slots: vec![],
            slots_status: None,
            total_requests: 0,
            cache: None,
            agent_usage: CacheUsage::default(),
            admission,
        };
        render_frame("http://127.0.0.1:8080", &snapshot, DEFAULT_TERM_WIDTH)
    }

    /// A proxy that predates admission control still renders — the section
    /// reports an empty resident set rather than the frame losing a panel.
    #[test]
    fn admission_section_renders_an_empty_resident_set() {
        let frame = frame_with_admission(AdmissionSnapshot::default());
        assert!(frame.contains("VRAM residency"), "{frame}");
        assert!(frame.contains("(no model loaded)"), "{frame}");
        assert!(frame.contains("Model swaps"), "{frame}");
    }

    #[test]
    fn admission_section_names_each_resident_and_its_role() {
        let frame = frame_with_admission(AdmissionSnapshot {
            slots: vec![
                ResidentSlotSnapshot {
                    model_name: "qwen-coder".to_string(),
                    inflight: 2,
                    is_primary: true,
                    resident_for_secs: 95,
                },
                ResidentSlotSnapshot {
                    model_name: "nomic-embed".to_string(),
                    inflight: 0,
                    is_primary: false,
                    resident_for_secs: 30,
                },
            ],
            ..Default::default()
        });

        assert!(frame.contains("qwen-coder"), "{frame}");
        assert!(frame.contains("primary"), "{frame}");
        assert!(frame.contains("2 in flight"), "{frame}");
        assert!(frame.contains("1m 35s"), "{frame}");
        assert!(frame.contains("nomic-embed"), "{frame}");
        assert!(frame.contains("secondary"), "{frame}");
        assert!(frame.contains("idle"), "{frame}");
    }

    /// The whole reason the server sends prose: an idle second slot has to
    /// explain itself, or a user with free VRAM reads it as a bug.
    #[test]
    fn admission_section_prints_the_second_slot_explanation() {
        let frame = frame_with_admission(AdmissionSnapshot {
            secondary_slot: SecondarySlotStatus {
                detail: "Not enough free VRAM to keep a second model loaded.".to_string(),
            },
            ..Default::default()
        });

        assert!(frame.contains("Not enough free VRAM"), "{frame}");
    }

    #[test]
    fn admission_section_reports_queue_depth_and_the_oldest_wait() {
        let frame = frame_with_admission(AdmissionSnapshot {
            queued: vec![QueuedModelSnapshot {
                model_name: "nomic-embed".to_string(),
                waiting: 4,
                oldest_wait_ms: 95_000,
            }],
            total_swaps: 3,
            ..Default::default()
        });

        assert!(frame.contains("4 waiting"), "{frame}");
        assert!(frame.contains("oldest 1m 35s"), "{frame}");
        assert!(frame.contains("Model swaps"), "{frame}");
        assert!(frame.contains('3'), "{frame}");
    }

    /// A server-phrased explanation can be arbitrarily long; it must not wrap
    /// and break the cursor arithmetic the redraw depends on.
    #[test]
    fn a_long_second_slot_explanation_is_clipped_to_the_terminal() {
        let detail = "x".repeat(400);
        let snapshot = DashboardSnapshot {
            active_connections: vec![],
            slots_available: false,
            slots: vec![],
            slots_status: None,
            total_requests: 0,
            cache: None,
            agent_usage: CacheUsage::default(),
            admission: AdmissionSnapshot {
                secondary_slot: SecondarySlotStatus { detail },
                ..Default::default()
            },
        };

        let frame = render_frame("http://127.0.0.1:8080", &snapshot, 80);
        for line in frame.lines() {
            assert!(
                line.chars().count() <= 80,
                "line exceeds the terminal width: {line}"
            );
        }
    }

    #[test]
    fn cache_section_reports_when_no_model_has_resolved() {
        let frame = frame_with_cache(None);
        assert!(frame.contains("Prompt cache"));
        assert!(frame.contains("(no model resolved yet)"), "{frame}");
    }

    /// The agent population renders in its own section, even when no proxied
    /// model has resolved (so the "Prompt cache" section shows the placeholder).
    #[test]
    fn agent_cache_section_renders_its_own_population() {
        let idle = frame_with_agent_usage(CacheUsage::default());
        assert!(idle.contains("Agent cache (GUI chat)"), "{idle}");
        assert!(idle.contains("(no cache activity recorded yet)"), "{idle}");
        // The proxied section is independent and still shows its placeholder.
        assert!(idle.contains("(no model resolved yet)"), "{idle}");

        let active = frame_with_agent_usage(CacheUsage {
            reporting_requests: 4,
            prompt_tokens: 12_000,
            cached_tokens: 9_800,
            last_prompt_tokens: Some(3_000),
            last_cached_tokens: Some(2_500),
            ..CacheUsage::default()
        });
        assert!(active.contains("Agent cache (GUI chat)"), "{active}");
        assert!(active.contains("9,800 of 12,000 prompt tokens"), "{active}");
        assert!(
            active.contains("2,500 of 3,000 tokens from cache"),
            "{active}"
        );
    }

    #[test]
    fn cache_section_shows_reuse_totals_with_separators() {
        let frame = frame_with_cache(Some(cache_status(CacheUsage {
            reporting_requests: 3,
            prompt_tokens: 30_342,
            cached_tokens: 29_450,
            last_prompt_tokens: Some(10_000),
            last_cached_tokens: Some(9_500),
            ..CacheUsage::default()
        })));
        assert!(frame.contains("29,450 of 30,342 prompt tokens"), "{frame}");
        assert!(
            frame.contains("9,500 of 10,000 tokens from cache"),
            "{frame}"
        );
        assert!(frame.contains("RAM budget: 70,008 MiB"), "{frame}");
        assert!(frame.contains("disk: on"), "{frame}");
    }

    /// "Nothing measured yet" and "measured, and it was zero" are different
    /// facts; the server keeps them apart, so the frame must too.
    #[test]
    fn cache_section_distinguishes_no_activity_from_a_measured_zero() {
        // Scope to the proxied "Prompt cache" section: the agent section shares
        // the same placeholder text and would otherwise mask the distinction.
        let proxied = |frame: &str| frame.split("Agent cache").next().unwrap().to_string();

        let idle = proxied(&frame_with_cache(Some(cache_status(CacheUsage::default()))));
        assert!(idle.contains("(no cache activity recorded yet)"), "{idle}");

        let measured_zero = proxied(&frame_with_cache(Some(cache_status(CacheUsage {
            reporting_requests: 1,
            prompt_tokens: 5_000,
            cached_tokens: 0,
            last_prompt_tokens: Some(5_000),
            last_cached_tokens: Some(0),
            ..CacheUsage::default()
        }))));
        assert!(
            !measured_zero.contains("no cache activity"),
            "{measured_zero}"
        );
        assert!(
            measured_zero.contains("0 of 5,000 prompt tokens"),
            "{measured_zero}"
        );
    }

    #[test]
    fn cache_section_renders_server_warnings() {
        let mut cache = cache_status(CacheUsage::default());
        cache.warnings = vec!["Low memory available for prompt caching.".to_string()];
        let frame = frame_with_cache(Some(cache));
        assert!(frame.contains("! Low memory available"), "{frame}");
    }

    /// Warnings are server-phrased and can be long; they must not wrap the
    /// frame onto extra physical rows, which would corrupt the redraw's
    /// line-count arithmetic.
    #[test]
    fn cache_section_truncates_a_long_warning_to_one_row() {
        let mut cache = cache_status(CacheUsage::default());
        cache.warnings = vec!["w".repeat(500)];
        let frame = frame_with_cache(Some(cache));
        let longest = frame.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        assert!(
            longest <= usize::from(DEFAULT_TERM_WIDTH),
            "longest line was {longest} columns"
        );
    }

    #[test]
    fn cache_section_names_a_model_suppressed_disk_layer() {
        let mut cache = cache_status(CacheUsage::default());
        cache.disk_suppressed_for_model = true;
        let frame = frame_with_cache(Some(cache));
        assert!(frame.contains("disk: off for this model"), "{frame}");
    }

    #[test]
    fn cache_section_omits_the_budget_when_llama_default_applies() {
        let mut cache = cache_status(CacheUsage::default());
        cache.ram_state = "llama_default".to_string();
        cache.ram_budget_mb = None;
        let frame = frame_with_cache(Some(cache));
        assert!(!frame.contains("RAM budget"), "{frame}");
        assert!(frame.contains("disk: on"), "{frame}");
    }

    #[test]
    fn cache_section_explains_a_budget_the_machine_cannot_afford() {
        let mut cache = cache_status(CacheUsage::default());
        cache.ram_state = "disabled_insufficient_ram".to_string();
        cache.ram_budget_mb = Some(0);
        let frame = frame_with_cache(Some(cache));
        assert!(frame.contains("not enough memory"), "{frame}");
    }

    /// A permanent "0" row would be noise on any current llama.cpp.
    #[test]
    fn cache_section_hides_the_no_data_row_unless_it_is_non_zero() {
        let none_missing = frame_with_cache(Some(cache_status(CacheUsage {
            reporting_requests: 1,
            ..CacheUsage::default()
        })));
        assert!(!none_missing.contains("No cache data"), "{none_missing}");

        let some_missing = frame_with_cache(Some(cache_status(CacheUsage {
            reporting_requests: 1,
            unreported_requests: 2,
            ..CacheUsage::default()
        })));
        assert!(some_missing.contains("No cache data"), "{some_missing}");
    }

    #[test]
    fn thousands_inserts_separators_at_the_right_boundaries() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(70_008), "70,008");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(u64::MAX), "18,446,744,073,709,551,615");
    }

    /// A field the server may add later must not break deserialization —
    /// the mirror deliberately has no `deny_unknown_fields`.
    #[test]
    fn cache_status_tolerates_unknown_and_missing_fields() {
        let json = serde_json::json!({
            "disk_enabled": true,
            "ram_state": "healthy",
            "some_future_field": 42
        })
        .to_string();
        let got: CacheStatus = serde_json::from_str(&json).expect("should deserialize");
        assert!(got.disk_enabled);
        assert_eq!(got.usage.reporting_requests, 0);
        assert_eq!(got.ram_budget_mb, None);
    }
}
