//! Web server command handler.
//!
//! Handles starting the Axum HTTP server with optional static file serving.
//! Discovers frontend build artifacts automatically from well-known paths,
//! or falls back to API-only mode when no frontend is present.

use std::path::PathBuf;

use anyhow::Result;

/// Execute the `web` command.
///
/// Builds the Axum `ServerConfig`, resolves the static-files directory
/// (explicit flag → auto-discovery → API-only), prints startup information,
/// and then blocks until the server shuts down.
///
/// # Arguments
///
/// * `port`       — TCP port to listen on for HTTP requests.
/// * `base_port`  — Starting port range for llama-server subprocess allocation.
/// * `api_only`   — When `true`, skip static-file serving regardless of flags.
/// * `static_dir` — Explicit path to a built frontend; takes priority over
///   auto-discovery when `api_only` is `false`.
pub async fn execute(
    port: u16,
    base_port: u16,
    api_only: bool,
    static_dir: Option<PathBuf>,
) -> Result<()> {
    use gglib_axum::{CorsConfig, ServerConfig, start_server};
    use gglib_core::paths::llama_server_path;

    // Warn if the VITE env var is set but unparseable so the user knows
    // we are ignoring it rather than silently falling back to the default.
    if let Ok(env_port) = std::env::var("VITE_GGLIB_WEB_PORT")
        && env_port.parse::<u16>().is_err()
    {
        eprintln!(
            "Warning: VITE_GGLIB_WEB_PORT='{}' is not a valid port number. Using default: {}",
            env_port, port
        );
    }

    let mut config = ServerConfig {
        port,
        base_port,
        llama_server_path: llama_server_path()?,
        max_concurrent: 4,
        max_concurrent_agent_loops: 4,
        static_dir: None,
        cors: CorsConfig::AllowAll,
    };

    // Resolve static directory: api-only flag > explicit flag > auto-discover > none
    if !api_only {
        if let Some(dir) = static_dir {
            config.static_dir = Some(dir);
        } else {
            // Prefer built assets; accept the first directory that contains index.html.
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
    Ok(())
}
