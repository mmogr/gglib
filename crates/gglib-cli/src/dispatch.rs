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
            jinja,
            port,
            sampling,
        } => {
            handlers::inference::serve::execute(ctx, id, context, jinja, port, sampling).await?;
        }
        Commands::Chat {
            identifier,
            context,
            system_prompt,
            sampling,
            no_tools,
            port,
            max_iterations,
            tools,
            tool_timeout_ms,
            max_parallel,
            model,
            continue_id,
            command,
        } => {
            // Subcommand takes priority (e.g. `gglib chat history`)
            if let Some(sub) = command {
                match sub {
                    crate::commands::ChatCommand::History { limit } => {
                        handlers::history::execute(ctx, limit).await?;
                    }
                    crate::commands::ChatCommand::Council {
                        topic,
                        suggest,
                        edit,
                        config,
                        agent_count,
                        model,
                        port,
                    } => {
                        if suggest {
                            handlers::council::execute_suggest(
                                ctx,
                                &topic,
                                port,
                                agent_count,
                                model,
                            )
                            .await?;
                        } else if let Some(config_path) = config {
                            if edit {
                                handlers::council::execute_edit(
                                    ctx,
                                    &config_path,
                                    &topic,
                                    port,
                                    model,
                                )
                                .await?;
                            } else {
                                handlers::council::execute_run(
                                    ctx,
                                    &config_path,
                                    &topic,
                                    port,
                                    model,
                                )
                                .await?;
                            }
                        } else {
                            handlers::council::execute_interactive(
                                ctx,
                                &topic,
                                port,
                                agent_count,
                                model,
                            )
                            .await?;
                        }
                    }
                }
            } else {
                let args = handlers::inference::chat::ChatArgs {
                    identifier,
                    context,
                    system_prompt,
                    sampling,
                    no_tools,
                    port,
                    max_iterations,
                    tools,
                    tool_timeout_ms,
                    max_parallel,
                    verbose, // global flag forwarded here
                    model,
                    continue_id,
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
        } => {
            // When --no-tools is set, override tools to an empty allowlist
            // so the agent loop exposes zero tools to the model.
            let effective_tools = if no_tools {
                vec!["__none__".into()]
            } else {
                tools
            };

            handlers::inference::agent_question::execute(
                ctx,
                question,
                model,
                file,
                port,
                max_iterations,
                effective_tools,
                tool_timeout_ms,
                max_parallel,
                verbose,
                quiet,
                sampling,
                context,
            )
            .await?;
        }

        // ── GUI / web interfaces ────────────────────────────────────────────
        Commands::Gui { dev } => {
            handlers::gui::execute(dev)?;
        }
        Commands::Web {
            port,
            base_port,
            api_only,
            static_dir,
        } => {
            handlers::web::execute(port, base_port, api_only, static_dir).await?;
        }
        Commands::Proxy {
            host,
            port,
            llama_port,
            default_context,
        } => {
            gglib_runtime::proxy::start_proxy_standalone(
                host,
                port,
                llama_port,
                ctx.llama_server_path.clone(),
                ctx.model_repo.clone(),
                default_context,
                ctx.mcp.clone(),
            )
            .await?;
        }

        // ── MCP tool gateway ────────────────────────────────────────────────
        Commands::Mcp { command } => {
            handlers::mcp_cli::dispatch(ctx, command).await?;
        }

        // ── Shell completions ───────────────────────────────────────────────
        Commands::Completions { shell } => {
            handlers::completions::execute(shell)?;
        }
    }

    Ok(())
}
