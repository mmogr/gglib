/**
 * MCP Servers settings panel.
 *
 * Displays a list of configured MCP servers with controls to
 * start/stop, add, edit, and remove servers.
 */

import { FC, useState, useCallback } from "react";
import { AlertTriangle } from "lucide-react";
import { useMcpServers } from "../hooks/useMcpServers";
import {
  isServerRunning,
  hasServerError,
  getServerErrorMessage,
  resolveMcpServerPath,
} from "../services/clients/mcp";
import type { McpServerInfo } from "../services/clients/mcp";
import { Icon } from "./ui/Icon";
import { Button } from "./ui/Button";

interface McpServersPanelProps {
  onAddServer?: () => void;
  onEditServer?: (server: McpServerInfo) => void;
}

export const McpServersPanel: FC<McpServersPanelProps> = ({
  onAddServer,
  onEditServer,
}) => {
  const {
    servers,
    loading,
    error,
    refresh,
    removeServer,
    startServer,
    stopServer,
  } = useMcpServers();

  const [actionError, setActionError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<number | null>(null);

  const handleStart = useCallback(
    async (info: McpServerInfo) => {
      const id = info.server.id;

      setActionError(null);
      setActionLoading(id);
      try {
        await startServer(id);
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "Failed to start server");
      } finally {
        setActionLoading(null);
      }
    },
    [startServer]
  );

  const handleStop = useCallback(
    async (info: McpServerInfo) => {
      const id = info.server.id;

      setActionError(null);
      setActionLoading(id);
      try {
        await stopServer(id);
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "Failed to stop server");
      } finally {
        setActionLoading(null);
      }
    },
    [stopServer]
  );

  const handleRemove = useCallback(
    async (info: McpServerInfo) => {
      const id = info.server.id;

      const confirmed = window.confirm(
        `Remove MCP server "${info.server.name}"? This cannot be undone.`
      );
      if (!confirmed) return;

      setActionError(null);
      setActionLoading(id);
      try {
        await removeServer(id);
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "Failed to remove server");
      } finally {
        setActionLoading(null);
      }
    },
    [removeServer]
  );

  const handleAutoFix = useCallback(
    async (info: McpServerInfo) => {
      const id = info.server.id;

      setActionError(null);
      setActionLoading(id);
      try {
        const result = await resolveMcpServerPath(id);
        
        if (result.success) {
          // Success! Show toast and refresh
          alert(`✓ Resolved: ${result.resolved_path}`);
          // The list will auto-refresh via useMcpServers hook
        } else {
          // Show detailed error with attempts
          const attemptsText = result.attempts
            .slice(0, 8)
            .map(a => `  ✗ ${a.candidate}: ${a.outcome}`)
            .join('\n');
          
          const allNotFound = result.attempts.every(a => 
            a.outcome.toLowerCase().includes('not found')
          );
          
          const installHint = allNotFound 
            ? '\n\n• Install Node.js if not installed (includes npm/npx)'
            : '';
          
          const suggestedFix = result.suggested_fix 
            ? `\n\nSuggested fix: ${result.suggested_fix}`
            : '';
          
          alert(
            `Could not resolve executable:\n\n${attemptsText}${suggestedFix}${installHint}`
          );
        }
      } catch (e) {
        setActionError(e instanceof Error ? e.message : "Auto-fix failed");
      } finally {
        setActionLoading(null);
      }
    },
    [resolveMcpServerPath]
  );

  const getStatusBadge = (info: McpServerInfo) => {
    if (isServerRunning(info)) {
      return <span className="inline-flex items-center px-sm py-0.5 text-xs font-semibold rounded-full bg-[rgba(16,185,129,0.15)] text-[#10b981]">Running</span>;
    }
    if (hasServerError(info)) {
      return <span className="inline-flex items-center px-sm py-0.5 text-xs font-semibold rounded-full bg-[rgba(239,68,68,0.15)] text-[#ef4444]">Error</span>;
    }
    if (info.status === "starting") {
      return <span className="inline-flex items-center px-sm py-0.5 text-xs font-semibold rounded-full bg-[rgba(245,158,11,0.15)] text-[#f59e0b]">Starting...</span>;
    }
    return <span className="inline-flex items-center px-sm py-0.5 text-xs font-semibold rounded-full bg-background-tertiary text-text-secondary">Stopped</span>;
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center gap-sm p-xl text-text-secondary">
        <div className="w-5 h-5 border-2 border-border border-t-primary rounded-full animate-spin-360" />
        <span>Loading MCP servers...</span>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-md">
      <div className="flex justify-between items-center gap-md">
        <div className="flex items-center gap-sm">
          <h3 className="m-0 text-base font-semibold text-text">MCP Servers</h3>
          <span className="text-sm text-text-secondary">
            {servers.length} server{servers.length !== 1 ? "s" : ""}
          </span>
        </div>
        <div className="flex gap-sm">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={refresh}
          >
            Refresh
          </Button>
          {onAddServer && (
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={onAddServer}
            >
              Add Server
            </Button>
          )}
        </div>
      </div>

      {error && (
        <div className="p-md bg-[rgba(239,68,68,0.15)] text-[#ef4444] rounded-base text-sm" role="alert">
          {error}
        </div>
      )}

      {actionError && (
        <div className="p-md bg-[rgba(239,68,68,0.15)] text-[#ef4444] rounded-base text-sm" role="alert">
          {actionError}
        </div>
      )}

      {servers.length === 0 ? (
        <div className="text-center p-xl text-text-secondary">
          <p className="m-0 mb-sm">No MCP servers configured.</p>
          <p className="text-sm mb-md">
            MCP servers provide tools that can be called by the LLM during chat.
            Add servers for web search, file access, and more.
          </p>
          {onAddServer && (
            <Button
              type="button"
              variant="primary"
              onClick={onAddServer}
            >
              Add Your First Server
            </Button>
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-md">
          {servers.map((info) => {
            const id = info.server.id;
            const isLoading = actionLoading === id;
            const isRunning = isServerRunning(info);

            return (
              <div key={id} className="flex justify-between items-start gap-md p-md bg-background-secondary border border-border rounded-base">
                <div className="flex-1 min-w-0 flex flex-col gap-xs">
                  <div className="flex items-center gap-sm">
                    <span className="font-semibold text-text">{info.server.name}</span>
                    {getStatusBadge(info)}
                  </div>
                  <div className="flex items-center gap-sm text-sm text-text-secondary">
                    <span className="px-sm py-0.5 bg-background-tertiary rounded-sm text-xs font-semibold text-text-secondary">
                      {info.server.server_type === "stdio" ? "Stdio" : "SSE"}
                    </span>
                    {!info.server.is_valid && (
                      <span className="inline-flex items-center gap-1 px-sm py-0.5 bg-[#fef3c7] text-[#d97706] text-xs font-medium rounded-sm cursor-help" title={info.server.last_error || "Invalid configuration"}>
                        <Icon icon={AlertTriangle} size={14} />
                        <span className="ml-1.5">Needs relink</span>
                      </span>
                    )}
                    {info.server.server_type === "stdio" && info.server.config.command && (
                      <code className="font-mono text-xs text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap">
                        {info.server.config.command}
                        {info.server.config.args?.length ? ` ${info.server.config.args.join(" ")}` : ""}
                      </code>
                    )}
                    {info.server.config.url && (
                      <code className="font-mono text-xs text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap">{info.server.config.url}</code>
                    )}
                    {!info.server.is_valid && info.server.last_error && (
                      <div className="text-xs text-[#dc2626] mt-xs p-xs bg-[#fef2f2] rounded-sm border-l-2 border-[#dc2626]">
                        {info.server.last_error}
                      </div>
                    )}
                  </div>
                  {isRunning && info.tools.length > 0 && (
                    <div className="flex flex-wrap items-center gap-xs mt-xs">
                      <span className="text-xs text-text-secondary">Tools:</span>
                      {info.tools.slice(0, 5).map((tool) => (
                        <span key={tool.name} className="inline-flex px-sm py-0.5 bg-[rgba(99,102,241,0.15)] text-primary text-xs rounded-sm">
                          {tool.name}
                        </span>
                      ))}
                      {info.tools.length > 5 && (
                        <span className="inline-flex px-sm py-0.5 bg-[rgba(99,102,241,0.15)] text-primary text-xs rounded-sm">
                          +{info.tools.length - 5} more
                        </span>
                      )}
                    </div>
                  )}
                  {hasServerError(info) && (
                    <div className="text-xs text-[#ef4444] mt-xs">
                      {getServerErrorMessage(info)}
                    </div>
                  )}
                </div>
                <div className="flex gap-xs shrink-0">
                  {!info.server.is_valid && info.server.server_type === "stdio" && (
                    <Button
                      type="button"
                      variant="warning"
                      size="sm"
                      onClick={() => handleAutoFix(info)}
                      disabled={isLoading}
                      title="Attempt to resolve executable path automatically"
                    >
                      Auto-fix
                    </Button>
                  )}
                  {isRunning ? (
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      onClick={() => handleStop(info)}
                      disabled={isLoading}
                    >
                      {isLoading ? "..." : "Stop"}
                    </Button>
                  ) : (
                    <Button
                      type="button"
                      variant="primary"
                      size="sm"
                      onClick={() => handleStart(info)}
                      disabled={isLoading || !info.server.enabled}
                    >
                      {isLoading ? "..." : "Start"}
                    </Button>
                  )}
                  {onEditServer && (
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      onClick={() => onEditServer(info)}
                      disabled={isLoading}
                    >
                      Edit
                    </Button>
                  )}
                  <Button
                    type="button"
                    variant="danger"
                    size="sm"
                    onClick={() => handleRemove(info)}
                    disabled={isLoading || isRunning}
                    title={isRunning ? "Stop server before removing" : undefined}
                  >
                    Remove
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="mt-sm pt-md border-t border-border">
        <p className="text-xs text-text-secondary m-0">
          Learn more about{" "}
          <a
            href="https://modelcontextprotocol.io/introduction"
            target="_blank"
            rel="noopener noreferrer"
            className="text-primary no-underline hover:underline"
          >
            Model Context Protocol
          </a>
        </p>
      </div>
    </div>
  );
};

export default McpServersPanel;
