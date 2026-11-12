//! `gglib proxy test`: one real request through the running endpoint, then
//! how a client connects to it.
//!
//! Both are the daemon's (`POST /api/proxy/prove`, `GET /api/proxy/clients`),
//! the same two `gglib up` ends with; this names the model, waits, and
//! prints. With no model named the daemon proves with its pin; with no pin
//! this uses the one chat model loaded, else refuses: a proof needs a model
//! to ask for, and guessing among several would prove the wrong one.

use anyhow::{Result, anyhow};
use gglib_core::domain::RuntimeKind;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle};
use crate::handlers::model::resolver;
use crate::handlers::up::client_block::{render_client_config, render_proof};
use crate::handlers::up::use_color;
use crate::presentation::style::make_spinner;

/// Execute `gglib proxy test [model]`.
///
/// # Errors
///
/// gglib not running, the endpoint off, no model to prove with, or a proof
/// the endpoint did not answer: the block is printed first when there is
/// one, then the command exits non-zero, so a script can tell.
pub(crate) async fn execute(ctx: &CliContext, model: Option<&str>) -> Result<()> {
    let handle = daemon_client::running().await.map_err(|_| {
        anyhow!("gglib isn't running, so nothing can be served: `gglib proxy` starts it")
    })?;
    let status = handle.proxy_status().await?;
    if !status.running {
        return Err(anyhow!(
            "the endpoint is off: turn it on first (`gglib proxy`, or Turn on)"
        ));
    }
    let model_id = match model {
        Some(identifier) => Some(
            resolver::resolve_model_identifier(ctx, identifier)
                .await?
                .id,
        ),
        // The daemon proves its pin by id when none is named.
        None if status.pinned_model_id.is_some() => None,
        None => Some(the_one_chat_model(&handle).await?),
    };

    let spinner = make_spinner();
    spinner.set_message("Asking the endpoint...".to_owned());
    let proved = handle.proxy_prove(model_id).await;
    spinner.finish_and_clear();
    let lines = match &proved {
        Ok(proof) => render_proof(Ok(proof), use_color()),
        Err(e) => render_proof(Err(&e.to_string()), use_color()),
    };
    for line in lines {
        println!("{line}");
    }

    let recipes = handle.proxy_clients(model_id).await?;
    for line in render_client_config(&recipes, use_color()) {
        println!("{line}");
    }
    proved
        .map(drop)
        .map_err(|_| anyhow!("the endpoint did not answer the proof (see above)"))
}

/// The model to prove with when none is named and nothing is pinned: the
/// one chat model loaded. An image model cannot answer a chat completion,
/// so it is not a candidate. Several, or none, is a refusal that says what
/// to name.
async fn the_one_chat_model(handle: &DaemonHandle) -> Result<i64> {
    let servers = handle.list_servers().await?;
    let chat: Vec<i64> = servers
        .iter()
        .filter(|s| s.runtime == RuntimeKind::Llama)
        .map(|s| s.model_id)
        .collect();
    match chat.as_slice() {
        [only] => Ok(*only),
        [] => Err(anyhow!(
            "no chat model is loaded: name the model to prove the endpoint with (`gglib proxy test <model>`)"
        )),
        several => Err(anyhow!(
            "{} chat models are loaded: name the one to prove the endpoint with (`gglib proxy test <model>`)",
            several.len()
        )),
    }
}
