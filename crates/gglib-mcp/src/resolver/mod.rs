#![doc = include_str!("README.md")]
mod env;
mod fs;
mod resolve;
mod search;
mod types;

// Trimmed to what is actually reached through `resolver::`. The rest of this
// module's types are used inside `resolver/` via their defining modules, so the
// re-exports were carrying names nobody imported by this path — invisible while
// the module was `pub`, an unused-import error once it was not.
pub(crate) use resolve::resolve_executable;
pub(crate) use types::{ResolveError, ResolveResult};
