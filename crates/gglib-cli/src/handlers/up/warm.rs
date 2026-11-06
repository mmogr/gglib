//! Step 5: prove the endpoint works, then say how to connect to it (the
//! client block, `client_block`) and what to run next.
//!
//! Both are the daemon's: it sends one real request through its own endpoint
//! (`POST /api/proxy/prove`), so the router, admission control, model
//! resolution and the forward pipeline are exercised rather than assumed, and
//! it writes how a client connects (`GET /api/proxy/clients`). This prints
//! what it answered; `gglib proxy test` prints the same two things.
//!
//! The proof is also what makes the launch narration part of `gglib up`'s
//! output: an unpinned proxy loads nothing until a request arrives, so without
//! a first request the user's evidence that anything happened is a bound
//! socket.

use super::client_block::{render_client_config, render_proof};
use super::use_color;
use crate::daemon_client::DaemonHandle;
use crate::presentation::style::{BOLD, DIM, RESET, make_spinner};

/// Have the daemon prove the endpoint it just started with `model_id`, then
/// print how a client connects to it.
///
/// Never returns an error: a failed proof leaves a perfectly usable proxy,
/// and tearing that down over a slow first load would be a worse outcome
/// than the cold start it was trying to avoid.
pub(super) async fn run(handle: &DaemonHandle, model_id: i64) {
    let spinner = make_spinner();
    spinner.set_message("Loading the model through the endpoint...".to_owned());
    let proved = handle.proxy_prove(model_id).await;
    spinner.finish_and_clear();
    println!();
    let lines = match &proved {
        Ok(proof) => render_proof(Ok(proof), use_color()),
        Err(e) => render_proof(Err(&e.to_string()), use_color()),
    };
    for line in lines {
        println!("{line}");
    }
    // The block is the daemon's too. A daemon that refuses to say (older
    // than this CLI, or its endpoint gone between the two calls) is printed
    // as it answered.
    match handle.proxy_clients(model_id).await {
        Ok(recipes) => {
            for line in render_client_config(&recipes, use_color()) {
                println!("{line}");
            }
        }
        Err(e) => println!("  The daemon did not say how a client connects: {e}"),
    }
}

/// What to run once `up` is done, as data: the endpoint keeps serving on the
/// daemon, so the list ends with how to watch it and how to stop it. The model
/// is named by its id, which a shell passes as one argument.
pub(super) fn render_next_steps(port: u16, model_id: i64, color: bool) -> Vec<String> {
    let (bold, dim, reset) = if color {
        (BOLD, DIM, RESET)
    } else {
        ("", "", "")
    };
    let watch = format!("gglib proxy dashboard --port {port}");
    vec![
        format!("  {bold}Ready.{reset} The endpoint keeps running in the background."),
        String::new(),
        format!(
            "    {:<36}{dim}chat in this terminal{reset}",
            format!("gglib chat {model_id}")
        ),
        format!(
            "    {:<36}{dim}open the chat page in your browser{reset}",
            "gglib web"
        ),
        format!("    {watch:<36}{dim}watch requests as they arrive{reset}"),
        format!(
            "    {:<36}{dim}stop the endpoint{reset}",
            "gglib proxy stop"
        ),
        String::new(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `up` ends here rather than in the dashboard, so the list has to name
    /// the chat, the web page, and how to watch and stop what is still
    /// running, with this run's model and port filled in.
    #[test]
    fn the_next_steps_name_this_runs_model_and_port() {
        let text = render_next_steps(8123, 7, false).join("\n");
        for command in [
            "gglib chat 7 ",
            "gglib web",
            "gglib proxy dashboard --port 8123",
            "gglib proxy stop",
        ] {
            assert!(text.contains(command), "missing `{command}` in:\n{text}");
        }
    }
}
