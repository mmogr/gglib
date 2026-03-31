//! MCP server management command handlers.
//!
//! All handlers delegate to `McpService` via `ctx.mcp` — no business logic
//! lives here, only CLI input parsing and output formatting.

use anyhow::{Result, anyhow, bail};
use gglib_core::domain::mcp::{McpServerStatus, McpServerType, NewMcpServer};

use crate::bootstrap::CliContext;
use crate::mcp_commands::McpCommand;
use crate::presentation::{print_separator, truncate_string};
use crate::utils::input;

/// Dispatch an MCP subcommand to its handler.
pub async fn dispatch(ctx: &CliContext, cmd: McpCommand) -> Result<()> {
    match cmd {
        McpCommand::List => list(ctx).await,
        McpCommand::Add {
            name,
            r#type,
            command,
            args,
            url,
            working_dir,
            path_extra,
            env,
            auto_start,
            disabled,
        } => {
            add(
                ctx,
                &name,
                &r#type,
                command.as_deref(),
                args,
                url.as_deref(),
                working_dir.as_deref(),
                path_extra,
                &env,
                auto_start,
                disabled,
            )
            .await
        }
        McpCommand::Remove { server, force } => remove(ctx, &server, force).await,
        McpCommand::Start { server } => start(ctx, &server).await,
        McpCommand::Stop { server } => stop(ctx, &server).await,
        McpCommand::Enable { server } => set_enabled(ctx, &server, true).await,
        McpCommand::Disable { server } => set_enabled(ctx, &server, false).await,
        McpCommand::Tools { server } => tools(ctx, &server).await,
        McpCommand::Test { server } => test(ctx, &server).await,
    }
}

// ─── Handlers ───────────────────────────────────────────────────────────────

async fn list(ctx: &CliContext) -> Result<()> {
    let servers = ctx.mcp.list_servers_with_status().await?;

    if servers.is_empty() {
        println!("No MCP servers configured.");
        println!("Use 'gglib mcp add' to register a server.");
        return Ok(());
    }

    println!("Found {} MCP server(s):\n", servers.len());
    println!(
        "{:<4} {:<25} {:<6} {:<9} {:<11} {:<10} {:<6}",
        "ID", "Name", "Type", "Enabled", "Auto-start", "Status", "Tools"
    );
    print_separator(75);

    for info in servers {
        let s = &info.server;
        let type_str = match s.server_type {
            McpServerType::Stdio => "stdio",
            McpServerType::Sse => "sse",
        };
        let status_str = match &info.status {
            McpServerStatus::Stopped => "stopped".to_string(),
            McpServerStatus::Starting => "starting".to_string(),
            McpServerStatus::Running => "running".to_string(),
            McpServerStatus::Error(e) => format!("error: {}", truncate_string(e, 20)),
        };
        let enabled_str = if s.enabled { "yes" } else { "no" };
        let auto_str = if s.auto_start { "yes" } else { "no" };

        println!(
            "{:<4} {:<25} {:<6} {:<9} {:<11} {:<10} {:<6}",
            s.id,
            truncate_string(&s.name, 24),
            type_str,
            enabled_str,
            auto_str,
            truncate_string(&status_str, 9),
            info.tools.len()
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn add(
    ctx: &CliContext,
    name: &str,
    server_type: &str,
    command: Option<&str>,
    args: Vec<String>,
    url: Option<&str>,
    working_dir: Option<&str>,
    path_extra: Option<String>,
    env_pairs: &[String],
    auto_start: bool,
    disabled: bool,
) -> Result<()> {
    let mut new_server = match server_type {
        "stdio" => {
            let cmd = command.ok_or_else(|| anyhow!("--command is required for stdio servers"))?;
            NewMcpServer::new_stdio(name, cmd, args, path_extra)
        }
        "sse" => {
            let server_url = url.ok_or_else(|| anyhow!("--url is required for sse servers"))?;
            NewMcpServer::new_sse(name, server_url)
        }
        _ => bail!("--type must be 'stdio' or 'sse'"),
    };

    // Apply optional settings
    if let Some(dir) = working_dir {
        new_server = new_server.with_working_dir(dir);
    }
    new_server = new_server.with_auto_start(auto_start);
    if disabled {
        new_server = new_server.with_enabled(false);
    }
    for pair in env_pairs {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| anyhow!("--env must be KEY=VALUE, got: {pair}"))?;
        new_server = new_server.with_env(key, value);
    }

    let server = ctx.mcp.add_server(new_server).await?;
    println!("✅ Added MCP server '{}' (id: {})", server.name, server.id);

    Ok(())
}

async fn remove(ctx: &CliContext, identifier: &str, force: bool) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;

    if !force {
        println!(
            "Server: {} (id: {}, type: {:?})",
            server.name, server.id, server.server_type
        );
        if !input::prompt_confirmation("Remove this MCP server?")? {
            println!("Cancelled.");
            return Ok(());
        }
    }

    ctx.mcp.remove_server(server.id).await?;
    println!("✅ Removed MCP server '{}'", server.name);

    Ok(())
}

async fn start(ctx: &CliContext, identifier: &str) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;
    let tools = ctx.mcp.start_server(server.id).await?;
    println!(
        "✅ Started '{}' — {} tool(s) available",
        server.name,
        tools.len()
    );
    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("(no description)");
        println!("   • {} — {}", tool.name, truncate_string(desc, 60));
    }

    Ok(())
}

async fn stop(ctx: &CliContext, identifier: &str) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;
    ctx.mcp.stop_server(server.id).await?;
    println!("✅ Stopped '{}'", server.name);

    Ok(())
}

async fn set_enabled(ctx: &CliContext, identifier: &str, enabled: bool) -> Result<()> {
    let mut server = resolve_server(ctx, identifier).await?;
    server.enabled = enabled;
    ctx.mcp.update_server(server.clone()).await?;
    let action = if enabled { "Enabled" } else { "Disabled" };
    println!("✅ {} '{}'", action, server.name);

    Ok(())
}

async fn tools(ctx: &CliContext, identifier: &str) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;
    let tools = ctx.mcp.list_server_tools(server.id).await?;

    if tools.is_empty() {
        println!(
            "No tools available for '{}' (is the server running?).",
            server.name
        );
        return Ok(());
    }

    println!("{} tool(s) from '{}':\n", tools.len(), server.name);
    println!("{:<30} Description", "Name");
    print_separator(75);

    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("(no description)");
        println!(
            "{:<30} {}",
            truncate_string(&tool.name, 29),
            truncate_string(desc, 44)
        );
    }

    Ok(())
}

async fn test(ctx: &CliContext, identifier: &str) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;
    println!("Testing connection to '{}'...", server.name);

    // Build a NewMcpServer from the existing server for test_connection
    let new_server = NewMcpServer {
        name: server.name.clone(),
        server_type: server.server_type,
        config: server.config.clone(),
        enabled: server.enabled,
        auto_start: server.auto_start,
        env: server.env.clone(),
    };

    let tools = ctx.mcp.test_connection(new_server).await?;
    println!(
        "✅ Connection successful — {} tool(s) discovered",
        tools.len()
    );
    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("(no description)");
        println!("   • {} — {}", tool.name, truncate_string(desc, 60));
    }

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Resolve a server identifier (numeric ID or name) to an `McpServer`.
async fn resolve_server(ctx: &CliContext, identifier: &str) -> Result<gglib_core::McpServer> {
    // Try as numeric ID first
    if let Ok(id) = identifier.parse::<i64>() {
        return ctx
            .mcp
            .get_server(id)
            .await
            .map_err(|e| anyhow!("Server with id {id} not found: {e}"));
    }

    // Fall back to name lookup
    ctx.mcp
        .get_server_by_name(identifier)
        .await
        .map_err(|e| anyhow!("Server '{}' not found: {e}", identifier))
}
