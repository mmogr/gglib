//! CLI entry point - the composition root.
//!
//! This is the ONLY place where infrastructure is wired together via bootstrap.
//! Command dispatch routes to handlers which delegate to AppCore.
//!
//! All CLI code uses CliContext for dependency access - no direct
//! database or pool access outside of bootstrap.

use clap::Parser;
use std::sync::Arc;

use gglib_cli::{
    AssistantUiCommand, Cli, CliConfig, Commands, LlamaCommand, bootstrap, handlers,
};

// Legacy imports for commands not yet migrated (TODO: migrate in subsequent PRs)
use gglib::commands;
use gglib::services::AppCore as LegacyAppCore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Bootstrap the CLI context (composition root)
    let config = CliConfig::with_defaults()?;
    let ctx = bootstrap(config).await?;

    // Create legacy AppCore for commands not yet migrated
    // TODO: Remove once all commands use CliContext
    let legacy_core = Arc::new(LegacyAppCore::new(
        gglib::services::database::setup_database().await?,
    ));

    // Dispatch to appropriate handler
    let Some(command) = cli.command else {
        // No command provided - show help
        use clap::CommandFactory;
        gglib_cli::Cli::command().print_help()?;
        return Ok(());
    };

    match command {
        Commands::CheckDeps => {
            commands::check_deps::handle_check_deps().await?;
        }
        Commands::Add { file_path } => {
            // NEW: Uses CliContext
            handlers::add::execute(&ctx, &file_path).await?;
        }
        Commands::List => {
            // NEW: Uses CliContext
            handlers::list::execute(&ctx).await?;
        }
        Commands::Remove { identifier, force } => {
            // NEW: Uses CliContext
            handlers::remove::execute(&ctx, &identifier, force).await?;
        }
        Commands::Serve {
            id,
            ctx_size,
            mlock,
            jinja,
            port,
        } => {
            // NEW: Uses CliContext
            handlers::serve::execute(&ctx, id, ctx_size, mlock, jinja, port).await?;
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
        } => {
            // NEW: Uses CliContext
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
            };
            handlers::chat::execute(&ctx, args).await?;
        }
        Commands::Download {
            model_id,
            quantization,
            list_quants,
            skip_db,
            token,
            force,
        } => {
            commands::download::execute(
                Arc::clone(&legacy_core),
                model_id,
                quantization,
                list_quants,
                !skip_db, // add_to_db: true by default, false if --skip-db passed
                token,
                force,
                None, // progress_callback
                None, // cancel_token (CLI uses Ctrl+C)
                None, // pid_storage
                None, // pid_key
            )
            .await?;
        }
        Commands::CheckUpdates { model_id, all } => {
            commands::download::handle_check_updates(Arc::clone(&legacy_core), model_id, all).await?;
        }
        Commands::UpdateModel { model_id, force } => {
            commands::download::handle_update_model(Arc::clone(&legacy_core), model_id, force).await?;
        }
        Commands::Search {
            query,
            limit,
            sort,
            gguf_only,
        } => {
            commands::download::handle_search(query, limit, sort, gguf_only).await?;
        }
        Commands::Browse {
            category,
            limit,
            size,
        } => {
            commands::download::handle_browse(category, limit, size).await?;
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
            dry_run,
            force,
        } => {
            // NEW: Uses CliContext
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
                dry_run,
                force,
            };
            handlers::update::execute(&ctx, args).await?;
        }
        Commands::Config { command } => {
            // NEW: Uses CliContext
            handlers::config::execute(&ctx, command).await?;
        }
        Commands::Llama { command } => match command {
            LlamaCommand::Install {
                cuda,
                metal,
                cpu_only,
                force,
                build,
            } => {
                commands::llama::handle_install(cuda, metal, cpu_only, force, build).await?;
            }
            LlamaCommand::CheckUpdates => {
                commands::llama::handle_check_updates().await?;
            }
            LlamaCommand::Update => {
                commands::llama::handle_update().await?;
            }
            LlamaCommand::Status => {
                commands::llama::handle_status().await?;
            }
            LlamaCommand::Rebuild {
                cuda,
                metal,
                cpu_only,
            } => {
                commands::llama::handle_rebuild(cuda, metal, cpu_only).await?;
            }
            LlamaCommand::Uninstall { force } => {
                commands::llama::handle_uninstall(force).await?;
            }
        },
        Commands::Gui { dev } => {
            // Tauri GUI - handled differently
            if dev {
                println!("Development mode requires running 'cargo tauri dev' directly");
            } else {
                println!("Desktop GUI requires running 'cargo tauri build' first");
            }
        }
        Commands::Web { port, base_port } => {
            println!("Starting web GUI server on http://localhost:{}", port);
            println!("Press Ctrl+C to stop");
            commands::gui_web::start_web_server(port, base_port, 5).await?;
        }
        Commands::Proxy {
            host,
            port,
            llama_port,
            default_context,
        } => {
            gglib::proxy::start_proxy(host, port, legacy_core.models(), llama_port, default_context)
                .await?;
        }
        Commands::AssistantUi { command } => match command {
            AssistantUiCommand::Install => {
                commands::assistant_ui::handle_install().map_err(|e| anyhow::anyhow!(e))?;
            }
            AssistantUiCommand::Update => {
                commands::assistant_ui::handle_update().map_err(|e| anyhow::anyhow!(e))?;
            }
            AssistantUiCommand::Status => {
                commands::assistant_ui::handle_status().map_err(|e| anyhow::anyhow!(e))?;
            }
        },
    }

    Ok(())
}
