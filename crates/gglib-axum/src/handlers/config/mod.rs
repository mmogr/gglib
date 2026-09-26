#![doc = include_str!("README.md")]
pub(crate) mod settings;
#[allow(
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::unused_async,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod setup;
