//! The `--profile` flag, shared by every command that names one model.
//!
//! Flattened into `chat`, `q` and `serve` so the three parse identically —
//! the same reasoning as [`CacheArgs`](crate::shared_args::CacheArgs) and
//! [`AccessArgs`](crate::shared_args::AccessArgs), where a flag present on one
//! command and absent from its twin was a hole rather than a decision.
//!
//! Deliberately **not** on `gglib proxy`. An unpinned proxy has no model in
//! scope for a default to attach to, and its clients already select per
//! request by asking for `{model}:{profile}`.

use clap::Args;

/// Selection of a named sampling profile.
#[derive(Args, Debug, Clone, Default)]
pub struct ProfileArgs {
    /// Apply a named inference profile (see `gglib config profile list`).
    ///
    /// Equivalent to suffixing the model — `gglib chat qwen:coding` — which
    /// is the form clients use over HTTP. Passing both is an error rather
    /// than a precedence rule.
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,
}
