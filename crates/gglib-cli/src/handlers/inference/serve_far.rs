//! `gglib serve --remote`: have the paired machine load the model now.
//!
//! A `#[path]` child of `serve.rs`, which the two arms together took past
//! the size budget. On the paired machine "serve" is what the word can mean
//! there: the model resident before a turn needs it, so the first turn does
//! not wait. Only the identifier and a numeric `--ctx-size` travel; every
//! other flag configures a proxy on *this* machine. The daemon carries the
//! request through the tunnel, the identifier as one encoded path segment,
//! so a name holding `/` reaches that machine whole.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::target::Target;

/// Have the paired machine load the model now, so the first turn does not
/// wait. Only the identifier and a numeric `--ctx-size` travel.
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
    let paired = target.paired(ctx).await?;
    let machine = &paired.connection.ticket_fingerprint;
    eprintln!("  Asking {machine} to load '{identifier}'\u{2026}");
    let loaded = paired.handle.paired_load(identifier, num_ctx).await?;
    eprintln!(
        "  \u{2705} {} is {} on {machine} (context {})",
        loaded.model,
        if loaded.started {
            "loaded"
        } else {
            "already running"
        },
        loaded.context
    );
    eprintln!("  Try:  gglib chat --remote {}", loaded.model);
    Ok(())
}
