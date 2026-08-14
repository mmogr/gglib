#![doc = include_str!("README.md")]
mod env;
mod fs;
mod resolve;
mod search;
mod types;

pub(crate) use resolve::resolve_executable;
pub(crate) use types::{ResolveError, ResolveResult};
