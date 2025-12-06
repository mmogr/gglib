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
        Commands::Add { file_path } => {
            // Migrated to gglib-cli; inline stub for legacy binary
            use gglib::gguf::apply_capability_detection;
            use gglib::models::Gguf;
            use gglib::utils::{input, validation};

            let gguf_metadata = validation::validate_and_parse_gguf(&file_path)?;
            println!("File validation and metadata extraction successful.");

            // Display extracted metadata
            println!("\nExtracted metadata:");
            if let Some(ref name) = gguf_metadata.name {
                println!("  Name: {name}");
            }
            if let Some(ref arch) = gguf_metadata.architecture {
                println!("  Architecture: {arch}");
            }
            if let Some(params) = gguf_metadata.param_count_b {
                println!("  Parameters: {params:.1}B");
            }
            if let Some(ref quant) = gguf_metadata.quantization {
                println!("  Quantization: {quant}");
            }
            if let Some(context) = gguf_metadata.context_length {
                println!("  Context Length: {context}");
            }

            // Prompt for name
            let name = if let Some(ref suggested) = gguf_metadata.name {
                input::prompt_string_with_default("Model name", Some(suggested))?
            } else {
                input::prompt_string("Model name")?
            };

            // Prompt for params
            let param_count_b = if let Some(params) = gguf_metadata.param_count_b {
                input::prompt_float_with_default("Parameter count (in billions)", Some(params))?
            } else {
                input::prompt_float("Parameter count (in billions)")?
            };

            let auto_tags = apply_capability_detection(&gguf_metadata.metadata);

            let new_model = Gguf {
                id: None,
                name,
                file_path: file_path.into(),
                param_count_b,
                architecture: gguf_metadata.architecture,
                quantization: gguf_metadata.quantization,
                context_length: gguf_metadata.context_length,
                metadata: gguf_metadata.metadata,
                added_at: chrono::Utc::now(),
                hf_repo_id: None,
                hf_commit_sha: None,
                hf_filename: None,
                download_date: None,
                last_update_check: None,
                tags: auto_tags,
            };

            core.models().add(&new_model).await?;
            println!("✅ Model '{}' added to database!", new_model.name);
            Ok(())
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
        Commands::List => {
            // Migrated to gglib-cli; inline stub for legacy binary
            let models = core.models().list().await?;
            if models.is_empty() {
                println!("No models found. Use 'gglib add <file_path>' to add your first model.");
            } else {
                println!("Found {} model(s):", models.len());
                for model in models {
                    println!(
                        "  [{}] {} - {}",
                        model.id.map(|id| id.to_string()).unwrap_or_else(|| "?".to_string()),
                        model.name,
                        model.file_path.display()
                    );
                }
            }
            Ok(())
        }
        Commands::Remove { identifier, force } => {
            // Migrated to gglib-cli; inline stub for legacy binary
            use gglib::utils::input;
            let models = core.models().find_by_name(&identifier).await?;
            if models.is_empty() {
                println!("No model found matching: '{identifier}'");
                return Ok(());
            }
            let model = &models[0];
            if !force {
                println!("Model: {} ({})", model.name, model.file_path.display());
                if !input::prompt_confirmation("Remove this model from the database?")? {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            if let Some(id) = model.id {
                core.models().remove(id).await?;
                println!("✅ Model '{}' removed.", model.name);
            }
            Ok(())
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
            // Migrated to gglib-cli; inline stub for legacy binary
            use std::io::{self, Write};

            // Get existing model
            let existing = core.models().get_by_id(id).await?;
            let mut updated = existing.clone();

            // Apply updates
            if let Some(n) = name {
                updated.name = n;
            }
            if let Some(p) = param_count {
                updated.param_count_b = p;
            }
            if let Some(a) = architecture {
                updated.architecture = Some(a);
            }
            if let Some(q) = quantization {
                updated.quantization = Some(q);
            }
            if let Some(c) = context_length {
                updated.context_length = Some(c);
            }

            // Handle metadata
            for meta_arg in &metadata {
                if let Some((key, value)) = meta_arg.split_once('=') {
                    if replace_metadata && metadata.iter().position(|m| m == meta_arg) == Some(0) {
                        updated.metadata.clear();
                    }
                    updated.metadata.insert(key.to_string(), value.to_string());
                }
            }
            if let Some(ref keys) = remove_metadata {
                for key in keys.split(',') {
                    updated.metadata.remove(key.trim());
                }
            }

            if dry_run {
                println!("🔍 Dry run - changes would be applied to model ID {}", id);
                return Ok(());
            }

            if !force {
                print!("Apply changes to model '{}'? [y/N]: ", existing.name);
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                if !input.trim().to_lowercase().starts_with('y') {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            core.models().update(id, &updated).await?;
            println!("✅ Model updated successfully!");
            Ok(())
        }
        Commands::Serve {
            id,
            ctx_size,
            mlock,
            jinja,
            port,
        } => {
            // Migrated to gglib-cli; inline stub for legacy binary
            use std::process::Stdio;
            use gglib::commands::llama::ensure_llama_initialized;
            use gglib::commands::llama_args::resolve_context_size;
            use gglib::commands::llama_invocation::LlamaCommandBuilder;
            use gglib::utils::paths::get_llama_server_path;

            ensure_llama_initialized().await?;
            let llama_server_path = get_llama_server_path()?;
            let model = core.models().get_by_id(id).await?;

            println!("Using model: {} (ID: {})", model.name, model.id.unwrap_or(0));
            println!("File: {}", model.file_path.display());
            println!("Server will be available on http://localhost:{}", port);

            let context_resolution = resolve_context_size(ctx_size, model.context_length)?;

            let mut builder = LlamaCommandBuilder::new(&llama_server_path, &model.file_path)
                .context_resolution(context_resolution)
                .mlock(mlock)
                .arg_with_value("--port", port.to_string());

            if jinja {
                builder = builder.flag("--jinja");
            }

            let mut cmd = builder.build();
            cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());

            println!("Starting server... (Press Ctrl+C to stop)");
            let status = cmd.status()?;
            if status.success() {
                println!("llama-server exited successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!("llama-server exited with code: {:?}", status.code()))
            }
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
            // Migrated to gglib-cli; inline stub for legacy binary
            use std::process::Stdio;
            use gglib::commands::llama::ensure_llama_initialized;
            use gglib::commands::llama_args::resolve_context_size;
            use gglib::commands::llama_invocation::LlamaCommandBuilder;
            use gglib::utils::paths::get_llama_cli_path;

            ensure_llama_initialized().await?;
            let llama_cli_path = get_llama_cli_path()?;

            let models = core.models().find_by_name(&identifier).await?;
            if models.is_empty() {
                return Err(anyhow::anyhow!("Model '{}' not found", identifier));
            }
            let model = &models[0];

            println!("Using model: {} (ID: {})", model.name, model.id.unwrap_or(0));
            println!("File: {}", model.file_path.display());

            let context_resolution = resolve_context_size(ctx_size, model.context_length)?;

            let mut cmd = LlamaCommandBuilder::new(&llama_cli_path, &model.file_path)
                .context_resolution(context_resolution)
                .mlock(mlock)
                .flag("--interactive-first")
                .build();

            if jinja {
                cmd.arg("--jinja");
            }
            if let Some(template) = chat_template {
                cmd.arg("--chat-template").arg(template);
            }
            if let Some(template_file) = chat_template_file {
                cmd.arg("--chat-template-file").arg(template_file);
            }
            if let Some(prompt) = system_prompt {
                cmd.arg("-sys").arg(prompt);
            }
            if multiline_input {
                cmd.arg("--multiline-input");
            }
            if simple_io {
                cmd.arg("--simple-io");
            }

            cmd.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());

            println!("Chat session ready. Press Ctrl+C to exit.");
            let status = cmd.status()?;
            if status.success() {
                println!("llama-cli exited successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!("llama-cli exited with code: {:?}", status.code()))
            }
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
        Commands::Config { command } => {
            // Migrated to gglib-cli; inline stub for legacy binary
            use cli::{ConfigCommand, ModelsDirCommand, SettingsCommand};
            use gglib::utils::paths::{
                DirectoryCreationStrategy, default_models_dir, ensure_directory,
                persist_models_dir, resolve_models_dir,
            };
            use gglib::utils::input;
            use gglib::services::settings::{Settings, SettingsUpdate};

            match command {
                ConfigCommand::ModelsDir { command } => match command {
                    ModelsDirCommand::Show => {
                        let resolved = resolve_models_dir(None)?;
                        println!(
                            "Current models directory: {} (source: {:?})",
                            resolved.path.display(),
                            resolved.source
                        );
                        Ok(())
                    }
                    ModelsDirCommand::Prompt => {
                        let default_path = default_models_dir()?.to_string_lossy().to_string();
                        let answer = input::prompt_string_with_default(
                            "Where should gglib store downloaded models?",
                            Some(&default_path),
                        )?;
                        let resolved = resolve_models_dir(Some(&answer))?;
                        ensure_directory(&resolved.path, DirectoryCreationStrategy::AutoCreate)?;
                        persist_models_dir(&resolved.path)?;
                        println!("✓ Models directory updated to {}", resolved.path.display());
                        Ok(())
                    }
                    ModelsDirCommand::Set { path, no_create } => {
                        let resolved = resolve_models_dir(Some(&path))?;
                        let strategy = if no_create {
                            DirectoryCreationStrategy::Disallow
                        } else {
                            DirectoryCreationStrategy::AutoCreate
                        };
                        ensure_directory(&resolved.path, strategy)?;
                        persist_models_dir(&resolved.path)?;
                        println!("✓ Models directory updated to {}", resolved.path.display());
                        Ok(())
                    }
                },
                ConfigCommand::Settings { command } => {
                    match command {
                        SettingsCommand::Show => {
                            let settings = core.settings().get().await?;
                            println!("Current application settings:");
                            println!("  default_download_path:   {:?}", settings.default_download_path);
                            println!("  default_context_size:    {:?}", settings.default_context_size);
                            println!("  proxy_port:              {:?}", settings.proxy_port);
                            println!("  server_port:             {:?}", settings.server_port);
                            println!("  max_download_queue_size: {:?}", settings.max_download_queue_size);
                            Ok(())
                        }
                        SettingsCommand::Set {
                            default_context_size,
                            proxy_port,
                            server_port,
                            max_download_queue_size,
                            default_download_path,
                        } => {
                            if default_download_path.is_none()
                                && default_context_size.is_none()
                                && proxy_port.is_none()
                                && server_port.is_none()
                                && max_download_queue_size.is_none()
                            {
                                println!("No settings provided. Use --help to see available options.");
                                return Ok(());
                            }
                            let update = SettingsUpdate {
                                default_download_path: default_download_path.map(Some),
                                default_context_size: default_context_size.map(Some),
                                proxy_port: proxy_port.map(Some),
                                server_port: server_port.map(Some),
                                max_download_queue_size: max_download_queue_size.map(Some),
                                show_memory_fit_indicators: None,
                            };
                            core.settings().update(update).await?;
                            println!("✓ Settings updated successfully");
                            Ok(())
                        }
                        SettingsCommand::Reset { force } => {
                            if !force {
                                if !input::prompt_confirmation("Reset all settings to defaults?")? {
                                    println!("Cancelled.");
                                    return Ok(());
                                }
                            }
                            let defaults = Settings::with_defaults();
                            core.settings().save(&defaults).await?;
                            println!("✓ All settings reset to defaults.");
                            Ok(())
                        }
                    }
                }
            }
        }
    }
}
