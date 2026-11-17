//! The context a launch resolves to.
//!
//! Split from `mod.rs`, which keeps the admission path. The fit it may fall
//! back from is gglib-core's `fit_on_device`.

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
    model_defaults: Option<&gglib_core::domain::ServerConfig>,
) -> (ServerConfigOptions, u64, ContextSizeSource) {
    let mut opts = template.overlay(per_call);
    opts.context_size = num_ctx.or(opts.context_size);
    // The model's own rung, set from this model and never inherited from
    // the template: a context, and since M2 its templates and speculative
    // decoding, each below what this launch chose and above the tags.
    opts.model_server_ctx = model_defaults.and_then(|d| d.context_length);
    opts.model_jinja = model_defaults.and_then(|d| d.jinja);
    opts.model_mtp = model_defaults.and_then(|d| d.mtp);
    opts.fitted_ctx = fitted_ctx;
    // Assigned as given, not `Some(default_ctx)`: a user who set nothing must
    // fall through to the fitted rung rather than be handed the floor as
    // though they had chosen it.
    opts.global_default_ctx = default_ctx;
    let (resolved_ctx, ctx_source) = resolve_context_size_with_source(&opts);
    (opts, resolved_ctx, ctx_source)
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
