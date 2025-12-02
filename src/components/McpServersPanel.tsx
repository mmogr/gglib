/**
 * MCP Servers settings panel.
 *
 * Displays a list of configured MCP servers with controls to
 * start/stop, add, edit, and remove servers.
 */

import { FC, useState, useCallback } from "react";
import { useMcpServers } from "../hooks/useMcpServers";
import { McpServerInfo, McpService } from "../services/mcp";
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
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  const handleStart = useCallback(
    async (server: McpServerInfo) => {
      const id = server.config.id?.toString();
      if (!id) return;

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
    async (server: McpServerInfo) => {
      const id = server.config.id?.toString();
      if (!id) return;

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
    async (server: McpServerInfo) => {
      const id = server.config.id?.toString();
      if (!id) return;

      const confirmed = window.confirm(
        `Remove MCP server "${server.config.name}"? This cannot be undone.`
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

  const getStatusBadge = (server: McpServerInfo) => {
    if (McpService.isRunning(server)) {
      return <span className={`${styles.badge} ${styles.badgeRunning}`}>Running</span>;
    }
    if (McpService.hasError(server)) {
      return <span className={`${styles.badge} ${styles.badgeError}`}>Error</span>;
    }
    if (server.status === "starting") {
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
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={refresh}
          >
            Refresh
          </button>
          {onAddServer && (
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={onAddServer}
            >
              Add Server
            </button>
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
            <button
              type="button"
              className="btn btn-primary"
              onClick={onAddServer}
            >
              Add Your First Server
            </button>
          )}
        </div>
      ) : (
        <div className={styles.serverList}>
          {servers.map((server) => {
            const id = server.config.id?.toString() || "";
            const isLoading = actionLoading === id;
            const isRunning = McpService.isRunning(server);

            return (
              <div key={id} className={styles.serverCard}>
                <div className={styles.serverInfo}>
                  <div className={styles.serverHeader}>
                    <span className={styles.serverName}>{server.config.name}</span>
                    {getStatusBadge(server)}
                  </div>
                  <div className={styles.serverDetails}>
                    <span className={styles.serverType}>
                      {server.config.type === "stdio" ? "Stdio" : "SSE"}
                    </span>
                    {server.config.command && (
                      <code className={styles.serverCommand}>
                        {server.config.command}
                        {server.config.args?.length ? ` ${server.config.args.join(" ")}` : ""}
                      </code>
                    )}
                    {server.config.url && (
                      <code className={styles.serverCommand}>{server.config.url}</code>
                    )}
                  </div>
                  {isRunning && server.tools.length > 0 && (
                    <div className={styles.serverTools}>
                      <span className={styles.toolsLabel}>Tools:</span>
                      {server.tools.slice(0, 5).map((tool) => (
                        <span key={tool.name} className={styles.toolChip}>
                          {tool.name}
                        </span>
                      ))}
                      {server.tools.length > 5 && (
                        <span className={styles.toolChip}>
                          +{server.tools.length - 5} more
                        </span>
                      )}
                    </div>
                  )}
                  {McpService.hasError(server) && (
                    <div className={styles.serverError}>
                      {McpService.getErrorMessage(server)}
                    </div>
                  )}
                </div>
                <div className={styles.serverActions}>
                  {isRunning ? (
                    <button
                      type="button"
                      className="btn btn-secondary btn-sm"
                      onClick={() => handleStop(server)}
                      disabled={isLoading}
                    >
                      {isLoading ? "..." : "Stop"}
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="btn btn-primary btn-sm"
                      onClick={() => handleStart(server)}
                      disabled={isLoading || !server.config.enabled}
                    >
                      {isLoading ? "..." : "Start"}
                    </button>
                  )}
                  {onEditServer && (
                    <button
                      type="button"
                      className="btn btn-secondary btn-sm"
                      onClick={() => onEditServer(server)}
                      disabled={isLoading}
                    >
                      Edit
                    </button>
                  )}
                  <button
                    type="button"
                    className="btn btn-danger btn-sm"
                    onClick={() => handleRemove(server)}
                    disabled={isLoading || isRunning}
                    title={isRunning ? "Stop server before removing" : undefined}
                  >
                    Remove
                  </button>
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
