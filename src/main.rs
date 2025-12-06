//! GGLib Library Management CLI Application
//!
//! This binary provides a command-line interface for managing GGUF model files.
//! It supports adding models to a local database, listing stored models,
//! serving models via llama-server, and running an OpenAI-compatible proxy.
//! It also serves as the launcher for the Desktop and Web GUIs.

use anyhow::Result;
use clap::Parser;
use gglib::services::{AppCore, database};
use gglib::{cli, commands};
use std::sync::Arc;

/// The main entry point for the GGUF library management CLI application.
///
/// Parses command-line arguments and dispatches to the appropriate command handler.
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
///
/// # Errors
///
/// Returns an error if:
/// - Command-line arguments are invalid
/// - Command execution fails
#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    // Parse CLI args first to check for verbose flag
    let cli = cli::Cli::parse();

    // Initialize tracing/logging
    // Priority: RUST_LOG env var > --verbose flag > default (warn)
    let default_level = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level)),
        )
        .init();

    if let Some(ref override_path) = cli.models_dir {
        let resolution = gglib::utils::paths::resolve_models_dir(Some(override_path))?;
        gglib::utils::paths::ensure_directory(
            &resolution.path,
            gglib::utils::paths::DirectoryCreationStrategy::AutoCreate,
        )?;
        // SAFETY: Setting environment variable early in main() before any threads are spawned.
        // This is a global configuration that affects all subsequent operations in the application.
        // While inherently unsafe in multi-threaded programs, this is acceptable here as it occurs
        // during initialization before the tokio runtime creates additional threads.
        unsafe {
            std::env::set_var(
                "GGLIB_MODELS_DIR",
                resolution.path.to_string_lossy().to_string(),
            );
        }
    }

    match cli.command {
        Some(command) => {
            // Create centralized AppCore for commands that need database access
            // Some commands (CheckDeps, Gui, Llama, AssistantUi) don't need it
            // but creating it is cheap and simplifies the interface
            let pool = database::setup_database().await?;
            let core = Arc::new(AppCore::new(pool));
            run_command(core, command).await
        }
        None => {
            println!("Use --help to see available commands");
            Ok(())
        }
    }
}

/// Execute a command by dispatching to the appropriate handler
async fn run_command(core: Arc<AppCore>, command: cli::Commands) -> Result<()> {
    use cli::{AssistantUiCommand, Commands, LlamaCommand};

    match command {
        Commands::CheckDeps => commands::check_deps::handle_check_deps().await,
        Commands::Add { file_path } => commands::add::handle_add(core, file_path).await,
        Commands::Download {
            model_id,
            quantization,
            list_quants,
            skip_db,
            token,
            force,
        } => {
            commands::download::execute(
                core,
                model_id,
                quantization,
                list_quants,
                !skip_db, // add_to_db: true by default, false if --skip-db passed
                token,
                force,
                None,
                None, // CLI uses Ctrl+C for cancellation
                None, // No PID storage for CLI
                None, // No PID key for CLI
            )
            .await
        }
        Commands::CheckUpdates { model_id, all } => {
            commands::download::handle_check_updates(core, model_id, all).await
        }
        Commands::UpdateModel { model_id, force } => {
            commands::download::handle_update_model(core, model_id, force).await
        }
        Commands::Search {
            query,
            limit,
            sort,
            gguf_only,
        } => commands::download::handle_search(query, limit, sort, gguf_only).await,
        Commands::Browse {
            category,
            limit,
            size,
        } => commands::download::handle_browse(category, limit, size).await,
        Commands::List => commands::list::handle_list(core).await,
        Commands::Remove { identifier, force } => {
            commands::remove::handle_remove(core, identifier, force).await
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
            commands::update::handle_update(core, args).await
        }
        Commands::Serve {
            id,
            ctx_size,
            mlock,
            jinja,
            port,
        } => commands::serve::handle_serve(core, id, ctx_size, mlock, jinja, port).await,
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
                core,
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
            .await
        }
        Commands::Gui { dev } => {
            if dev {
                println!("Launching Tauri GUI in development mode...");
                std::process::Command::new("npm")
                    .args(["run", "tauri:dev"])
                    .status()?;
            } else {
                // Try multiple locations for the GUI binary:
                // 1. The compile-time repo root (for development builds)
                // 2. The runtime resource root (for installed binaries)
                let compile_time_repo = std::path::PathBuf::from(env!("GGLIB_REPO_ROOT"));
                let runtime_root = gglib::utils::paths::get_resource_root()?;

                // Helper to find binary in a given root
                let find_binary = |root: &std::path::Path| -> Option<std::path::PathBuf> {
                    #[cfg(target_os = "macos")]
                    {
                        let bundled = root.join(
                            "src-tauri/target/release/bundle/macos/GGLib GUI.app/Contents/MacOS/gglib-gui",
                        );
                        let unbundled = root.join("src-tauri/target/release/gglib-gui");
                        if bundled.exists() {
                            Some(bundled)
                        } else if unbundled.exists() {
                            Some(unbundled)
                        } else {
                            None
                        }
                    }
                    #[cfg(target_os = "linux")]
                    {
                        let appimage = root
                            .join("src-tauri/target/release/bundle/appimage/gglib-gui.AppImage");
                        let deb_binary = root.join("src-tauri/target/release/gglib-gui");
                        if appimage.exists() {
                            Some(appimage)
                        } else if deb_binary.exists() {
                            Some(deb_binary)
                        } else {
                            None
                        }
                    }
                    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                    {
                        let binary = root.join("src-tauri/target/release/gglib-gui.exe");
                        if binary.exists() { Some(binary) } else { None }
                    }
                };

                // Try compile-time repo first, then runtime root
                let binary_path =
                    find_binary(&compile_time_repo).or_else(|| find_binary(&runtime_root));

                match binary_path {
                    Some(path) => {
                        println!("Launching Tauri GUI from {}...", path.display());
                        let canonical_binary_path = std::fs::canonicalize(&path)?;
                        std::process::Command::new(canonical_binary_path).spawn()?;
                    }
                    None => {
                        eprintln!("Error: Desktop GUI binary not found.");
                        eprintln!();
                        eprintln!("Searched in:");
                        eprintln!("  - {}", compile_time_repo.display());
                        if runtime_root != compile_time_repo {
                            eprintln!("  - {}", runtime_root.display());
                        }
                        eprintln!();
                        eprintln!("The Tauri desktop app needs to be built first.");
                        eprintln!();
                        eprintln!("Build the desktop GUI:");
                        eprintln!("  cd {} && make build-tauri", compile_time_repo.display());
                        eprintln!();
                        eprintln!("Or use the web GUI instead:");
                        eprintln!("  gglib web");
                        eprintln!("  # Then open http://localhost:9887 in your browser");
                        std::process::exit(1);
                    }
                }
            }
            Ok(())
        }
        Commands::Web { port, base_port } => {
            println!("Starting web GUI server on http://localhost:{}", port);
            println!("Press Ctrl+C to stop");
            commands::gui_web::start_web_server(port, base_port, 5).await?;
            Ok(())
        }
        Commands::Proxy {
            host,
            port,
            llama_port,
            default_context,
        } => {
            gglib::proxy::start_proxy(host, port, core.models(), llama_port, default_context).await
        }
        Commands::Llama { command } => match command {
            LlamaCommand::Install {
                cuda,
                metal,
                cpu_only,
                force,
                build,
            } => commands::llama::handle_install(cuda, metal, cpu_only, force, build).await,
            LlamaCommand::CheckUpdates => commands::llama::handle_check_updates().await,
            LlamaCommand::Update => commands::llama::handle_update().await,
            LlamaCommand::Status => commands::llama::handle_status().await,
            LlamaCommand::Rebuild {
                cuda,
                metal,
                cpu_only,
            } => commands::llama::handle_rebuild(cuda, metal, cpu_only).await,
            LlamaCommand::Uninstall { force } => commands::llama::handle_uninstall(force).await,
        },
        Commands::AssistantUi { command } => match command {
            AssistantUiCommand::Install => {
                commands::assistant_ui::handle_install().map_err(|e| anyhow::anyhow!(e))
            }
            AssistantUiCommand::Update => {
                commands::assistant_ui::handle_update().map_err(|e| anyhow::anyhow!(e))
            }
            AssistantUiCommand::Status => {
                commands::assistant_ui::handle_status().map_err(|e| anyhow::anyhow!(e))
            }
        },
        Commands::Config { command } => commands::config::handle(core, command),
    }
}
