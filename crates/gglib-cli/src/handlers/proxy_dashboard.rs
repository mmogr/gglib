//! `gglib proxy dashboard` — live terminal view of an already-running proxy.
//!
//! Connects to `GET /v1/proxy/status/stream` on a running `gglib proxy` (or
//! `gglib web`) instance and redraws a compact text dashboard in place: active
//! `/v1/chat/completions` connections, per-slot context-window usage from
//! llama.cpp's `/slots` endpoint, and a running request count.
//!
//! ## Decoupled JSON contract, not a shared Rust type
//!
//! This module does **not** depend on `gglib-proxy` (that would pull an
//! Infrastructure-layer, axum-based crate into `gglib-cli`, which
//! `scripts/check_boundaries.sh` treats as a web/gui dependency this crate
//! must not have). Instead, [`DashboardSnapshot`] and friends are a local,
//! `Deserialize`-only mirror of the JSON shape produced by
//! `gglib_proxy::dashboard::DashboardSnapshot` — exactly the same relationship
//! the TypeScript frontend has to that same endpoint. Unknown fields are
//! ignored by default (no `deny_unknown_fields`), so this client tolerates
//! additive changes to the server-side contract.
//!
//! ## Redraw strategy: cursor movement, not raw mode
//!
//! Earlier CLI work in this crate (see
//! [`crate::handlers::model::download::interactive`]) already established
//! that `crossterm::terminal::enable_raw_mode()` breaks `println!`-based
//! redraws (it disables `OPOST`, so `\n` stops returning the cursor to column
//! 0). This module never touches raw mode. Instead, each frame after the
//! first moves the cursor up by the previous frame's line count and clears
//! everything below before printing the next frame — plain
//! `crossterm::cursor`/`terminal` commands in normal (cooked) mode, which
//! compose fine with ordinary `print!`/`println!`. When stdout is not a TTY
//! (piped output, CI), frames are printed sequentially instead, since there is
//! no cursor to move.
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
    /// Agent-path prompt-cache reuse (council + GUI chat) — a separate
    /// population from [`CacheStatus::usage`]. Top-level and always present,
    /// since it does not depend on a resolved model; `default` on a proxy older
    /// than this field.
    #[serde(default)]
    agent_usage: CacheUsage,
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

/// Mirrors `gglib_proxy::slots::SlotSnapshot`'s wire shape, including its
/// private-but-serialized legacy fields — see that type's doc comment for
/// why `/slots`' schema needs this priority-fallback handling.
#[derive(Debug, Deserialize)]
struct SlotSnapshot {
    id: i64,
    #[serde(default)]
    n_ctx: Option<u64>,
    #[serde(default)]
    n_past: Option<u64>,
    #[serde(default)]
    cache_tokens: Option<u64>,
    #[serde(default)]
    n_prompt_tokens: Option<u64>,
    #[serde(default)]
    n_prompt_tokens_processed: Option<u64>,
    #[serde(default)]
    n_prompt_tokens_cache: Option<u64>,
    #[serde(default)]
    next_token: Option<NextTokenField>,
}

#[derive(Debug, Deserialize)]
struct NextTokenInfo {
    #[serde(default)]
    n_decoded: Option<u64>,
}

/// Mirrors `gglib_proxy::slots::NextTokenField` — `next_token` is a single
/// object on regular llama-server builds, but an array of objects on builds
/// with Multi-Token Prediction ("draft-mtp") enabled.
///
/// `Many` must come before `Single` — see the server-side type's doc
/// comment for why (a single-element array can otherwise falsely match
/// `Single` via serde's struct-from-seq deserialization).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NextTokenField {
    Many(Vec<NextTokenInfo>),
    Single(NextTokenInfo),
}

impl NextTokenField {
    /// See `gglib_proxy::slots::NextTokenField::primary` — element 0 is the
    /// accepted/main decode stream on MTP builds.
    fn primary(&self) -> Option<&NextTokenInfo> {
        match self {
            Self::Single(info) => Some(info),
            Self::Many(items) => items.first(),
        }
    }
}

impl SlotSnapshot {
    /// Same additive logic as the server's `SlotSnapshot::tokens_in_use`:
    /// when `n_prompt_tokens_processed` is present, combine it with
    /// `n_prompt_tokens_cache` (tokens reused from KV cache this round) plus
    /// `next_token.n_decoded`. The grand-total `n_prompt_tokens` fallback
    /// (used only when `_processed` is absent) already includes any cached
    /// prefix, so cache is not added on top of it. Only falls back to the
    /// legacy `n_past`/`cache_tokens` chain (no addition) when neither
    /// prompt-side field is present.
    fn tokens_in_use(&self) -> Option<u64> {
        let n_decoded = self
            .next_token
            .as_ref()
            .and_then(NextTokenField::primary)
            .and_then(|nt| nt.n_decoded);

        let prompt_component = if let Some(processed) = self.n_prompt_tokens_processed {
            Some(processed + self.n_prompt_tokens_cache.unwrap_or(0))
        } else {
            self.n_prompt_tokens
        };

        if let Some(prompt_tokens) = prompt_component {
            return Some(prompt_tokens + n_decoded.unwrap_or(0));
        }

        self.n_past.or(self.cache_tokens).or(n_decoded)
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

/// Seconds elapsed since a Unix timestamp, formatted as `Ns` (or `Nm Ss` past
/// one minute). Never panics: a clock skew that makes `started_at_secs` look
/// like it's in the future just renders as `0s`.
fn format_elapsed_secs(started_at_secs: u64) -> String {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(started_at_secs);
    let elapsed = now_secs.saturating_sub(started_at_secs);
    if elapsed < 60 {
        format!("{elapsed}s")
    } else {
        format!("{}m {}s", elapsed / 60, elapsed % 60)
    }
}

/// Build the full multi-line dashboard frame for one snapshot. Pure text
/// generation — no IO — so it's testable without a terminal or network.
///
/// `term_width` bounds every rendered line to at most one physical terminal
/// row. Without this, an unbounded string (e.g. the `/slots` unreachable
/// reason, which can easily exceed 100 characters) wraps onto extra
/// *physical* rows that the caller's `frame.lines().count()` bookkeeping
/// never sees, undercounting how far to move the cursor up on the next
/// redraw — this both corrupts the display (stale wrapped remnants left
/// on screen, looking like truncation) and makes the whole frame drift
/// down the terminal on every subsequent tick (visible scrolling).
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
    out.push_str("Prompt cache\n");
    match &snapshot.cache {
        None => out.push_str("  (no model resolved yet)\n"),
        Some(cache) => out.push_str(&render_cache_section(cache, term_width)),
    }

    // A separate population from the proxied figure above: council and GUI-chat
    // runs talk to llama-server directly, so their reuse profile is nothing
    // like a user's conversation and must not be averaged into it.
    out.push('\n');
    out.push_str("Agent cache (council · GUI chat)\n");
    out.push_str(&render_usage_rows(&snapshot.agent_usage));

    out.push('\n');
    out.push_str(&format!(
        "Total requests served: {}\n",
        snapshot.total_requests
    ));
    out
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
pub async fn execute(host: String, port: u16) -> Result<()> {
    let url = format!("http://{host}:{port}/v1/proxy/status/stream");

    let response = reqwest::get(&url)
        .await
        .with_context(|| format!("failed to connect to {url} — is the proxy running?"))?;
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
                        previous_frame_lines = frame.lines().count() as u16;
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
            slots: vec![SlotSnapshot {
                id: 0,
                n_ctx: Some(4096),
                n_past: Some(2048),
                cache_tokens: None,
                n_prompt_tokens: None,
                n_prompt_tokens_processed: None,
                n_prompt_tokens_cache: None,
                next_token: None,
            }],
            slots_status: None,
            total_requests: 3,
            cache: None,
            agent_usage: CacheUsage::default(),
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
    fn slot_snapshot_parses_mtp_array_next_token_shape() {
        // Same wire shape the proxy re-serializes for an MTP ("draft-mtp")
        // llama-server build — `next_token` is an array, not a bare object.
        let json = r#"{
            "id": 3,
            "n_ctx": 131072,
            "next_token": [
                { "n_decoded": 89 }
            ]
        }"#;
        let slot: SlotSnapshot = serde_json::from_str(json).expect("should parse MTP shape");
        assert_eq!(slot.tokens_in_use(), Some(89));
    }

    #[test]
    fn slot_snapshot_tokens_in_use_is_additive_with_prompt_tokens() {
        // Real payload shape: n_prompt_tokens_processed (prompt usage) and
        // next_token.n_decoded (generated tokens) must be summed, not just
        // read as n_decoded alone (which previously showed ~0% used for a
        // 20k+-token prompt).
        let json = r#"{
            "id": 3,
            "n_ctx": 131072,
            "n_prompt_tokens": 20994,
            "n_prompt_tokens_processed": 20906,
            "next_token": [
                { "n_decoded": 89 }
            ]
        }"#;
        let slot: SlotSnapshot = serde_json::from_str(json).expect("should parse");
        assert_eq!(slot.tokens_in_use(), Some(20906 + 89));
    }

    #[test]
    fn slot_snapshot_tokens_in_use_adds_cache_reused_tokens() {
        // KV-cache-reuse scenario: a follow-up prompt where llama-server
        // found a large cached prefix match and only newly processed a
        // small delta. n_prompt_tokens_cache must be added to
        // n_prompt_tokens_processed, or context usage falsely collapses to
        // just the tiny newly-processed delta.
        let json = r#"{
            "id": 0,
            "n_ctx": 131072,
            "n_prompt_tokens": 7981,
            "n_prompt_tokens_processed": 1245,
            "n_prompt_tokens_cache": 6736,
            "next_token": [
                { "n_decoded": 12 }
            ]
        }"#;
        let slot: SlotSnapshot = serde_json::from_str(json).expect("should parse");
        assert_eq!(slot.tokens_in_use(), Some(1245 + 6736 + 12));
    }

    #[test]
    fn render_frame_truncates_long_slots_error_to_fit_terminal_width() {
        // A realistic reqwest connect-error string easily exceeds 100 chars
        // — e.g. "error sending request for url (http://127.0.0.1:5500/slots):
        // error trying to connect: tcp connect error: Connection refused (os
        // error 61)". Without width-aware truncation this would wrap onto
        // multiple physical terminal rows that `frame.lines().count()` can't
        // see, corrupting the redraw (bugs #1 and #4).
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
        };
        render_frame("http://127.0.0.1:8080", &snapshot, DEFAULT_TERM_WIDTH)
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
        assert!(idle.contains("Agent cache (council"), "{idle}");
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
        assert!(active.contains("Agent cache (council"), "{active}");
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
