//! CLI entry point.
//!
//! Parses CLI arguments, runs a command that must leave nothing behind
//! before anything else is set up, then initialises the tracing subscriber,
//! runs the commands that read nothing from the library before it is
//! opened, then bootstraps the composition root and delegates to the
//! command dispatcher.
//!
//! See [`gglib_cli::leaves_nothing_behind`],
//! [`gglib_cli::dispatch_without_library`] and [`gglib_cli::dispatch`] for
//! command routing and [`gglib_cli::bootstrap`] for dependency wiring.

use gglib_cli::{bootstrap, dispatch, dispatch_without_library, leaves_nothing_behind, parse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gglib_core::utils::process::host::prepare();
    let (cli, remote) = parse();
    if leaves_nothing_behind(cli.command.as_ref()) {
        return dispatch_without_library(cli.command).await.map(drop);
    }
    gglib_core::telemetry::init_tracing(cli.verbose)?;
    let Some(command) = dispatch_without_library(cli.command).await? else {
        return Ok(());
    };
    let ctx = bootstrap().await?;
    dispatch(&ctx, command, cli.verbose, remote).await
}
