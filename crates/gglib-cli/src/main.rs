//! CLI entry point.
//!
//! Parses CLI arguments and initialises the tracing subscriber, runs the
//! commands that read nothing from the library before it is opened, then
//! bootstraps the composition root and delegates to the command dispatcher.
//!
//! See [`gglib_cli::dispatch_without_library`] and [`gglib_cli::dispatch`]
//! for command routing and [`gglib_cli::bootstrap`] for dependency wiring.

use gglib_cli::{bootstrap, dispatch, dispatch_without_library, parse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (cli, remote) = parse();
    gglib_core::telemetry::init_tracing(cli.verbose)?;
    let Some(command) = dispatch_without_library(cli.command)? else {
        return Ok(());
    };
    let ctx = bootstrap().await?;
    dispatch(&ctx, command, cli.verbose, remote).await
}
