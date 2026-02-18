import { FC } from "react";
import type { McpServerType } from "../../services/transport/types/mcp";
import { Input } from "../ui/Input";

interface StdioConfigFieldsProps {
  command: string;
  setCommand: (value: string) => void;
  args: string;
  setArgs: (value: string) => void;
  workingDir: string;
  setWorkingDir: (value: string) => void;
  pathExtra: string;
  setPathExtra: (value: string) => void;
  disabled: boolean;
}

interface SseConfigFieldsProps {
  url: string;
  setUrl: (value: string) => void;
  disabled: boolean;
}

interface ServerTypeConfigProps {
  serverType: McpServerType;
  setServerType: (type: McpServerType) => void;
  stdioProps: StdioConfigFieldsProps;
  sseProps: SseConfigFieldsProps;
  disabled: boolean;
}

export const ServerTypeConfig: FC<ServerTypeConfigProps> = ({
  serverType,
  setServerType,
  stdioProps,
  sseProps,
  disabled,
}) => {
  return (
    <>
      <div className="flex flex-col gap-xs">
        <label className="text-sm font-semibold text-text">Connection Type</label>
        <div className="flex gap-lg">
          <label className="flex items-center gap-sm text-sm text-text cursor-pointer [&>input]:m-0 [&>input]:accent-primary">
            <input
              type="radio"
              name="serverType"
              checked={serverType === "stdio"}
              onChange={() => setServerType("stdio")}
              disabled={disabled}
            />
            <span>Stdio (spawn process)</span>
          </label>
          <label className="flex items-center gap-sm text-sm text-text cursor-pointer [&>input]:m-0 [&>input]:accent-primary">
            <input
              type="radio"
              name="serverType"
              checked={serverType === "sse"}
              onChange={() => setServerType("sse")}
              disabled={disabled}
            />
            <span>SSE (connect to URL)</span>
          </label>
        </div>
      </div>

      {serverType === "stdio" && <StdioConfigFields {...stdioProps} />}
      {serverType === "sse" && <SseConfigFields {...sseProps} />}
    </>
  );
};

const StdioConfigFields: FC<StdioConfigFieldsProps> = ({
  command,
  setCommand,
  args,
  setArgs,
  workingDir,
  setWorkingDir,
  pathExtra,
  setPathExtra,
  disabled,
}) => {
  return (
    <>
      <div className="flex flex-col gap-xs">
        <label className="text-sm font-semibold text-text" htmlFor="mcp-command">
          Command *
        </label>
        <Input
          id="mcp-command"
          type="text"
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          placeholder="npx, python3, node"
          disabled={disabled}
        />
        <span className="text-xs text-text-secondary">
          Single executable name or path (no arguments). Will be resolved via PATH.
        </span>
      </div>

      <div className="flex flex-col gap-xs">
        <label className="text-sm font-semibold text-text" htmlFor="mcp-args">
          Arguments
        </label>
        <Input
          id="mcp-args"
          type="text"
          value={args}
          onChange={(e) => setArgs(e.target.value)}
          placeholder="-y @tavily/mcp-server"
          disabled={disabled}
        />
        <span className="text-xs text-text-secondary">Space-separated arguments</span>
      </div>

      <div className="flex flex-col gap-xs">
        <label className="text-sm font-semibold text-text" htmlFor="mcp-working-dir">
          Working Directory
        </label>
        <Input
          id="mcp-working-dir"
          type="text"
          value={workingDir}
          onChange={(e) => setWorkingDir(e.target.value)}
          placeholder="(optional) /absolute/path/to/directory"
          disabled={disabled}
        />
        <span className="text-xs text-text-secondary">Must be absolute if specified</span>
      </div>

      <div className="flex flex-col gap-xs">
        <label className="text-sm font-semibold text-text" htmlFor="mcp-path-extra">
          Additional PATH Entries
        </label>
        <Input
          id="mcp-path-extra"
          type="text"
          value={pathExtra}
          onChange={(e) => setPathExtra(e.target.value)}
          placeholder="(optional) /custom/bin:/other/path"
          disabled={disabled}
        />
        <span className="text-xs text-text-secondary">Colon-separated paths added to child process PATH</span>
      </div>
    </>
  );
};

const SseConfigFields: FC<SseConfigFieldsProps> = ({ url, setUrl, disabled }) => {
  return (
    <div className="flex flex-col gap-xs">
      <label className="text-sm font-semibold text-text" htmlFor="mcp-url">
        Server URL *
      </label>
      <Input
        id="mcp-url"
        type="url"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        placeholder="http://localhost:3001/sse"
        disabled={disabled}
      />
    </div>
  );
};
