/**
 * Modal for adding or editing MCP server configurations.
 */

import { FC, useState, useCallback, useEffect, FormEvent } from "react";
import type { NewMcpServer, McpServerInfo, McpEnvEntry } from "../services/clients/mcp";
import type { McpServerType } from "../services/transport/types/mcp";
import styles from "./AddMcpServerModal.module.css";
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Icon } from "./ui/Icon";
import { Plus, X } from "lucide-react";

interface AddMcpServerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (server: NewMcpServer) => Promise<void>;
  /** If provided, the modal is in edit mode */
  editingServer?: McpServerInfo;
}

/** Preset MCP server templates */
const SERVER_TEMPLATES = [
  {
    name: "Tavily Web Search",
    type: "stdio" as McpServerType,
    command: "npx",
    args: ["-y", "tavily-mcp"],
    envKeys: ["TAVILY_API_KEY"],
    description: "Search the web using Tavily API (may download package on first run)",
  },
  {
    name: "Filesystem Access",
    type: "stdio" as McpServerType,
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed"],
    envKeys: [],
    description: "Read/write files in allowed directories (may download package on first run)",
  },
  {
    name: "GitHub",
    type: "stdio" as McpServerType,
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-github"],
    envKeys: ["GITHUB_PERSONAL_ACCESS_TOKEN"],
    description: "Interact with GitHub repositories (may download package on first run)",
  },
  {
    name: "Brave Search",
    type: "stdio" as McpServerType,
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-brave-search"],
    envKeys: ["BRAVE_API_KEY"],
    description: "Search the web using Brave Search API (may download package on first run)",
  },
];

export const AddMcpServerModal: FC<AddMcpServerModalProps> = ({
  isOpen,
  onClose,
  onSave,
  editingServer,
}) => {
  const isEditing = !!editingServer;

  // Form state
  const [name, setName] = useState("");
  const [serverType, setServerType] = useState<McpServerType>("stdio");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [workingDir, setWorkingDir] = useState("");
  const [pathExtra, setPathExtra] = useState("");
  const [url, setUrl] = useState("");
  const [envVars, setEnvVars] = useState<[string, string][]>([]);
  const [autoStart, setAutoStart] = useState(false);
  const [enabled, setEnabled] = useState(true);

  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset form when opening or changing edit target
  useEffect(() => {
    if (isOpen) {
      if (editingServer) {
        // Populate from existing server
        const srv = editingServer.server;
        setName(srv.name);
        setServerType(srv.server_type);
        setCommand(srv.config.command || "");
        setArgs(srv.config.args?.join(" ") || "");
        setWorkingDir(srv.config.working_dir || "");
        setPathExtra(srv.config.path_extra || "");
        setUrl(srv.config.url || "");
        setEnvVars(srv.env.map(e => [e.key, e.value] as [string, string]));
        setAutoStart(srv.auto_start);
        setEnabled(srv.enabled);
      } else {
        // Reset for new server
        setName("");
        setServerType("stdio");
        setCommand("");
        setArgs("");
        setWorkingDir("");
        setPathExtra("");
        setUrl("");
        setEnvVars([]);
        setAutoStart(false);
        setEnabled(true);
      }
      setError(null);
    }
  }, [isOpen, editingServer]);

  const applyTemplate = useCallback((template: (typeof SERVER_TEMPLATES)[0]) => {
    setName(template.name);
    setServerType(template.type);
    setCommand(template.command);
    setArgs(template.args.join(" "));
    // Pre-populate env var keys with empty values
    setEnvVars(template.envKeys.map((key) => [key, ""]));
  }, []);

  const addEnvVar = useCallback(() => {
    setEnvVars((prev) => [...prev, ["", ""]]);
  }, []);

  const removeEnvVar = useCallback((index: number) => {
    setEnvVars((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const updateEnvVar = useCallback(
    (index: number, field: 0 | 1, value: string) => {
      setEnvVars((prev) => {
        const next = [...prev];
        next[index] = [...next[index]] as [string, string];
        next[index][field] = value;
        return next;
      });
    },
    []
  );

  const handleSubmit = useCallback(
    async (e: FormEvent) => {
      e.preventDefault();
      setError(null);

      // Validation
      if (!name.trim()) {
        setError("Server name is required");
        return;
      }

      if (serverType === "stdio") {
        if (!command.trim()) {
          setError("Command is required for stdio servers");
          return;
        }
        // Validate command has no spaces (prevent accidental full commandline)
        if (command.trim().includes(" ")) {
          setError("Command must be a single executable (e.g., 'npx'), not a full command line. Use Args field for arguments.");
          return;
        }
      }

      if (serverType === "sse" && !url.trim()) {
        setError("URL is required for SSE servers");
        return;
      }

      // Build new server
      const server: NewMcpServer = {
        name: name.trim(),
        server_type: serverType,
        enabled,
        auto_start: autoStart,
        env: envVars
          .filter(([key]) => key.trim())
          .map(([key, value]): McpEnvEntry => ({ key, value })),
        config: {},
      };

      if (serverType === "stdio") {
        server.config.command = command.trim();
        server.config.args = args.trim() ? args.trim().split(/\s+/) : [];
        server.config.working_dir = workingDir.trim() || undefined;
        server.config.path_extra = pathExtra.trim() || undefined;
      } else {
        server.config.url = url.trim();
      }

      setSaving(true);
      try {
        await onSave(server);
        onClose();
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to save server");
      } finally {
        setSaving(false);
      }
    },
    [name, serverType, command, args, workingDir, pathExtra, url, envVars, autoStart, enabled, onSave, onClose]
  );

  if (!isOpen) return null;

  return (
    <Modal
      open={isOpen}
      onClose={onClose}
      title={isEditing ? "Edit MCP Server" : "Add MCP Server"}
      size="lg"
      preventClose={saving}
    >
      <form onSubmit={handleSubmit} className={styles.form}>
            {/* Templates (only for new servers) */}
            {!isEditing && (
              <div className={styles.section}>
                <label className={styles.label}>Quick Start Templates</label>
                <div className={styles.templates}>
                  {SERVER_TEMPLATES.map((template) => (
                    <button
                      key={template.name}
                      type="button"
                      className={styles.templateBtn}
                      onClick={() => applyTemplate(template)}
                    >
                      <span className={styles.templateName}>{template.name}</span>
                      <span className={styles.templateDesc}>{template.description}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Basic Info */}
            <div className={styles.section}>
              <label className={styles.label} htmlFor="mcp-name">
                Server Name *
              </label>
              <Input
                id="mcp-name"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="My MCP Server"
                disabled={saving}
                required
              />
            </div>

            <div className={styles.section}>
              <label className={styles.label}>Connection Type</label>
              <div className={styles.radioGroup}>
                <label className={styles.radioLabel}>
                  <input
                    type="radio"
                    name="serverType"
                    checked={serverType === "stdio"}
                    onChange={() => setServerType("stdio")}
                    disabled={saving}
                  />
                  <span>Stdio (spawn process)</span>
                </label>
                <label className={styles.radioLabel}>
                  <input
                    type="radio"
                    name="serverType"
                    checked={serverType === "sse"}
                    onChange={() => setServerType("sse")}
                    disabled={saving}
                  />
                  <span>SSE (connect to URL)</span>
                </label>
              </div>
            </div>

            {/* Stdio-specific fields */}
            {serverType === "stdio" && (
              <>
                <div className={styles.section}>
                  <label className={styles.label} htmlFor="mcp-command">
                    Command *
                  </label>
                  <Input
                    id="mcp-command"
                    type="text"
                    value={command}
                    onChange={(e) => setCommand(e.target.value)}
                    placeholder="npx, python3, node"
                    disabled={saving}
                  />
                  <span className={styles.hint}>
                    Single executable name or path (no arguments). Will be resolved via PATH.
                  </span>
                </div>

                <div className={styles.section}>
                  <label className={styles.label} htmlFor="mcp-args">
                    Arguments
                  </label>
                  <Input
                    id="mcp-args"
                    type="text"
                    value={args}
                    onChange={(e) => setArgs(e.target.value)}
                    placeholder="-y @tavily/mcp-server"
                    disabled={saving}
                  />
                  <span className={styles.hint}>Space-separated arguments</span>
                </div>

                <div className={styles.section}>
                  <label className={styles.label} htmlFor="mcp-working-dir">
                    Working Directory
                  </label>
                  <Input
                    id="mcp-working-dir"
                    type="text"
                    value={workingDir}
                    onChange={(e) => setWorkingDir(e.target.value)}
                    placeholder="(optional) /absolute/path/to/directory"
                    disabled={saving}
                  />
                  <span className={styles.hint}>Must be absolute if specified</span>
                </div>

                <div className={styles.section}>
                  <label className={styles.label} htmlFor="mcp-path-extra">
                    Additional PATH Entries
                  </label>
                  <Input
                    id="mcp-path-extra"
                    type="text"
                    value={pathExtra}
                    onChange={(e) => setPathExtra(e.target.value)}
                    placeholder="(optional) /custom/bin:/other/path"
                    disabled={saving}
                  />
                  <span className={styles.hint}>Colon-separated paths added to child process PATH</span>
                </div>
              </>
            )}

            {/* SSE-specific fields */}
            {serverType === "sse" && (
              <div className={styles.section}>
                <label className={styles.label} htmlFor="mcp-url">
                  Server URL *
                </label>
                <Input
                  id="mcp-url"
                  type="url"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="http://localhost:3001/sse"
                  disabled={saving}
                />
              </div>
            )}

            {/* Environment Variables */}
            <div className={styles.section}>
              <div className={styles.sectionHeader}>
                <label className={styles.label}>Environment Variables</label>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className={styles.addBtn}
                  onClick={addEnvVar}
                  disabled={saving}
                >
                  <Icon icon={Plus} size={14} />
                  Add variable
                </Button>
              </div>
              {envVars.length === 0 ? (
                <p className={styles.hint}>
                  Add environment variables for API keys and secrets
                </p>
              ) : (
                <div className={styles.envVars}>
                  {envVars.map(([key, value], index) => (
                    <div key={index} className={styles.envRow}>
                      <Input
                        type="text"
                        className={styles.envKey}
                        value={key}
                        onChange={(e) => updateEnvVar(index, 0, e.target.value)}
                        placeholder="KEY"
                        disabled={saving}
                      />
                      <Input
                        type="password"
                        className={styles.envValue}
                        value={value}
                        onChange={(e) => updateEnvVar(index, 1, e.target.value)}
                        placeholder="value"
                        disabled={saving}
                      />
                      <button
                        type="button"
                        className={styles.envRemove}
                        onClick={() => removeEnvVar(index)}
                        disabled={saving}
                        aria-label="Remove variable"
                      >
                        <Icon icon={X} size={14} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* Options */}
            <div className={styles.section}>
              <label className={styles.checkboxLabel}>
                <input
                  type="checkbox"
                  checked={autoStart}
                  onChange={(e) => setAutoStart(e.target.checked)}
                  disabled={saving}
                />
                <span>Auto-start when app launches</span>
              </label>
              <label className={styles.checkboxLabel}>
                <input
                  type="checkbox"
                  checked={enabled}
                  onChange={(e) => setEnabled(e.target.checked)}
                  disabled={saving}
                />
                <span>Enabled (tools included in chat)</span>
              </label>
            </div>

            {error && (
              <div className={styles.error} role="alert">
                {error}
              </div>
            )}

            <div className="modal-footer">
              <Button
                type="button"
                variant="ghost"
                onClick={onClose}
                disabled={saving}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                variant="primary"
                disabled={saving}
              >
                {saving ? "Saving..." : isEditing ? "Update" : "Add Server"}
              </Button>
            </div>
      </form>
    </Modal>
  );
};

export default AddMcpServerModal;
