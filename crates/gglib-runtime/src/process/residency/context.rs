//! The context a launch resolves to, and the fit it may fall back from.
//!
//! Split from `mod.rs`, so the admission path there stays within its size.

use gglib_core::server_config::{
    ContextSizeSource, ServerConfigOptions, resolve_context_size_with_source,
};

/// Resolve one launch's [`ServerConfigOptions`] and context size together.
///
/// Pulled out of the admission path so the context-resolution logic can be
/// tested without spawning a process. Its result is the one context the rest
/// of a launch reads, so no later step sizes the context its own way ([#685]).
///
/// The context chain is assigned onto the overlaid template rather than
/// overlaid itself: the manager is authoritative for every rung, and a
/// stale `model_server_ctx` inherited from `template` would silently size the
/// launch for a different model than `model_server_ctx` names here.
///
/// [#685]: https://github.com/mmogr/gglib/issues/685
pub(super) fn resolve_launch_opts(
    template: &ServerConfigOptions,
    per_call: &ServerConfigOptions,
    num_ctx: Option<u64>,
    default_ctx: Option<u64>,
    fitted_ctx: Option<u64>,
    model_server_ctx: Option<usize>,
) -> (ServerConfigOptions, u64, ContextSizeSource) {
    let mut opts = template.overlay(per_call);
    opts.context_size = num_ctx.or(opts.context_size);
    opts.model_server_ctx = model_server_ctx;
    opts.fitted_ctx = fitted_ctx;
    // Assigned as given, not `Some(default_ctx)`: a user who set nothing must
    // fall through to the fitted rung rather than be handed the floor as
    // though they had chosen it.
    opts.global_default_ctx = default_ctx;
    let (resolved_ctx, ctx_source) = resolve_context_size_with_source(&opts);
    (opts, resolved_ctx, ctx_source)
}

/// Fit against the reserved budget, falling back to the undivided device.
///
/// A seam, not indirection: the chain is the whole of the co-resident
/// reservation's escape hatch, and inlining it left the behaviour unguarded —
/// deleting the fallback passed every test in the crate.
///
/// The seam guards the logic, not the wiring. Passing the same budget as both
/// arguments still neuters the fallback and no test would notice; catching that
/// needs `admit` exercised end to end, which this module does not do.
pub(super) fn fit_or_undivided<F>(
    fit: F,
    reserved: Option<u64>,
    undivided: Option<u64>,
) -> Option<u64>
where
    F: Fn(Option<u64>) -> Option<u64>,
{
    fit(reserved).or_else(|| fit(undivided))
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
