//! `gglib serve --remote`: have the paired machine load the model now.
//!
//! A `#[path]` child of `serve.rs`, which the two arms together took past
//! the size budget. On the paired machine "serve" is what the word can mean
//! there: the model resident before a turn needs it, so the first turn does
//! not wait. Only the name and a numeric `--ctx-size` travel; every other
//! flag configures a proxy on *this* machine.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::target::Target;

/// What the far proxy answers once the model is resident.
#[derive(Debug, serde::Deserialize)]
struct Loaded {
    model: String,
    started: bool,
    context: u64,
}

/// Have the paired machine load the model now, so the first turn does not
/// wait. Only the name and a numeric `--ctx-size` travel.
pub(super) async fn serve_far(
    ctx: &CliContext,
    target: Target,
    identifier: &str,
    ctx_size: Option<&str>,
) -> Result<()> {
    let num_ctx = match ctx_size {
        None => None,
        Some(size) => Some(size.parse::<u64>().map_err(|_| {
            anyhow::anyhow!(
                "on the paired machine --ctx-size is a number of tokens; `max` is decided by \
                 the machine that has the model"
            )
        })?),
    };
    let far = target.far(ctx).await?;
    eprintln!(
        "  Asking {} to load '{identifier}'\u{2026}",
        far.fingerprint
    );
    let loaded: Loaded = far
        .post_json(
            &format!("/models/{identifier}/load"),
            &serde_json::json!({ "num_ctx": num_ctx }),
        )
        .await?;
    eprintln!(
        "  \u{2705} {} is {} on {} (context {})",
        loaded.model,
        if loaded.started {
            "loaded"
        } else {
            "already running"
        },
        far.fingerprint,
        loaded.context
    );
    eprintln!("  Try:  gglib chat --remote {}", loaded.model);
    Ok(())
}
