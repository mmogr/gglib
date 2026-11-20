//! CLI entry point.
//!
//! Parses CLI arguments, runs a command that must leave nothing behind
//! before anything else is set up, then initialises the tracing subscriber,
//! runs the commands that read nothing from the library before it is
//! opened, then bootstraps the composition root and delegates to the
//! command dispatcher. A library a newer gglib upgraded is not opened: the
//! command is handed to a gglib on `PATH` that opens it, or refused
//! ([`gglib_cli::refused`]).
//!
//! A command that fails exits 1 with its error printed after `Error:`,
//! unless it is [`AlreadySaid`]: that command has printed why itself.
//!
//! See [`gglib_cli::leaves_nothing_behind`],
//! [`gglib_cli::dispatch_without_library`] and [`gglib_cli::dispatch`] for
//! command routing and [`gglib_cli::bootstrap`] for dependency wiring.

use std::process::ExitCode;

use gglib_cli::{
    AlreadySaid, bootstrap, dispatch, dispatch_without_library, leaves_nothing_behind, parse,
    refused,
};

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    match run().await {
        Err(e) if e.is::<AlreadySaid>() => Ok(ExitCode::FAILURE),
        ended => ended.map(|()| ExitCode::SUCCESS),
    }
}

/// The command line, parsed and run.
async fn run() -> anyhow::Result<()> {
    #[expect(
        clippy::disallowed_methods,
        reason = "this is the gglib program's `main`"
    )]
    gglib_core::paths::this_is_a_gglib_program();
    gglib_core::utils::process::host::prepare();
    let (cli, remote) = parse();
    if leaves_nothing_behind(cli.command.as_ref()) {
        return dispatch_without_library(cli.command).await.map(drop);
    }
    gglib_core::telemetry::init_tracing(cli.verbose)?;
    let Some(command) = dispatch_without_library(cli.command).await? else {
        return Ok(());
    };
    let ctx = match bootstrap().await {
        Ok(ctx) => ctx,
        Err(e) => return Err(refused(e, &command)),
    };
    dispatch(&ctx, command, cli.verbose, remote).await
}
