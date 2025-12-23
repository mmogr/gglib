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
import styles from "./McpServersPanel.module.css";

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
      return <span className={`${styles.badge} ${styles.badgeRunning}`}>Running</span>;
    }
    if (hasServerError(info)) {
      return <span className={`${styles.badge} ${styles.badgeError}`}>Error</span>;
    }
    if (info.status === "starting") {
      return <span className={`${styles.badge} ${styles.badgeStarting}`}>Starting...</span>;
    }
    return <span className={`${styles.badge} ${styles.badgeStopped}`}>Stopped</span>;
  };

  if (loading) {
    return (
      <div className={styles.loading}>
        <div className={styles.spinner} />
        <span>Loading MCP servers...</span>
      </div>
    );
  }

  return (
    <div className={styles.panel}>
      <div className={styles.header}>
        <div className={styles.headerTitle}>
          <h3>MCP Servers</h3>
          <span className={styles.headerCount}>
            {servers.length} server{servers.length !== 1 ? "s" : ""}
          </span>
        </div>
        <div className={styles.headerActions}>
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
        <div className={styles.error} role="alert">
          {error}
        </div>
      )}

      {actionError && (
        <div className={styles.error} role="alert">
          {actionError}
        </div>
      )}

      {servers.length === 0 ? (
        <div className={styles.emptyState}>
          <p>No MCP servers configured.</p>
          <p className={styles.emptyHint}>
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
        <div className={styles.serverList}>
          {servers.map((info) => {
            const id = info.server.id;
            const isLoading = actionLoading === id;
            const isRunning = isServerRunning(info);

            return (
              <div key={id} className={styles.serverCard}>
                <div className={styles.serverInfo}>
                  <div className={styles.serverHeader}>
                    <span className={styles.serverName}>{info.server.name}</span>
                    {getStatusBadge(info)}
                  </div>
                  <div className={styles.serverDetails}>
                    <span className={styles.serverType}>
                      {info.server.server_type === "stdio" ? "Stdio" : "SSE"}
                    </span>
                    {!info.server.is_valid && (
                      <span className={styles.invalidBadge} title={info.server.last_error || "Invalid configuration"}>
                        <Icon icon={AlertTriangle} size={14} />
                        <span style={{ marginLeft: 6 }}>Needs relink</span>
                      </span>
                    )}
                    {info.server.server_type === "stdio" && info.server.config.command && (
                      <code className={styles.serverCommand}>
                        {info.server.config.command}
                        {info.server.config.args?.length ? ` ${info.server.config.args.join(" ")}` : ""}
                      </code>
                    )}
                    {info.server.config.url && (
                      <code className={styles.serverCommand}>{info.server.config.url}</code>
                    )}
                    {!info.server.is_valid && info.server.last_error && (
                      <div className={styles.validationError}>
                        {info.server.last_error}
                      </div>
                    )}
                  </div>
                  {isRunning && info.tools.length > 0 && (
                    <div className={styles.serverTools}>
                      <span className={styles.toolsLabel}>Tools:</span>
                      {info.tools.slice(0, 5).map((tool) => (
                        <span key={tool.name} className={styles.toolChip}>
                          {tool.name}
                        </span>
                      ))}
                      {info.tools.length > 5 && (
                        <span className={styles.toolChip}>
                          +{info.tools.length - 5} more
                        </span>
                      )}
                    </div>
                  )}
                  {hasServerError(info) && (
                    <div className={styles.serverError}>
                      {getServerErrorMessage(info)}
                    </div>
                  )}
                </div>
                <div className={styles.serverActions}>
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

      <div className={styles.footer}>
        <p className={styles.footerHelp}>
          Learn more about{" "}
          <a
            href="https://modelcontextprotocol.io/introduction"
            target="_blank"
            rel="noopener noreferrer"
          >
            Model Context Protocol
          </a>
        </p>
      </div>
    </div>
  );
};

export default McpServersPanel;
