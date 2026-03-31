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
            chat_template,
            chat_template_file,
            jinja,
            system_prompt,
            multiline_input,
            simple_io,
            sampling,
            agent,
            port,
            max_iterations,
            tools,
            tool_timeout_ms,
            max_parallel,
            model,
        } => {
            let args = handlers::inference::chat::ChatArgs {
                identifier,
                context,
                chat_template,
                chat_template_file,
                jinja,
                system_prompt,
                multiline_input,
                simple_io,
                sampling,
                agent,
                port,
                max_iterations,
                tools,
                tool_timeout_ms,
                max_parallel,
                verbose, // global flag forwarded here
                model,
            };
            handlers::inference::chat::execute(ctx, args).await?;
        }
        Commands::Question {
            question,
            model,
            file,
            context,
            verbose,
            quiet,
            sampling,
        } => {
            handlers::inference::question::execute(
                ctx, question, model, file, context, verbose, quiet, sampling,
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
    }

    Ok(())
}
