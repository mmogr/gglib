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

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::commands::Commands;
use crate::handlers;

/// Route a parsed command to its handler.
///
/// # Arguments
///
/// * `ctx`     — Shared reference to the fully-composed CLI context.
/// * `command` — The command to execute, as parsed by Clap.
/// * `verbose` — Value of the global `--verbose` flag; forwarded only to
///   handlers that expose a verbosity knob.
pub async fn dispatch(ctx: &CliContext, command: Commands, verbose: bool) -> Result<()> {
    match command {
        // ── Getting started ─────────────────────────────────────────────────
        Commands::Up { yes, model, port } => {
            handlers::up::execute(ctx, handlers::up::UpArgs { yes, model, port }).await?;
        }

        // ── Grouped: model management ───────────────────────────────────────
        Commands::Model { command } => {
            handlers::model::dispatch(ctx, command).await?;
        }

        // ── Grouped: configuration & system ─────────────────────────────────
        Commands::Config { command } => {
            handlers::config::dispatch(ctx, command).await?;
        }

        // ── Inference (top-level for ergonomic access) ──────────────────────
        Commands::Serve {
            id,
            context,
            options,
            sampling,
            mtp,
            cache,
            access,
        } => {
            handlers::inference::serve::execute(
                ctx, id, context, options, sampling, mtp, cache, access, verbose,
            )
            .await?;
        }
        Commands::Chat {
            identifier,
            context,
            system_prompt,
            sampling,
            retry,
            no_tools,
            port,
            max_iterations,
            tools,
            tool_timeout_ms,
            max_parallel,
            model,
            continue_id,
            observation_tools,
            max_observation_steps,
            command,
        } => {
            // Subcommand takes priority (e.g. `gglib chat history`)
            if let Some(sub) = command {
                match sub {
                    crate::commands::ChatCommand::History { limit } => {
                        handlers::history::execute(ctx, limit).await?;
                    }
                }
            } else {
                let args = handlers::inference::chat::ChatArgs {
                    identifier,
                    context,
                    system_prompt,
                    sampling,
                    retry_policy: retry.into_policy(),
                    no_tools,
                    port,
                    max_iterations,
                    tools,
                    tool_timeout_ms,
                    max_parallel,
                    verbose, // global flag forwarded here
                    model,
                    continue_id,
                    observation_tools,
                    max_observation_steps,
                    // Filled from persisted settings in agent_chat::run.
                    max_stagnation_steps: None,
                };
                handlers::inference::chat::execute(ctx, args).await?;
            }
        }

        Commands::Question {
            question,
            model,
            file,
            context,
            verbose,
            quiet,
            sampling,
            no_tools,
            port,
            max_iterations,
            tools,
            tool_timeout_ms,
            max_parallel,
            observation_tools,
            max_observation_steps,
        } => {
            // When --no-tools is set, override tools to an empty allowlist
            // so the agent loop exposes zero tools to the model.
            let effective_tools = if no_tools {
                vec!["__none__".into()]
            } else {
                tools
            };

            let args = handlers::inference::agent_question::QuestionArgs {
                question,
                model_arg: model,
                file,
                port,
                max_iterations,
                tools: effective_tools,
                tool_timeout_ms,
                max_parallel,
                observation_tools,
                max_observation_steps,
                verbose,
                quiet,
                sampling,
                context,
            };
            handlers::inference::agent_question::execute(ctx, args).await?;
        }

        // ── GUI / web interfaces ────────────────────────────────────────────
        Commands::Gui { dev } => {
            handlers::gui::execute(dev)?;
        }
        Commands::Web { share_lan } => {
            handlers::web::execute(share_lan).await?;
        }
        Commands::Daemon { command } => match command {
            crate::commands::DaemonCommand::Run {
                share_lan,
                allowed_host,
            } => {
                handlers::daemon::run(share_lan, allowed_host).await?;
            }
            crate::commands::DaemonCommand::Status => {
                handlers::daemon::status().await?;
            }
            crate::commands::DaemonCommand::Stop => {
                handlers::daemon::stop().await?;
            }
        },
        Commands::Proxy {
            host,
            port,
            default_context,
            sampling,
            cache,
            access,
            command,
        } => {
            // Subcommand takes priority (e.g. `gglib proxy dashboard`) — it
            // connects to an already-running proxy rather than starting one.
            if let Some(sub) = command {
                match sub {
                    crate::commands::ProxyCommand::Dashboard {
                        host: dash_host,
                        port: dash_port,
                        api_key,
                    } => {
                        let key = resolve_client_api_key(ctx, api_key).await;
                        handlers::proxy_dashboard::execute(dash_host, dash_port, key.as_deref())
                            .await?;
                    }
                    crate::commands::ProxyCommand::CacheClear {
                        host: clear_host,
                        port: clear_port,
                        session_id,
                        api_key,
                    } => {
                        let key = resolve_client_api_key(ctx, api_key).await;
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
                }
                return Ok(());
            }

            handlers::inference::proxy::execute(
                ctx,
                host,
                port,
                default_context,
                sampling,
                cache,
                access,
            )
            .await?;
        }

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

/// The key a `gglib proxy` subcommand should present to an already-running
/// proxy.
///
/// `--api-key`/`GGLIB_API_KEY` first, then the stored `proxy_api_key`. The
/// stored fallback is what makes `gglib proxy dashboard` keep working with no
/// extra flag against a proxy that generated its own key — the same settings
/// row the proxy wrote it to.
///
/// An unreadable settings store yields `None` rather than an error: the target
/// proxy may well be unauthenticated, and failing the command outright would
/// turn a maybe-irrelevant local problem into a hard stop.
async fn resolve_client_api_key(ctx: &CliContext, flag: Option<String>) -> Option<String> {
    if flag.is_some() {
        return flag;
    }
    ctx.app
        .settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.proxy_api_key)
        .filter(|key| !key.trim().is_empty())
}
