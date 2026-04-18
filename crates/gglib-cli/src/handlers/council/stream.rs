//! Live terminal renderer for council SSE events.
//!
//! Consumes a `CouncilEvent` channel and prints ANSI-coloured output
//! to stdout (text) and stderr (chrome: agent headers, tool status, etc.).

use std::io::{self, Write as _};

use tokio::sync::mpsc;

use gglib_agent::council::events::CouncilEvent;

use crate::presentation::style::{BOLD, DIM, RESET};

// ─── Temperature colours (contentiousness → ANSI 256-colour) ────────────────

/// Map contentiousness to an ANSI 256-colour code for terminal display.
/// Mirrors the 5-tier mapping from `prompts::contentiousness_to_instruction`.
pub(crate) fn temperature_fg(c: f32) -> &'static str {
    if c < 0.2 {
        "\x1b[38;5;37m" // teal — collaborative
    } else if c < 0.4 {
        "\x1b[38;5;35m" // emerald — constructive
    } else if c < 0.6 {
        "\x1b[38;5;249m" // neutral grey — balanced
    } else if c < 0.8 {
        "\x1b[38;5;214m" // amber — adversarial
    } else {
        "\x1b[38;5;196m" // red — devil's advocate
    }
}

// ─── Stream renderer ────────────────────────────────────────────────────────

/// Consume council events from the channel and render them to the terminal.
pub async fn render_council_stream(rx: &mut mpsc::Receiver<CouncilEvent>) {
    let mut current_agent_color = "";
    let mut in_synthesis = false;

    while let Some(event) = rx.recv().await {
        match event {
            CouncilEvent::AgentTurnStart {
                agent_name,
                round,
                contentiousness,
                ..
            } => {
                let color = temperature_fg(contentiousness);
                current_agent_color = color;
                in_synthesis = false;
                eprintln!("\n{color}{BOLD}── {agent_name}{RESET}  {DIM}(round {round}){RESET}");
            }

            CouncilEvent::AgentTextDelta { delta, .. } => {
                print!("{delta}");
                let _ = io::stdout().flush();
            }

            CouncilEvent::AgentReasoningDelta { delta, .. } => {
                eprint!("{DIM}{delta}{RESET}");
                let _ = io::stderr().flush();
            }

            CouncilEvent::AgentToolCallStart {
                display_name,
                args_summary,
                ..
            } => match args_summary {
                Some(summary) => {
                    eprintln!(
                        "  {DIM}⚙{RESET}  {BOLD}{display_name}{RESET}  {DIM}{summary}{RESET} …"
                    );
                }
                None => {
                    eprintln!("  {DIM}⚙{RESET}  {BOLD}{display_name}{RESET} …");
                }
            },

            CouncilEvent::AgentToolCallComplete {
                display_name,
                duration_display,
                result,
                ..
            } => {
                let icon = if result.success { "✓" } else { "✗" };
                let icon_color = if result.success {
                    "\x1b[32m"
                } else {
                    "\x1b[31m"
                };
                eprintln!(
                    "  {icon_color}{icon}{RESET}  {BOLD}{display_name}{RESET}  {DIM}{duration_display}{RESET}"
                );
            }

            CouncilEvent::AgentTurnComplete { core_claim, .. } => {
                println!();
                if let Some(claim) = core_claim {
                    eprintln!("  {current_agent_color}{DIM}CORE CLAIM: {claim}{RESET}");
                }
            }

            CouncilEvent::RoundSeparator { round } => {
                eprintln!("\n{DIM}═══════════════════ Round {round} ═══════════════════{RESET}");
            }

            CouncilEvent::JudgeStart { round } => {
                eprintln!(
                    "\n{DIM}── Judge evaluating round {round} ──{RESET}"
                );
            }

            CouncilEvent::JudgeTextDelta { delta } => {
                eprint!("{DIM}{delta}{RESET}");
                let _ = io::stderr().flush();
            }

            CouncilEvent::JudgeSummary {
                consensus_reached,
                summary,
                ..
            } => {
                eprintln!();
                if consensus_reached {
                    eprintln!(
                        "  \x1b[32m✓{RESET}  {BOLD}Consensus reached{RESET}  {DIM}{summary}{RESET}"
                    );
                } else {
                    eprintln!(
                        "  {DIM}○  No consensus — debate continues  {summary}{RESET}"
                    );
                }
            }

            CouncilEvent::RoundCompacted { round, .. } => {
                eprintln!("{DIM}  ↹ Round {round} compacted{RESET}");
            }

            CouncilEvent::SynthesisStart => {
                in_synthesis = true;
                eprintln!("\n\x1b[36m{BOLD}── Council Synthesis ──{RESET}");
            }

            CouncilEvent::SynthesisTextDelta { delta } => {
                print!("{delta}");
                let _ = io::stdout().flush();
            }

            CouncilEvent::SynthesisComplete { .. } => {
                println!();
            }

            CouncilEvent::CouncilError { message } => {
                eprintln!("\n  \x1b[31m❌  {message}{RESET}");
            }

            CouncilEvent::CouncilComplete => {
                if !in_synthesis {
                    println!();
                }
                eprintln!("{DIM}Council complete.{RESET}");
            }
        }
    }
}
