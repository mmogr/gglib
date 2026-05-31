//! HITL approval prompt with optional timeout and plan-edit flow.
//!
//! Separated from `render.rs` so the approval I/O surface has its own
//! responsibility boundary and can evolve independently of the event
//! rendering code.
//!
//! # Supported decisions
//!
//! | Input | Action |
//! |-------|--------|
//! | `y` / Enter | [`ApprovalDecision::Approve`] |
//! | `n` | [`ApprovalDecision::Reject`] (prompts for optional reason) |
//! | `e` | [`ApprovalDecision::ApproveWithEdits`] — opens `$EDITOR` for Plan gates |
//! | timeout | auto-resolves with [`ApproveOpts::timeout_action`] |

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use tokio::sync::mpsc;

use gglib_app_services::CouncilApprovalRegistry;
use gglib_core::domain::council::events::ApprovalKind;
use gglib_core::domain::council::task_graph::TaskGraph;
use gglib_core::ports::{ApprovalDecision, CouncilApprovalRegistryPort as _};

use crate::presentation::style;

// =============================================================================
// ApproveOpts
// =============================================================================

/// Runtime options governing HITL approval prompts.
#[derive(Debug, Clone)]
pub(crate) struct ApproveOpts {
    /// How long to wait for user input before auto-resolving.
    ///
    /// `None` = wait indefinitely (default).
    pub timeout_secs: Option<u64>,
    /// What to do when the prompt times out.
    pub timeout_action: TimeoutAction,
}

impl Default for ApproveOpts {
    fn default() -> Self {
        Self {
            timeout_secs: None,
            timeout_action: TimeoutAction::Reject,
        }
    }
}

/// Action to take when an approval prompt times out.
#[derive(Debug, Clone, Copy)]
pub(crate) enum TimeoutAction {
    /// Auto-reject the gate and abort the run.
    Reject,
    /// Auto-approve the gate and continue.
    Approve,
}

/// Parse an `--approval-timeout-action` string.
pub(crate) fn parse_timeout_action(s: &str) -> Result<TimeoutAction> {
    match s {
        "reject" => Ok(TimeoutAction::Reject),
        "approve" => Ok(TimeoutAction::Approve),
        other => Err(anyhow!(
            "unknown timeout action: '{other}'. Valid values: reject, approve"
        )),
    }
}

impl ApproveOpts {
    fn timeout_label(&self) -> &'static str {
        match self.timeout_action {
            TimeoutAction::Reject => "reject",
            TimeoutAction::Approve => "approve",
        }
    }

    fn timed_out_decision(&self) -> ApprovalDecision {
        match self.timeout_action {
            TimeoutAction::Reject => ApprovalDecision::Reject("approval timed out".to_owned()),
            TimeoutAction::Approve => ApprovalDecision::Approve,
        }
    }
}

// =============================================================================
// prompt_and_resolve
// =============================================================================

/// Prompt the user for an approval decision and resolve it in the registry.
///
/// When `opts.timeout_secs` is set this function uses
/// `tokio::time::timeout` around a `tokio::io::AsyncBufReadExt::read_line`
/// future.  Wrapping `std::io::stdin().read_line()` in a timeout is
/// **incorrect** on tokio — the blocking call would hold a thread and the
/// timeout would fire but the read would never actually be cancelled.
pub(crate) async fn prompt_and_resolve(
    approval_id: &str,
    kind: &ApprovalKind,
    registry: &Arc<CouncilApprovalRegistry>,
    last_graph: Option<&TaskGraph>,
    opts: &ApproveOpts,
    input_rx: &mut mpsc::UnboundedReceiver<String>,
) {
    let description = match kind {
        ApprovalKind::Plan => "the proposed plan".to_owned(),
        ApprovalKind::Node { node_id } => format!("node '{node_id}'"),
        ApprovalKind::Tool { node_id, tool_name } => {
            format!("tool call '{tool_name}' in node '{node_id}'")
        }
        ApprovalKind::SpawnSubteam { node_id, .. } => {
            format!("spawn subteam requested by node '{node_id}'")
        }
    };

    // [e]dit is only available for Plan gates when we have the graph in hand.
    let can_edit = matches!(kind, ApprovalKind::Plan) && last_graph.is_some();

    eprintln!(
        "\n{}  ⏸  Awaiting approval for {description}{}",
        style::WARNING,
        style::RESET
    );

    let timeout_hint = opts
        .timeout_secs
        .map(|s| format!("  [auto-{} in {s}s]", opts.timeout_label()))
        .unwrap_or_default();

    if can_edit {
        eprintln!(
            "  [y] approve  [n] reject  [e] edit plan  (Enter = approve){}",
            timeout_hint
        );
    } else {
        eprintln!("  [y] approve  [n] reject  (Enter = approve){}", timeout_hint);
    }
    eprint!("  Decision: ");

    let input = read_line_with_timeout(opts.timeout_secs, input_rx).await;

    let decision = match input.as_deref().map(str::trim) {
        // Timed out
        None => {
            eprintln!(
                "\n  {}  timed out — auto-{}{}",
                style::DIM,
                opts.timeout_label(),
                style::RESET
            );
            opts.timed_out_decision()
        }

        // Reject
        Some("n" | "no" | "reject") => {
            eprint!("  Rejection reason (optional, Enter to skip): ");
            let reason = read_line_async(input_rx).await;
            let reason = reason.trim().to_owned();
            let msg = if reason.is_empty() {
                "rejected by user".to_owned()
            } else {
                reason
            };
            ApprovalDecision::Reject(msg)
        }

        // Edit — Plan gates only, graph must be available
        Some("e" | "edit") if can_edit => {
            // Safety: can_edit guarantees last_graph.is_some()
            let graph = last_graph.expect("can_edit guarantees last_graph is Some");
            match open_editor_for_graph(graph).await {
                Ok(edited) => {
                    eprintln!("  {}✓ edited plan accepted{}", style::SUCCESS, style::RESET);
                    ApprovalDecision::ApproveWithEdits(Box::new(edited))
                }
                Err(e) => {
                    eprintln!(
                        "  {}edit failed: {e} — falling back to approve{}",
                        style::WARNING,
                        style::RESET
                    );
                    ApprovalDecision::Approve
                }
            }
        }

        // y / yes / Enter / anything else → approve
        _ => ApprovalDecision::Approve,
    };

    registry.resolve(approval_id, decision);
}

// =============================================================================
// Async stdin helpers
// =============================================================================

/// Read one line from the shared input receiver, optionally timing out.
///
/// Returns `Some(line)` when a line arrives, `None` on timeout or channel
/// close.  The line was already read from stdin by the background input
/// router task in [`crate::presentation::input`], so this function never
/// touches stdin directly — there is no executor-blocking risk.
async fn read_line_with_timeout(
    timeout_secs: Option<u64>,
    input_rx: &mut mpsc::UnboundedReceiver<String>,
) -> Option<String> {
    let recv_fut = input_rx.recv();
    match timeout_secs {
        None => recv_fut.await,
        Some(secs) => tokio::time::timeout(Duration::from_secs(secs), recv_fut)
            .await
            .ok()
            .flatten(),
    }
}

/// Read one line from the shared input receiver without a timeout.
///
/// Used for follow-up prompts (e.g. rejection reason) where the user has
/// already committed to an action and no timeout is appropriate.
async fn read_line_async(input_rx: &mut mpsc::UnboundedReceiver<String>) -> String {
    input_rx.recv().await.unwrap_or_default()
}

// =============================================================================
// Editor flow
// =============================================================================

/// Serialise `graph` to a temp file, open `$EDITOR`, then parse the result.
async fn open_editor_for_graph(graph: &TaskGraph) -> Result<TaskGraph> {
    let tmp_path =
        std::env::temp_dir().join(format!("gglib-plan-{}.json", std::process::id()));

    let json = serde_json::to_string_pretty(graph).context("serialising graph to JSON")?;
    tokio::fs::write(&tmp_path, &json)
        .await
        .context("writing temporary plan file")?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| fallback_editor());

    eprintln!(
        "  {}  Opening {editor} → {}{}",
        style::DIM,
        tmp_path.display(),
        style::RESET
    );
    eprintln!(
        "  {}  Save and quit the editor to apply your edits.{}",
        style::DIM,
        style::RESET
    );

    let path_for_spawn = tmp_path.clone();
    let exit_status = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&editor)
            .arg(&path_for_spawn)
            .status()
    })
    .await
    .context("editor task panicked")??;

    if !exit_status.success() {
        anyhow::bail!("editor exited with non-zero status ({exit_status})");
    }

    let contents = tokio::fs::read_to_string(&tmp_path)
        .await
        .context("reading edited plan file")?;

    // Best-effort cleanup; don't fail the run if removal fails.
    let _ = tokio::fs::remove_file(&tmp_path).await;

    serde_json::from_str::<TaskGraph>(&contents).context("parsing edited plan JSON")
}

/// Locate the best available editor when `$EDITOR` is unset.
fn fallback_editor() -> String {
    for candidate in ["nano", "vim", "vi"] {
        if std::path::Path::new(&format!("/usr/bin/{candidate}")).exists() {
            return candidate.to_owned();
        }
    }
    "nano".to_owned()
}
