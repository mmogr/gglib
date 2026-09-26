#![doc = include_str!("README.md")]
#[allow(
    clippy::manual_let_else,
    clippy::or_fun_call,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod handlers;
pub(crate) mod meta_tools;
pub(crate) mod session;
pub(crate) mod types;
