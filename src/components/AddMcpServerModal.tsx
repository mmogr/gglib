/**
 * Modal for adding or editing MCP server configurations.
 */

import { FC, useState, useCallback, useEffect, FormEvent } from "react";
import type { NewMcpServer, McpServerInfo, McpEnvEntry } from "../services/clients/mcp";
import type { McpServerType } from "../services/transport/types/mcp";
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { ServerTemplatePicker, type ServerTemplate } from "./AddMcpServerModal/ServerTemplatePicker";
import { ServerTypeConfig } from "./AddMcpServerModal/ServerTypeConfig";
import { EnvVarManager } from "./AddMcpServerModal/EnvVarManager";

interface AddMcpServerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (server: NewMcpServer) => Promise<void>;
  /** If provided, the modal is in edit mode */
  editingServer?: McpServerInfo;
}

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

  const applyTemplate = useCallback((template: ServerTemplate) => {
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
      footer={
        <>
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
            form="add-mcp-server-form"
            variant="primary"
            disabled={saving}
          >
            {saving ? "Saving..." : isEditing ? "Update" : "Add Server"}
          </Button>
        </>
      }
    >
      <form id="add-mcp-server-form" onSubmit={handleSubmit} className="flex flex-col gap-md">
            {/* Templates (only for new servers) */}
            {!isEditing && <ServerTemplatePicker onSelectTemplate={applyTemplate} />}

            {/* Basic Info */}
            <div className="flex flex-col gap-xs">
              <label className="text-sm font-semibold text-text" htmlFor="mcp-name">
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

            <ServerTypeConfig
              serverType={serverType}
              setServerType={setServerType}
              stdioProps={{
                command,
                setCommand,
                args,
                setArgs,
                workingDir,
                setWorkingDir,
                pathExtra,
                setPathExtra,
                disabled: saving,
              }}
              sseProps={{
                url,
                setUrl,
                disabled: saving,
              }}
              disabled={saving}
            />

            {/* Environment Variables */}
            <EnvVarManager
              envVars={envVars}
              onAdd={addEnvVar}
              onRemove={removeEnvVar}
              onUpdate={updateEnvVar}
              disabled={saving}
            />

            {/* Options */}
            <div className="flex flex-col gap-xs">
              <label className="flex items-center gap-sm text-sm text-text cursor-pointer [&>input]:m-0 [&>input]:w-4 [&>input]:h-4 [&>input]:accent-primary">
                <input
                  type="checkbox"
                  checked={autoStart}
                  onChange={(e) => setAutoStart(e.target.checked)}
                  disabled={saving}
                />
                <span>Auto-start when app launches</span>
              </label>
              <label className="flex items-center gap-sm text-sm text-text cursor-pointer [&>input]:m-0 [&>input]:w-4 [&>input]:h-4 [&>input]:accent-primary">
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
              <div className="p-md bg-[rgba(239,68,68,0.15)] text-[#ef4444] rounded-base text-sm" role="alert">
                {error}
              </div>
            )}

      </form>
    </Modal>
  );
};

export default AddMcpServerModal;
