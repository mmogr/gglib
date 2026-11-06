//! `gglib proxy test`: one real request through the running endpoint, then
//! how a client connects to it.
//!
//! Both are the daemon's (`POST /api/proxy/prove`, `GET /api/proxy/clients`),
//! the same two `gglib up` ends with; this names the model, waits, and
//! prints. With no model named it uses the pinned model, else the one
//! loaded model, else refuses: a proof needs a model to ask for, and
//! guessing among several would prove the wrong one.

use anyhow::{Result, anyhow};

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle};
use crate::handlers::model::resolver;
use crate::handlers::up::client_block::{render_client_config, render_proof};
use crate::handlers::up::use_color;
use crate::presentation::style::make_spinner;

/// Execute `gglib proxy test [model]`.
pub(crate) async fn execute(ctx: &CliContext, model: Option<&str>) -> Result<()> {
    let handle = daemon_client::running(ctx).await.map_err(|_| {
        anyhow!("gglib isn't running, so nothing can be served: `gglib proxy` starts it")
    })?;
    let model_id = match model {
        Some(identifier) => {
            resolver::resolve_model_identifier(ctx, identifier)
                .await?
                .id
        }
        None => implied_model(ctx, &handle).await?,
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
    Ok(())
}

/// The model to prove with when none is named: the pin, else the one model
/// loaded. Several loaded, or none, is a refusal that says what to name.
async fn implied_model(ctx: &CliContext, handle: &DaemonHandle) -> Result<i64> {
    let status = handle.proxy_status().await?;
    if let Some(pinned) = status.pinned_model {
        return Ok(resolver::resolve_model_identifier(ctx, &pinned).await?.id);
    }
    let servers = handle.list_servers().await?;
    match servers.as_slice() {
        [only] => Ok(only.model_id),
        [] => Err(anyhow!(
            "nothing is loaded: name the model to prove the endpoint with (`gglib proxy test <model>`)"
        )),
        several => Err(anyhow!(
            "{} models are loaded: name the one to prove the endpoint with (`gglib proxy test <model>`)",
            several.len()
        )),
    }
}
