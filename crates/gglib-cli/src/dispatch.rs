//! Top-level command dispatcher.
//!
//! Routes parsed `Commands` variants to their respective handler modules.
//! Every match arm is a thin delegation — no business logic lives here.
//!
//! ## Coupling discipline
//!
//! `dispatch` receives a shared reference to `CliContext`.  Individual calls
//! forward only the fields each handler actually needs, which keeps the
//! coupling between the dispatch layer and each handler as narrow as possible.

use crate::target::Target;
use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::commands::Commands;
use crate::config_commands::ConfigCommand;
use crate::handlers;

/// Run a command that reads nothing from the library, before the library is
/// opened: help (no command), `completions` and `config paths`. Each other
/// command is handed back for [`dispatch`].
///
/// Opening the library creates it on a fresh data folder, upgrades an older
/// one and refuses a newer one, and none of these three reads it, so they
/// answer with no library and with one this build refuses. `config paths`
/// is what `docs/compatibility.md` tells the owner of such a library to run.
///
/// # Errors
///
/// Whatever the command's handler returns.
pub fn dispatch_without_library(command: Option<Commands>) -> Result<Option<Commands>> {
    match command {
        None => {
            use clap::CommandFactory as _;
            crate::parser::Cli::command().print_help()?;
        }
        Some(Commands::Completions { shell }) => handlers::completions::execute(shell)?,
        Some(Commands::Config {
            command: ConfigCommand::Paths,
        }) => handlers::config::paths::execute()?,
        Some(other) => return Ok(Some(other)),
    }
    Ok(None)
}

/// Route a parsed command to its handler.
///
/// # Arguments
///
/// * `ctx`     — Shared reference to the fully-composed CLI context.
/// * `command` — The command to execute, as parsed by Clap.
/// * `verbose` — Value of the global `--verbose` flag; forwarded only to
///   handlers that expose a verbosity knob.
/// * `remote`  — Whether `--remote` was typed (ADR 0028). Becomes the
///   [`Target`] every command that uses a machine is told. A command that
///   does not declare the flag was refused it by
///   [`parse`](crate::remote_flag::parse), and so was one typed on a
///   parent before its subcommand (`gglib chat --remote history`).
#[expect(
    clippy::too_many_lines,
    reason = "one arm per command, each a call into its handler; the match is the command table"
)]
pub async fn dispatch(
    ctx: &CliContext,
    command: Commands,
    verbose: bool,
    remote: bool,
) -> Result<()> {
    let target = Target::from_flag(remote);
    target.admit(&command)?;
    match command {
        // ── Getting started ─────────────────────────────────────────────────
        Commands::Up { yes, model, port } => {
            handlers::up::execute(ctx, handlers::up::UpArgs { yes, model, port }).await?;
        }

        // ── Grouped: model management ───────────────────────────────────────
        Commands::Model { command } => {
            handlers::model::dispatch(ctx, command, target).await?;
        }

        // ── Grouped: configuration & system ─────────────────────────────────
        Commands::Config { command } => {
            handlers::config::dispatch(ctx, command).await?;
        }

        // ── Inference (top-level for ergonomic access) ──────────────────────
        Commands::Serve {
            identifier,
            context,
            options,
            sampling,
            profile,
            mtp,
            cache,
            access,
            remote: _,
        } => {
            handlers::inference::serve::execute(
                ctx,
                target,
                identifier,
                context,
                options,
                sampling,
                profile.profile,
                mtp,
                cache,
                access,
                verbose,
            )
            .await?;
        }
        Commands::Chat {
            identifier,
            system_prompt,
            sampling,
            profile,
            no_tools,
            remote: _,
            max_iterations,
            tools,
            limits,
            images,
            continue_id,
            thinking,
            command,
        } => {
            // A subcommand (`gglib chat history`) receives none of chat's own
            // flags, and the parse refused any typed before it.
            if let Some(sub) = command {
                match sub {
                    crate::commands::ChatCommand::History { limit } => {
                        handlers::history::execute(ctx, limit).await?;
                    }
                }
            } else {
                let args = handlers::inference::chat::ChatArgs {
                    identifier,
                    system_prompt,
                    sampling,
                    profile: profile.profile,
                    no_tools,
                    target,
                    max_iterations,
                    tools,
                    tool_timeout_ms: limits.tool_timeout_ms,
                    max_parallel: limits.max_parallel,
                    images: images.images,
                    verbose, // global flag forwarded here
                    continue_id,
                    thinking: thinking.map(Into::into),
                };
                handlers::inference::chat::execute(ctx, args).await?;
            }
        }

        Commands::Question {
            question,
            model,
            file,
            show_prompt,
            quiet,
            sampling,
            profile,
            no_tools,
            remote: _,
            max_iterations,
            tools,
            limits,
            images,
        } => {
            let args = handlers::inference::agent_question::QuestionArgs {
                question,
                model_arg: model,
                file,
                target,
                max_iterations,
                tools,
                no_tools,
                tool_timeout_ms: limits.tool_timeout_ms,
                max_parallel: limits.max_parallel,
                images: images.images,
                show_prompt,
                verbose, // global flag forwarded here, as on `chat`
                quiet,
                sampling,
                profile: profile.profile,
            };
            handlers::inference::agent_question::execute(ctx, args).await?;
        }

        // ── GUI / web interfaces ────────────────────────────────────────────
        Commands::Gui => {
            handlers::gui::execute()?;
        }
        Commands::Web { no_open } => {
            handlers::web::execute(no_open).await?;
        }
        Commands::Daemon { command } => match command {
            crate::commands::DaemonCommand::Run { allowed_host } => {
                handlers::daemon::run(allowed_host).await?;
            }
            crate::commands::DaemonCommand::Status => {
                handlers::daemon::status().await?;
            }
            crate::commands::DaemonCommand::Stop { yes, remote: _ } => {
                handlers::daemon::stop(target, yes).await?;
            }
        },
        Commands::Proxy {
            bind,
            sampling,
            cache,
            access,
            command,
        } => {
            // A subcommand (`gglib proxy dashboard`) connects to an
            // already-running proxy rather than starting one. It receives
            // none of the proxy's own flags, and the parse refused any typed
            // before it.
            if let Some(sub) = command {
                match sub {
                    crate::commands::ProxyCommand::Dashboard {
                        host: dash_host,
                        port: dash_port,
                        api_key,
                        remote: _,
                    } => {
                        let (dash_host, dash_port, key) = target
                            .proxy_endpoint(ctx, dash_host, dash_port, api_key)
                            .await?;
                        handlers::proxy_dashboard::execute(dash_host, dash_port, key.as_deref())
                            .await?;
                    }
                    crate::commands::ProxyCommand::CacheClear {
                        host: clear_host,
                        port: clear_port,
                        session_id,
                        api_key,
                        remote: _,
                    } => {
                        let (clear_host, clear_port, key) = target
                            .proxy_endpoint(ctx, clear_host, clear_port, api_key)
                            .await?;
                        handlers::proxy_cache_clear::execute(
                            &clear_host,
                            clear_port,
                            session_id.as_deref(),
                            key.as_deref(),
                        )
                        .await?;
                    }
                    crate::commands::ProxyCommand::Stop => {
                        handlers::inference::proxy::stop().await?;
                    }
                    crate::commands::ProxyCommand::Test { model } => {
                        handlers::proxy_test::execute(ctx, model.as_deref()).await?;
                    }
                    crate::commands::ProxyCommand::Trips { since } => {
                        handlers::proxy_trips::execute(ctx, since).await?;
                    }
                }
                return Ok(());
            }

            handlers::inference::proxy::execute(
                ctx,
                bind.host,
                bind.port,
                bind.default_context,
                sampling,
                cache,
                access,
            )
            .await?;
        }

        // ── Remote tunnel (ADR 0012) ────────────────────────────────────────
        Commands::Remote { command } => handlers::remote::dispatch(ctx, command).await?,
        // ── Runs: replies the daemon owns ───────────────────────────────────
        Commands::Run(args) => handlers::run::dispatch(args).await?,
        // ── Drawing, through this machine's daemon or the paired one's ──────
        Commands::Image(args) => handlers::image::execute(ctx, target, args).await?,
        // ── Stored images ───────────────────────────────────────────────────
        Commands::Attachment { command } => match command {
            crate::commands::AttachmentCommand::Save { id, path, force } => {
                handlers::attachment::execute(ctx, &id, path.as_deref(), force).await?;
            }
        },

        // ── MCP tool gateway ────────────────────────────────────────────────
        Commands::Mcp { command } => {
            handlers::mcp_cli::dispatch(ctx, command).await?;
        }

        // ── Benchmarking ────────────────────────────────────────────────────
        Commands::Benchmark { command } => {
            handlers::benchmark::dispatch(ctx, command).await?;
        }

        // ── Shell completions ───────────────────────────────────────────────
        Commands::Completions { shell } => {
            handlers::completions::execute(shell)?;
        }
    }

    Ok(())
}
