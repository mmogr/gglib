//! `gglib config llama update` — CLI surface adapter.
//!
//! Whether an update may start is [`update_preflight`]'s to say, as it is for
//! the GUI's update, and the work is [`run_llama_update`], shared with it too.
//! What is the command's own is the plan it prints, the question it asks and
//! the progress it draws.

use anyhow::Result;
use tokio::sync::mpsc;

use super::llama_events::{GIT_REPORTS_THE_PULL, render_build_events, updated};
use crate::utils::input;
use gglib_runtime::llama::{BuildEvent, UpdatePlan, run_llama_update, update_preflight};

/// Update llama.cpp to the latest version.
///
/// A refused preflight is the command's error: its message says what is
/// wrong and what to run, and the command exits non-zero.
pub(crate) async fn handle_update() -> Result<()> {
    let plan = update_preflight()?;

    for line in plan_lines(&plan) {
        println!("{line}");
    }
    if !input::prompt_confirmation("Continue?")? {
        println!("Update cancelled.");
        return Ok(());
    }

    println!();
    let (tx, rx) = mpsc::channel::<BuildEvent>(64);
    let update = tokio::spawn(run_llama_update(
        plan.acceleration,
        plan.llama_dir,
        plan.server_path,
        tx,
    ));
    render_build_events(rx, GIT_REPORTS_THE_PULL, updated, &mut std::io::stdout()).await;
    update.await??;

    Ok(())
}

/// What the command says it will do, before it asks whether to.
fn plan_lines(plan: &UpdatePlan) -> Vec<String> {
    let mut lines = vec!["Updating llama.cpp...".to_owned(), String::new()];

    if let Some(recorded) = &plan.recorded {
        lines.push(format!("Current version: {}", recorded.version));
        lines.push(format!("Build config: {}", recorded.acceleration));
    }

    lines.extend([
        String::new(),
        "This will:".to_owned(),
        "  - Pull latest llama.cpp changes".to_owned(),
        format!(
            "  - Rebuild with {} support",
            plan.acceleration.display_name()
        ),
        "  - Replace current binary".to_owned(),
        String::new(),
        "Current models will NOT be affected.".to_owned(),
        String::new(),
    ]);

    if let Some(caution) = &plan.caution {
        lines.push(caution.clone());
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
#[path = "llama_update_tests.rs"]
mod tests;
