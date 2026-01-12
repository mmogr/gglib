//! CLI entry point - the composition root.
//!
//! This is the ONLY place where infrastructure is wired together via bootstrap.
//! Command dispatch routes to handlers which delegate to AppCore.
//!
//! All CLI code uses CliContext for dependency access - no direct
//! database or pool access outside of bootstrap.

use clap::Parser;

use gglib_cli::{AssistantUiCommand, Cli, CliConfig, Commands, LlamaCommand, bootstrap, handlers};
use gglib_runtime::DefaultSystemProbe;
use gglib_runtime::assistant_ui::{
    handle_install as handle_assistant_install, handle_status as handle_assistant_status,
    handle_update as handle_assistant_update,
};
use gglib_runtime::llama::{
    handle_check_updates, handle_install, handle_rebuild, handle_status, handle_uninstall,
    handle_update,
};

#[cfg(target_os = "linux")]
fn find_linux_gui_artifact(repo_root: &std::path::Path) -> std::path::PathBuf {
    let appimage_dir = repo_root.join("src-tauri/target/release/bundle/appimage");
    if let Ok(read_dir) = std::fs::read_dir(&appimage_dir) {
        let mut candidates: Vec<std::path::PathBuf> = read_dir
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|name| name.ends_with(".AppImage"))
            })
            .collect();

        candidates.sort();
        if let Some(path) = candidates.into_iter().next() {
            return path;
        }
    }

    repo_root.join("src-tauri/target/release/gglib-app")
}

fn launch_gui_command(repo_root: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let app_bundle = repo_root.join("src-tauri/target/release/bundle/macos/GGLib GUI.app");
        if app_bundle.exists() {
            println!("Launching GGLib GUI...");
            let status = std::process::Command::new("open").arg(&app_bundle).status();
            match status {
                Ok(s) if s.success() => Ok(()),
                Ok(s) => anyhow::bail!("Failed to launch GUI (exit code: {:?})", s.code()),
                Err(e) => Err(e.into()),
            }
        } else {
            println!("Desktop GUI not found at: {}", app_bundle.display());
            println!();
            println!("To build the GUI, run: make build-tauri");
            println!("Or: npm run tauri:build");
            Ok(())
        }
    }

    #[cfg(target_os = "linux")]
    {
        let artifact = find_linux_gui_artifact(repo_root);
        if artifact.exists() {
            println!("Launching GGLib GUI...");
            let spawned = std::process::Command::new(&artifact).spawn();
            match spawned {
                Ok(_child) => Ok(()),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        anyhow::bail!(
                            "Failed to launch GUI: {} (is it executable? try: chmod +x \"{}\")",
                            e,
                            artifact.display()
                        );
                    }
                    Err(e.into())
                }
            }
        } else {
            let appimage_dir = repo_root.join("src-tauri/target/release/bundle/appimage");
            println!(
                "Desktop GUI not found at: {} (or any *.AppImage in {})",
                repo_root
                    .join("src-tauri/target/release/gglib-app")
                    .display(),
                appimage_dir.display()
            );
            println!();
            println!("To build the GUI, run: make build-tauri");
            println!("Or: npm run tauri:build");
            Ok(())
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = repo_root;
        anyhow::bail!("gglib gui is not supported on this OS yet")
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn make_temp_dir(prefix: &str) -> std::path::PathBuf {
        let mut base = std::env::temp_dir();
        base.push(format!(
            "{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn linux_gui_artifact_prefers_any_appimage() {
        let root = make_temp_dir("gglib_cli_gui");
        let appimage_dir = root.join("src-tauri/target/release/bundle/appimage");
        std::fs::create_dir_all(&appimage_dir).unwrap();

        let appimage = appimage_dir.join("GGLib GUI_0.2.4_amd64.AppImage");
        std::fs::write(&appimage, b"stub").unwrap();

        let chosen = find_linux_gui_artifact(&root);
        assert_eq!(chosen, appimage);
    }

    #[test]
    fn linux_gui_artifact_falls_back_to_binary_when_no_appimage() {
        let root = make_temp_dir("gglib_cli_gui");
        let chosen = find_linux_gui_artifact(&root);
        assert_eq!(chosen, root.join("src-tauri/target/release/gglib-app"));
    }
}

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

    // Dispatch to appropriate handler
    let Some(command) = cli.command else {
        // No command provided - show help
        use clap::CommandFactory;
        gglib_cli::Cli::command().print_help()?;
        return Ok(());
    };

    match command {
        Commands::CheckDeps => {
            let probe = DefaultSystemProbe::new();
            handlers::check_deps::execute(&probe).await?;
        }
        Commands::Paths => {
            handlers::paths::execute()?;
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
        Commands::Question {
            question,
            model,
            file,
            ctx_size,
            mlock,
            verbose,
            quiet,
        } => {
            // Ask a question with optional piped/file context
            handlers::question::execute(&ctx, question, model, file, ctx_size, mlock, verbose, quiet).await?;
        }
        Commands::Download {
            model_id,
            quantization,
            list_quants,
            skip_db: _skip_db, // Not used yet - future: skip registration
            token,
            force,
        } => {
            // NEW: Uses CliContext and gglib-download
            let args = handlers::download::DownloadArgs {
                model_id: &model_id,
                quantization: quantization.as_deref(),
                list_quants,
                force,
                token: token.as_deref(),
            };
            handlers::download::download(&ctx, args).await?;
        }
        Commands::CheckUpdates { model_id, all } => {
            // NEW: Uses CliContext
            handlers::download::check_updates(&ctx, model_id, all).await?;
        }
        Commands::UpdateModel {
            model_id,
            force: _force,
        } => {
            // NEW: Uses CliContext
            handlers::download::update_model(&ctx, model_id).await?;
        }
        Commands::Search {
            query,
            limit,
            sort,
            gguf_only,
        } => {
            // NEW: Uses gglib-download (no AppCore needed)
            handlers::download::search(query, limit, sort, gguf_only).await?;
        }
        Commands::Browse {
            category,
            limit,
            size,
        } => {
            // NEW: Uses gglib-download (no AppCore needed)
            handlers::download::browse(category, limit, size).await?;
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
                handle_install(cuda, metal, cpu_only, force, build).await?;
            }
            LlamaCommand::CheckUpdates => {
                handle_check_updates().await?;
            }
            LlamaCommand::Update => {
                handle_update().await?;
            }
            LlamaCommand::Status => {
                handle_status().await?;
            }
            LlamaCommand::Rebuild {
                cuda,
                metal,
                cpu_only,
            } => {
                handle_rebuild(cuda, metal, cpu_only).await?;
            }
            LlamaCommand::Uninstall { force } => {
                handle_uninstall(force).await?;
            }
        },
        Commands::Gui { dev } => {
            if dev {
                println!("Development mode requires running 'cargo tauri dev' directly");
            } else {
                let repo_root = std::path::PathBuf::from(env!("GGLIB_REPO_ROOT"));
                if let Err(e) = launch_gui_command(&repo_root) {
                    eprintln!("{}", e);
                }
            }
        }
        Commands::Web {
            port,
            base_port,
            api_only,
            static_dir,
        } => {
            use gglib_axum::{ServerConfig, start_server};
            use gglib_core::paths::llama_server_path;

            // Build server config
            let mut config = ServerConfig {
                port,
                base_port,
                llama_server_path: llama_server_path()?,
                max_concurrent: 4,
                static_dir: None,
                cors: gglib_axum::CorsConfig::AllowAll,
            };

            // Resolve static directory: api-only flag > explicit flag > default location > API-only
            if !api_only {
                if let Some(dir) = static_dir {
                    config.static_dir = Some(dir);
                } else {
                    // Try default locations (order matters - prefer built assets first)
                    let candidates = ["./web_ui/dist", "./dist", "./web_ui/assets", "./web_ui"];
                    for candidate in &candidates {
                        let path = std::path::Path::new(candidate);
                        if path.join("index.html").exists() {
                            config.static_dir = Some(path.to_path_buf());
                            break;
                        }
                    }
                }
            }

            if let Some(ref dir) = config.static_dir {
                println!();
                println!("  🚀 gglib web server starting...");
                println!();
                println!("  📂 Serving UI from: {}", dir.display());
                println!("  🌐 Local:   http://localhost:{}", port);
                println!("  🌐 Network: http://0.0.0.0:{}", port);
                println!();
                println!("  Press Ctrl+C to stop");
                println!();
            } else {
                println!();
                println!("  🚀 gglib web server starting (API only)...");
                println!();
                println!("  🌐 API:     http://localhost:{}", port);
                println!();
                println!("  💡 Tip: Use --static-dir to serve a frontend build");
                println!();
            }

            start_server(config).await?;
        }
        Commands::Proxy {
            host,
            port,
            llama_port,
            default_context,
        } => {
            // Uses standalone proxy with model repo and llama path from context
            gglib_runtime::proxy::start_proxy_standalone(
                host,
                port,
                llama_port,
                ctx.llama_server_path.clone(),
                ctx.model_repo.clone(),
                default_context,
            )
            .await?;
        }
        Commands::AssistantUi { command } => match command {
            AssistantUiCommand::Install => {
                handle_assistant_install().map_err(|e| anyhow::anyhow!(e))?;
            }
            AssistantUiCommand::Update => {
                handle_assistant_update().map_err(|e| anyhow::anyhow!(e))?;
            }
            AssistantUiCommand::Status => {
                handle_assistant_status().map_err(|e| anyhow::anyhow!(e))?;
            }
        },
    }

    Ok(())
}
