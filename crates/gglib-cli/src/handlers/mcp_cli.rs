//! MCP server management command handlers.
//!
//! The configuration handlers delegate to `McpService` via `ctx.mcp`. What
//! runs is the daemon's: `list` asks it for each server's status and tools
//! when it is running, and `tools` asks it for one server's, since the
//! daemon's copy of a server is the one its chats use (#1689). No business
//! logic lives here, only CLI input parsing and output formatting.

use anyhow::{Result, anyhow};
use gglib_app_services::types::{McpServerInfo, McpServerStatusDto, McpToolInfo};
use gglib_core::McpServiceError;
use gglib_core::daemon_port;
use gglib_core::domain::mcp::{McpLifecycle, McpServerType, NewMcpServer};
use gglib_mcp::McpService;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, Absent, DaemonHandle};
use crate::mcp_commands::McpCommand;
use crate::presentation::{print_separator, truncate_string};
use crate::utils::input;

/// Dispatch an MCP subcommand to its handler.
pub(crate) async fn dispatch(ctx: &CliContext, cmd: McpCommand) -> Result<()> {
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
            lifecycle,
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
                lifecycle,
                disabled,
            )
            .await
        }
        McpCommand::Remove { server, yes } => remove(ctx, &server, yes).await,
        McpCommand::Enable { server } => set_enabled(ctx, &server, true).await,
        McpCommand::Disable { server } => set_enabled(ctx, &server, false).await,
        McpCommand::AllowRemote { servers } => set_allow_remote(ctx, &servers, true).await,
        McpCommand::DenyRemote { servers } => set_allow_remote(ctx, &servers, false).await,
        McpCommand::Tools { server } => tools(ctx, &server).await,
        McpCommand::Test { server } => test(ctx, &server).await,
    }
}

// ─── Handlers ───────────────────────────────────────────────────────────────

async fn list(ctx: &CliContext) -> Result<()> {
    let servers = ctx.mcp.list_servers().await?;

    if servers.is_empty() {
        println!("No MCP servers configured.");
        println!("Use 'gglib mcp add' to register a server.");
        return Ok(());
    }

    println!("Found {} MCP server(s):\n", servers.len());
    println!(
        "{:<4} {:<25} {:<6} {:<9} {:<7} {:<11} {:<10} {:<6}",
        "ID", "Name", "Type", "Enabled", "Remote", "Lifecycle", "Status", "Tools"
    );
    print_separator(83);

    let daemon = daemon_servers(ctx).await;
    for s in &servers {
        let type_str = s.server_type.to_string();
        let yes_no = |on: bool| if on { "yes" } else { "no" };
        let lifecycle_str = s.lifecycle.to_string();

        println!(
            "{:<4} {:<25} {:<6} {:<9} {:<7} {:<11} {}",
            s.id,
            truncate_string(&s.name, 24),
            type_str,
            yes_no(s.enabled),
            yes_no(s.allow_remote),
            lifecycle_str,
            status_and_tools(s, daemon.as_deref().ok())
        );
    }
    if let Err(why) = daemon {
        println!("\nStatus and Tools are unknown: {why}.");
    }

    Ok(())
}

/// The daemon's servers, when a daemon is running, or why there are none to
/// read. Never a launch: a list is no reason to start one.
async fn daemon_servers(ctx: &CliContext) -> Result<Vec<McpServerInfo>, String> {
    match daemon_client::running(ctx).await {
        Ok(daemon) => daemon
            .mcp_servers()
            .await
            .map_err(|e| format!("the daemon did not answer ({e})")),
        Err(Absent::NotRunning) => Err("the daemon is not running".to_owned()),
        Err(Absent::ForeignServer) => Err(format!(
            "port {} is held by another program (not a gglib daemon)",
            daemon_port()
        )),
    }
}

/// A row's Status and Tools cells: the daemon's copy of `server`, from
/// `daemon`'s list, and a dash in each when there is no daemon to read.
///
/// A server gglib cannot run has no status and no tools to count, daemon or
/// not. It says so across the two cells, which the words fill exactly.
fn status_and_tools(server: &gglib_core::McpServer, daemon: Option<&[McpServerInfo]>) -> String {
    if server.server_type == McpServerType::Sse {
        return "not supported yet".to_string();
    }
    let Some(info) = daemon.and_then(|infos| infos.iter().find(|i| i.server.id == server.id))
    else {
        return format!("{:<10} {:<6}", "—", "—");
    };
    let status = match &info.status {
        McpServerStatusDto::Unsupported => return "not supported yet".to_string(),
        McpServerStatusDto::Stopped => "stopped".to_string(),
        McpServerStatusDto::Starting => "starting".to_string(),
        McpServerStatusDto::Running => "running".to_string(),
        McpServerStatusDto::Error(e) => format!("error: {}", truncate_string(e, 20)),
    };
    format!(
        "{:<10} {:<6}",
        truncate_string(&status, 9),
        info.tools.len()
    )
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
    lifecycle: String,
    disabled: bool,
) -> Result<()> {
    let server_type = server_type
        .parse::<McpServerType>()
        .map_err(|e| anyhow!("Invalid --type value: {e}"))?;
    let mut new_server = match server_type {
        McpServerType::Stdio => {
            let cmd = command.ok_or_else(|| anyhow!("--command is required for stdio servers"))?;
            NewMcpServer::new_stdio(name, cmd, args, path_extra)
        }
        // The service refuses an SSE server whatever its URL is, so a missing
        // one is not asked for first.
        McpServerType::Sse => NewMcpServer::new_sse(name, url.unwrap_or_default()),
    };

    // Apply optional settings
    if let Some(dir) = working_dir {
        new_server = new_server.with_working_dir(dir);
    }
    let parsed_lifecycle = lifecycle
        .parse::<McpLifecycle>()
        .map_err(|e| anyhow!("Invalid --lifecycle value: {e}"))?;
    new_server = new_server.with_lifecycle(parsed_lifecycle);
    if disabled {
        new_server = new_server.with_enabled(false);
    }
    for pair in env_pairs {
        let (key, value) = pair
            .split_once('=')
            // Not echoed: what was typed may be the value itself.
            .ok_or_else(|| anyhow!("each --env must be KEY=VALUE"))?;
        new_server = new_server.with_env(key, value);
    }

    let server = ctx.mcp.add_server(new_server).await?;
    println!("✓ Added MCP server '{}' (id: {})", server.name, server.id);

    Ok(())
}

async fn remove(ctx: &CliContext, identifier: &str, yes: bool) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;

    if !yes {
        println!(
            "Server: {} (id: {}, type: {})",
            server.name, server.id, server.server_type
        );
        if !input::prompt_confirmation("Remove this MCP server?")? {
            println!("Cancelled.");
            return Ok(());
        }
    }

    ctx.mcp.remove_server(server.id).await?;
    println!("✓ Removed MCP server '{}'", server.name);

    Ok(())
}

async fn set_enabled(ctx: &CliContext, identifier: &str, enabled: bool) -> Result<()> {
    let mut server = resolve_server(ctx, identifier).await?;
    server.enabled = enabled;
    ctx.mcp.update_server(server.clone()).await?;
    let action = if enabled { "Enabled" } else { "Disabled" };
    println!("✓ {} '{}'", action, server.name);

    Ok(())
}

/// Open each of `identifiers` to paired devices, or close it: the flag's own
/// write, which never stops a server. Every one is found, and checked when
/// opening, before any is changed, so a typo or an SSE server in the last
/// changes none.
async fn set_allow_remote(ctx: &CliContext, identifiers: &[String], allow: bool) -> Result<()> {
    let mut found = Vec::with_capacity(identifiers.len());
    for identifier in identifiers {
        let server = the_one_server(ctx, identifier).await?;
        if allow {
            McpService::refuse_opening(&server)?;
        }
        found.push(server);
    }
    for server in found {
        let server = ctx.mcp.set_allow_remote(server.id, allow).await?;
        if !allow {
            println!("✓ '{}' is closed to paired devices", server.name);
            continue;
        }
        println!("✓ '{}' is open to paired devices", server.name);
        if !server.enabled {
            println!(
                "  It is disabled, so it offers them no tools until `gglib mcp enable {}`",
                server.id
            );
        }
    }
    Ok(())
}

/// The tools the daemon's copy of a server offers: the ones its chats get.
/// A disabled server is not started, as no chat gets its tools.
async fn tools(ctx: &CliContext, identifier: &str) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;
    if server.server_type == McpServerType::Sse {
        return Err(McpServiceError::SseNotSupported.into());
    }
    if !server.enabled {
        println!(
            "'{}' is disabled, so chats don't get its tools; `gglib mcp enable {}` includes them.",
            server.name, server.id
        );
        return Ok(());
    }

    let daemon = daemon_client::ensure_daemon(ctx).await?;
    let Some(tools) = tools_in_the_daemon(&daemon, server.id).await? else {
        println!(
            "'{}' is starting in the daemon; ask again in a moment.",
            server.name
        );
        return Ok(());
    };

    if tools.is_empty() {
        println!("'{}' offers no tools.", server.name);
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

/// The tools of the daemon's copy of server `id`, started there first when
/// it is not running, as a chat's first call would start it. `None` while
/// it is starting.
async fn tools_in_the_daemon(daemon: &DaemonHandle, id: i64) -> Result<Option<Vec<McpToolInfo>>> {
    let info = daemon
        .mcp_servers()
        .await?
        .into_iter()
        .find(|info| info.server.id == id)
        .ok_or_else(|| {
            anyhow!(
                "the daemon on port {} has no MCP server {id}: it keeps its data \
                 somewhere other than this command",
                daemon_port()
            )
        })?;
    match info.status {
        McpServerStatusDto::Running => Ok(Some(info.tools)),
        McpServerStatusDto::Starting => Ok(None),
        McpServerStatusDto::Unsupported => Err(McpServiceError::SseNotSupported.into()),
        McpServerStatusDto::Stopped | McpServerStatusDto::Error(_) => {
            eprintln!("  starting '{}' in the daemon\u{2026}", info.server.name);
            match daemon.mcp_start(id).await {
                Ok(started) => Ok(Some(started.tools)),
                // A chat's lazy start can get there between the read above
                // and this start, which then answers "already running".
                Err(e) => running_tools(daemon, id).await.map(Some).ok_or(e),
            }
        }
    }
}

/// The tools of the daemon's copy of server `id`, when it is running.
async fn running_tools(daemon: &DaemonHandle, id: i64) -> Option<Vec<McpToolInfo>> {
    let servers = daemon.mcp_servers().await.ok()?;
    let info = servers.into_iter().find(|info| info.server.id == id)?;
    matches!(info.status, McpServerStatusDto::Running).then_some(info.tools)
}

async fn test(ctx: &CliContext, identifier: &str) -> Result<()> {
    let server = resolve_server(ctx, identifier).await?;
    println!("Testing connection to '{}'...", server.name);

    let tools = ctx.mcp.test_server(server.id).await?;
    println!(
        "✓ Connection successful — {} tool(s) discovered",
        tools.len()
    );
    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("(no description)");
        println!("   • {} — {}", tool.name, truncate_string(desc, 60));
    }

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// The one server `identifier` names: by its id, or by a name no other
/// server has.
///
/// An older database can hold two servers of one name, and a lookup by name
/// would pick either; a grant to devices must not land on the one the owner
/// did not mean, so a shared name is refused and the ids are pointed to.
async fn the_one_server(ctx: &CliContext, identifier: &str) -> Result<gglib_core::McpServer> {
    if identifier.parse::<i64>().is_ok() {
        return resolve_server(ctx, identifier).await;
    }
    let mut named: Vec<_> = ctx
        .mcp
        .list_servers()
        .await?
        .into_iter()
        .filter(|server| server.name == identifier)
        .collect();
    match named.len() {
        0 => Err(anyhow!("Server '{identifier}' not found")),
        1 => Ok(named.remove(0)),
        n => Err(anyhow!(
            "{n} servers are named '{identifier}'; give its id (`gglib mcp list`)"
        )),
    }
}

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
        .map_err(|e| anyhow!("Server '{identifier}' not found: {e}"))
}
