//! CLI entry point.
//!
//! Initialises the tracing subscriber and environment, parses CLI arguments,
//! bootstraps the composition root, and delegates to the command dispatcher.
//!
//! See [`gglib_cli::dispatch`] for command routing and
//! [`gglib_cli::bootstrap`] for dependency wiring.

use clap::Parser;

use gglib_cli::{Cli, CliConfig, bootstrap, dispatch};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    let config = CliConfig::with_defaults()?;
    let ctx = bootstrap(config).await?;

    let Some(command) = cli.command else {
        use clap::CommandFactory;
        gglib_cli::Cli::command().print_help()?;
        return Ok(());
    };

    dispatch(&ctx, command, cli.verbose).await
}
