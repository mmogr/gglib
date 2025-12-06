//! CLI entry point - the composition root.
//!
//! This is the ONLY place where infrastructure is wired together:
//! - Database pool creation
//! - Repository instantiation  
//! - AppCore construction
//! - Command dispatch
//!
//! All other CLI code delegates to AppCore without touching infrastructure.

use clap::Parser;
use std::sync::Arc;

use gglib_cli::{
    AssistantUiCommand, Cli, Commands, ConfigCommand, LlamaCommand,
    ModelsDirCommand, SettingsCommand,
};

// Import handlers from root gglib crate (temporary until handlers migrate here)
use gglib::commands;
use gglib::services::{database, AppCore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Set up database pool (composition root responsibility)
    let pool = database::setup_database().await?;

    // Create AppCore (the facade that handlers use)
    let core = Arc::new(AppCore::new(pool));

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
            commands::add::handle_add(Arc::clone(&core), file_path).await?;
        }
        Commands::List => {
            commands::list::handle_list(Arc::clone(&core)).await?;
        }
        Commands::Remove { identifier, force } => {
            commands::remove::handle_remove(Arc::clone(&core), identifier, force).await?;
        }
        Commands::Serve {
            id,
            ctx_size,
            mlock,
            jinja,
            port,
        } => {
            commands::serve::handle_serve(Arc::clone(&core), id, ctx_size, mlock, jinja, port)
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
        } => {
            commands::chat::handle_chat(
                Arc::clone(&core),
                commands::chat::ChatCommandArgs {
                    identifier,
                    ctx_size,
                    mlock,
                    chat_template,
                    chat_template_file,
                    jinja,
                    system_prompt,
                    multiline_input,
                    simple_io,
                },
            )
            .await?;
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
                Arc::clone(&core),
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
            commands::download::handle_check_updates(Arc::clone(&core), model_id, all).await?;
        }
        Commands::UpdateModel { model_id, force } => {
            commands::download::handle_update_model(Arc::clone(&core), model_id, force).await?;
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
            let args = commands::update::UpdateArgs {
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
            commands::update::handle_update(Arc::clone(&core), args).await?;
        }
        Commands::Config { command } => {
            // Convert gglib-cli ConfigCommand to legacy cli::ConfigCommand
            let legacy_command = match command {
                ConfigCommand::ModelsDir { command } => {
                    let legacy_dir_cmd = match command {
                        ModelsDirCommand::Show => gglib::cli::ModelsDirCommand::Show,
                        ModelsDirCommand::Prompt => gglib::cli::ModelsDirCommand::Prompt,
                        ModelsDirCommand::Set { path, no_create } => {
                            gglib::cli::ModelsDirCommand::Set { path, no_create }
                        }
                    };
                    gglib::cli::ConfigCommand::ModelsDir {
                        command: legacy_dir_cmd,
                    }
                }
                ConfigCommand::Settings { command } => {
                    let legacy_settings_cmd = match command {
                        SettingsCommand::Show => gglib::cli::SettingsCommand::Show,
                        SettingsCommand::Set {
                            default_context_size,
                            proxy_port,
                            server_port,
                            max_download_queue_size,
                            default_download_path,
                        } => gglib::cli::SettingsCommand::Set {
                            default_context_size,
                            proxy_port,
                            server_port,
                            max_download_queue_size,
                            default_download_path,
                        },
                        SettingsCommand::Reset { force } => {
                            gglib::cli::SettingsCommand::Reset { force }
                        }
                    };
                    gglib::cli::ConfigCommand::Settings {
                        command: legacy_settings_cmd,
                    }
                }
            };
            commands::config::handle(Arc::clone(&core), legacy_command)?;
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
            gglib::proxy::start_proxy(host, port, core.models(), llama_port, default_context)
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
