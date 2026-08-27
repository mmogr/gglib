//! Turning a snapshot into the text of one frame.
//!
//! Pure: every function here takes data and returns a `String`, so the whole
//! dashboard's layout is unit-testable without a terminal, a proxy, or a
//! clock. The IO — cursor movement, the SSE read loop — lives in the parent
//! module and never formats anything itself.
//!
//! Width is passed in rather than queried. In cooked mode a line longer than
//! the terminal wraps onto another physical row, which is what
//! [`visual_row_count`] has to account for when the next frame decides how far
//! to move the cursor up; a renderer that asked the terminal directly would be
//! measuring a different width than the one it drew for.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::BAR_WIDTH;
use super::render_reasoning::{render_client_fields_section, render_reasoning_section};
use super::wire::{
    AdmissionSnapshot, CacheStatus, CacheUsage, DashboardSnapshot, ModelDefectCounts,
};

/// Render a `[███░░░] NN%` bar. `total == 0` renders an empty bar at 0%
/// rather than dividing by zero — used for every gauge in this dashboard so
/// the bar-drawing logic exists in exactly one place.
pub(super) fn progress_bar(filled: u64, total: u64, width: usize) -> String {
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
pub(super) fn format_duration_secs(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

/// Seconds elapsed since a Unix timestamp, formatted as `Ns` (or `Nm Ss` past
/// one minute). Never panics: a clock skew that makes `started_at_secs` look
/// like it's in the future just renders as `0s`.
pub(super) fn format_elapsed_secs(started_at_secs: u64) -> String {
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
pub(super) fn render_frame(url: &str, snapshot: &DashboardSnapshot, term_width: u16) -> String {
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
    out.push_str(&render_defects_section(&snapshot.per_model_defects));

    let audit = snapshot.sampling_audit.as_ref();
    out.push('\n');
    out.push_str(&render_reasoning_section(audit, term_width));
    out.push('\n');
    out.push_str(&render_client_fields_section(audit));

    out.push('\n');
    out.push_str(&format!(
        "Total requests served: {}\n",
        snapshot.total_requests
    ));
    out
}

/// Render the per-model signals section — what failed, what merely went in
/// circles, and for which model.
///
/// These are the diagnostic counters ADR 0006 kept when the tuning scheduler
/// went. Nothing acts on them automatically, which is exactly why they need
/// somewhere to be read: until now they were on `/v1/proxy/status` and in no
/// human-facing surface at all, so the only way to see them was `curl | jq`.
///
/// Only models with something to report get a line, and listing every clean
/// model would bury the one that is not.
///
/// Three counters here are not failures. `identical_result_repeats` describes a
/// conversation that went in a circle, and `repeats_not_evaluated` says how
/// often that question could not be answered — facts about the client's
/// history rather than faults in the model. `repeats_rescued` is a fact about
/// gglib instead: how often the loop guard declined to act because the answer
/// had moved. They print below the defects under an `observed` heading of their
/// own, and a model whose only signal is one of them still earns a line. The
/// section is named for signals rather than defects because of them.
pub(super) fn render_defects_section(per_model: &BTreeMap<String, ModelDefectCounts>) -> String {
    let mut out = String::from("Per-model signals (this proxy run)\n");

    let faulty: Vec<_> = per_model
        .iter()
        .filter(|(_, counts)| !counts.is_clean())
        .collect();

    if per_model.is_empty() {
        out.push_str("  (nothing recorded yet)\n");
        return out;
    }
    if faulty.is_empty() {
        let served: u64 = per_model.values().map(|c| c.requests).sum();
        out.push_str(&format!(
            "  none across {} request(s), {} model(s)\n",
            thousands(served),
            per_model.len()
        ));
        return out;
    }

    for (model, counts) in faulty {
        out.push_str(&format!(
            "  {:<28} {} request(s)\n",
            truncate(model, 28),
            thousands(counts.requests)
        ));

        // Repairs read as a ratio: the attempt rate says how often this model
        // packages a tool call badly, and the success rate says whether the
        // one lever gglib pulls is working on it.
        if counts.repairs_attempted > 0 {
            out.push_str(&format!(
                "    {:<24} {} of {} succeeded\n",
                "tool-call repairs",
                thousands(counts.repairs_succeeded),
                thousands(counts.repairs_attempted)
            ));
        }

        for (label, value) in [
            ("loop-guard trips", counts.loop_guard_trips),
            ("stream errors", counts.stream_errors),
            ("truncated at ceiling", counts.truncated_generations),
            ("dialect residue", counts.dialect_residue),
            ("unvalidatable schemas", counts.unvalidatable_schemas),
            ("normalization errors", counts.normalization_errors),
        ] {
            if value > 0 {
                out.push_str(&format!("    {label:<24} {}\n", thousands(value)));
            }
        }

        // reasoning_only is counted *within* empty_responses, so it is shown
        // as a share of them rather than beside them — printing both as peers
        // reads as more empty turns than actually happened.
        if counts.empty_responses > 0 {
            if counts.reasoning_only > 0 {
                out.push_str(&format!(
                    "    {:<24} {} ({} reasoning-only)\n",
                    "empty responses",
                    thousands(counts.empty_responses),
                    thousands(counts.reasoning_only)
                ));
            } else {
                out.push_str(&format!(
                    "    {:<24} {}\n",
                    "empty responses",
                    thousands(counts.empty_responses)
                ));
            }
        }

        // Below the defects, under a heading of their own, because neither is
        // one: the model asked for the same thing twice and the environment
        // gave the same answer twice. Nothing acts on them — they are the
        // evidence for whether a corrective arm on the input plane would ever
        // fire, and the second says how often the question could be asked at
        // all. Own indent level so the label column is not shared with the
        // defect rows above.
        if counts.identical_result_repeats > 0
            || counts.repeats_not_evaluated > 0
            || counts.repeats_rescued > 0
        {
            out.push_str("    observed\n");
            for (label, value) in [
                ("repeated, same result", counts.identical_result_repeats),
                ("repeated, not comparable", counts.repeats_not_evaluated),
                ("repeated, new result", counts.repeats_rescued),
            ] {
                if value > 0 {
                    out.push_str(&format!("      {label:<24} {}\n", thousands(value)));
                }
            }
        }
    }

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
pub(super) fn render_admission_section(admission: &AdmissionSnapshot, term_width: u16) -> String {
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
pub(super) fn visual_row_count(frame: &str, term_width: u16) -> u16 {
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
pub(super) fn render_usage_rows(usage: &CacheUsage) -> String {
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
pub(super) fn render_cache_section(cache: &CacheStatus, term_width: u16) -> String {
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
pub(super) fn ram_budget_label(cache: &CacheStatus) -> Option<String> {
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
pub(super) fn thousands(value: u64) -> String {
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
pub(super) fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        truncated.push('\u{2026}');
        truncated
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
