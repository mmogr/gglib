//! What the daemon is asked to start for a model: the body of
//! `POST /api/servers/start`, which `gglib model load` sends.

use anyhow::Result;
use gglib_app_services::types::StartServerRequest;
use gglib_core::domain::Model;
use gglib_core::server_config::parse_ctx_size_flag;

use crate::daemon_client::StartServerBody;

/// What the daemon is asked to start for `model`, at the context `--context`
/// names.
///
/// Only that tier is resolved here, which is what makes `--context max` work:
/// it needs the model's own context length. The daemon applies the per-model
/// and global tiers itself, as it does for every other start request.
pub(super) fn start_body(model: &Model, ctx_size: Option<&str>) -> Result<StartServerBody> {
    let ctx_arg = parse_ctx_size_flag(ctx_size)?;
    Ok(StartServerBody {
        id: model.id,
        config: StartServerRequest {
            context_length: ctx_arg.and_then(|arg| arg.resolve(model.context_length)),
            ..Default::default()
        },
    })
}

#[cfg(test)]
#[path = "start_body_tests.rs"]
mod tests;
