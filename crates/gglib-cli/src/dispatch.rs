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
use gglib_core::domain::inference::InferenceConfig;
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};

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
            options,
            sampling,
            mtp,
        } => {
            handlers::inference::serve::execute(ctx, id, context, options, sampling, mtp, verbose)
                .await?;
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
                observation_tools,
                max_observation_steps,
                verbose,
                quiet,
                sampling,
                context,
            )
            .await?;
        }

        // ── GUI / web interfaces ────────────────────────────────────────────
        Commands::Plan {
            goal,
            model,
            port,
            max_replans,
            context,
        } => {
            handlers::plan::execute(ctx, &goal, port, model, context.ctx_size, max_replans).await?;
        }

        Commands::Council { cmd } => {
            use crate::commands::CouncilCmd;
            match cmd {
                CouncilCmd::Run {
                    goal,
                    model,
                    port,
                    max_replans,
                    max_iterations,
                    hitl,
                    approval_timeout,
                    approval_timeout_action,
                    json,
                    sampling,
                    context,
                } => {
                    handlers::council::run::execute(
                        ctx,
                        &goal,
                        port,
                        model,
                        context.ctx_size,
                        max_replans,
                        max_iterations,
                        sampling,
                        hitl.as_deref(),
                        approval_timeout,
                        &approval_timeout_action,
                        json,
                    )
                    .await?;
                }
                CouncilCmd::List { status } => {
                    handlers::council::list::execute(ctx, status.as_deref()).await?;
                }
                CouncilCmd::Show { run_id } => {
                    handlers::council::show::execute(ctx, &run_id).await?;
                }
                CouncilCmd::Resume {
                    run_id,
                    model,
                    port,
                    max_replans,
                    max_iterations,
                    hitl,
                    approval_timeout,
                    approval_timeout_action,
                    json,
                    sampling,
                    context,
                } => {
                    handlers::council::resume::execute(
                        ctx,
                        &run_id,
                        port,
                        model,
                        context.ctx_size,
                        max_replans,
                        max_iterations,
                        sampling,
                        hitl.as_deref(),
                        approval_timeout,
                        &approval_timeout_action,
                        json,
                    )
                    .await?;
                }
                CouncilCmd::Rewind {
                    run_id,
                    wave,
                    note,
                    model,
                    port,
                    context,
                } => {
                    handlers::council::rewind::execute(
                        ctx,
                        &run_id,
                        wave,
                        note.as_deref(),
                        port,
                        model,
                        context.ctx_size,
                    )
                    .await?;
                }
            }
        }

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
            temperature,
            top_p,
            top_k,
            max_tokens,
            repeat_penalty,
            presence_penalty,
            min_p,
            cache,
            slot_dir,
            cache_ram_mb,
            cache_reuse,
            command,
        } => {
            // Subcommand takes priority (e.g. `gglib proxy dashboard`) — it
            // connects to an already-running proxy rather than starting one.
            if let Some(sub) = command {
                match sub {
                    crate::commands::ProxyCommand::Dashboard {
                        host: dash_host,
                        port: dash_port,
                    } => {
                        handlers::proxy_dashboard::execute(dash_host, dash_port).await?;
                    }
                    crate::commands::ProxyCommand::CacheClear {
                        host: clear_host,
                        port: clear_port,
                        session_id,
                    } => {
                        handlers::proxy_cache_clear::execute(
                            &clear_host,
                            clear_port,
                            session_id.as_deref(),
                        )
                        .await?;
                    }
                }
                return Ok(());
            }

            let settings = ctx.app.settings().get().await?;
            let effective_context = resolve_context_size(&ServerConfigOptions {
                context_size: default_context
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok()),
                global_default_ctx: settings.default_context_size,
                ..Default::default()
            });
            let inference_override = if temperature.is_some()
                || top_p.is_some()
                || top_k.is_some()
                || max_tokens.is_some()
                || repeat_penalty.is_some()
                || presence_penalty.is_some()
                || min_p.is_some()
            {
                Some(InferenceConfig {
                    temperature,
                    top_p,
                    top_k,
                    max_tokens,
                    repeat_penalty,
                    presence_penalty,
                    min_p,
                })
            } else {
                None
            };
            gglib_runtime::proxy::start_proxy_standalone(
                host,
                port,
                llama_port,
                ctx.llama_server_path.clone(),
                ctx.model_repo.clone(),
                effective_context,
                ctx.mcp.clone(),
                ctx.app.settings().repo(),
                inference_override,
                cache,
                slot_dir,
                cache_ram_mb,
                cache_reuse,
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
