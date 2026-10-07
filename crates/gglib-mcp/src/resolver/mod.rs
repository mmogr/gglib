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
pub(crate) use fs::{FsProvider, SystemFs};
pub(crate) use resolve::resolve_executable;
// Outside the resolver only a child's `PATH` is built from the default
// directories, and only on macOS.
#[cfg(target_os = "macos")]
pub(crate) use search::DEFAULT_DIRS;
pub(crate) use search::PATH_SEPARATOR;
pub(crate) use types::{AttemptOutcome, ResolveError, ResolveResult};
