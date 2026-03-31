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
use gglib_runtime::DefaultSystemProbe;

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
        // ── System / introspection ──────────────────────────────────────────
        Commands::CheckDeps => {
            let probe = DefaultSystemProbe::new();
            handlers::check_deps::execute(&probe).await?;
        }
        Commands::Paths => {
            handlers::paths::execute()?;
        }

        // ── Model library management ────────────────────────────────────────
        Commands::Add { file_path } => {
            handlers::add::execute(ctx, &file_path).await?;
        }
        Commands::List => {
            handlers::list::execute(ctx).await?;
        }
        Commands::Remove { identifier, force } => {
            handlers::remove::execute(ctx, &identifier, force).await?;
        }
        Commands::Update {
            id,
            name,
            param_count,
            architecture,
            quantization,
            context_length,
            metadata,
            remove_metadata,
            replace_metadata,
            temperature,
            top_p,
            top_k,
            max_tokens,
            repeat_penalty,
            clear_inference_defaults,
            dry_run,
            force,
        } => {
            let args = handlers::update::UpdateArgs {
                id,
                name,
                param_count,
                architecture,
                quantization,
                context_length,
                metadata,
                remove_metadata,
                replace_metadata,
                temperature,
                top_p,
                top_k,
                max_tokens,
                repeat_penalty,
                clear_inference_defaults,
                dry_run,
                force,
            };
            handlers::update::execute(ctx, args).await?;
        }
        Commands::Verify { model_id, verbose } => {
            handlers::verification::execute_verify(ctx, model_id, verbose).await?;
        }
        Commands::Repair {
            model_id,
            shards,
            force,
        } => {
            handlers::verification::execute_repair(ctx, model_id, shards, force).await?;
        }

        // ── HuggingFace / download ──────────────────────────────────────────
        Commands::Download {
            model_id,
            quantization,
            list_quants,
            skip_db: _skip_db,
            token,
            force,
        } => {
            let args = handlers::download::DownloadArgs {
                model_id: &model_id,
                quantization: quantization.as_deref(),
                list_quants,
                force,
                token: token.as_deref(),
            };
            handlers::download::download(ctx, args).await?;
        }
        Commands::CheckUpdates { model_id, all } => {
            handlers::download::check_updates(ctx, model_id, all).await?;
        }
        Commands::UpdateModel {
            model_id,
            force: _force,
        } => {
            handlers::download::update_model(ctx, model_id).await?;
        }
        Commands::Search {
            query,
            limit,
            sort,
            gguf_only,
        } => {
            handlers::download::search(query, limit, sort, gguf_only).await?;
        }
        Commands::Browse {
            category,
            limit,
            size,
        } => {
            handlers::download::browse(category, limit, size).await?;
        }

        // ── Inference ───────────────────────────────────────────────────────
        Commands::Serve {
            id,
            ctx_size,
            mlock,
            jinja,
            port,
            temperature,
            top_p,
            top_k,
            max_tokens,
            repeat_penalty,
        } => {
            handlers::serve::execute(
                ctx,
                id,
                ctx_size,
                mlock,
                jinja,
                port,
                temperature,
                top_p,
                top_k,
                max_tokens,
                repeat_penalty,
            )
            .await?;
        }
        Commands::Chat {
            identifier,
            ctx_size,
            mlock,
            chat_template,
            chat_template_file,
            jinja,
            system_prompt,
            multiline_input,
            simple_io,
            temperature,
            top_p,
            top_k,
            max_tokens,
            repeat_penalty,
            agent,
            port,
            max_iterations,
            tools,
            tool_timeout_ms,
            max_parallel,
            model,
        } => {
            let args = handlers::chat::ChatArgs {
                identifier,
                ctx_size,
                mlock,
                chat_template,
                chat_template_file,
                jinja,
                system_prompt,
                multiline_input,
                simple_io,
                temperature,
                top_p,
                top_k,
                max_tokens,
                repeat_penalty,
                agent,
                port,
                max_iterations,
                tools,
                tool_timeout_ms,
                max_parallel,
                verbose, // global flag forwarded here
                model,
            };
            handlers::chat::execute(ctx, args).await?;
        }
        Commands::Question {
            question,
            model,
            file,
            ctx_size,
            mlock,
            verbose,
            quiet,
            temperature,
            top_p,
            top_k,
            max_tokens,
            repeat_penalty,
        } => {
            handlers::question::execute(
                ctx,
                question,
                model,
                file,
                ctx_size,
                mlock,
                verbose,
                quiet,
                temperature,
                top_p,
                top_k,
                max_tokens,
                repeat_penalty,
            )
            .await?;
        }

        // ── Configuration ───────────────────────────────────────────────────
        Commands::Config { command } => {
            handlers::config::execute(ctx, command).await?;
        }

        // ── llama.cpp lifecycle ─────────────────────────────────────────────
        Commands::Llama { command } => {
            handlers::llama::dispatch(command).await?;
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
        Commands::Mcp { command } => {
            handlers::mcp_cli::dispatch(ctx, command).await?;
        }
        Commands::AssistantUi { command } => {
            handlers::assistant_ui::dispatch(command)?;
        }
    }

    Ok(())
}
