//! `gglib run start`.

use anyhow::Result;
use gglib_core::domain::runs::new_run_id;
use serde_json::json;

use super::read;
use crate::daemon_client::DaemonHandle;

/// Start a run asking `model` one question. Prints the id on stdout alone,
/// so `id=$(gglib run start …)` works; with `follow` the id goes to stderr
/// and the reply to stdout.
pub(super) async fn start(
    daemon: &DaemonHandle,
    model: &str,
    prompt: &str,
    follow: bool,
) -> Result<()> {
    let id = new_run_id();
    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
    });
    let info = daemon.run_start(&id, &body).await?;
    if !follow {
        println!("{}", info.id);
        eprintln!("  Started. Read it with `gglib run show {}`.", info.id);
        return Ok(());
    }
    eprintln!("  run {}", info.id);
    read::print_reply(daemon, &info.id, true, 0).await
}
