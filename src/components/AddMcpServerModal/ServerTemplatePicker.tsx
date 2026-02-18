import { FC } from "react";
import type { McpServerType } from "../../services/transport/types/mcp";

interface ServerTemplatePickerProps {
  onSelectTemplate: (template: ServerTemplate) => void;
}

export interface ServerTemplate {
  name: string;
  type: McpServerType;
  command: string;
  args: string[];
  envKeys: string[];
  description: string;
}

export const SERVER_TEMPLATES: ServerTemplate[] = [
  {
    name: "Tavily Web Search",
    type: "stdio",
    command: "npx",
    args: ["-y", "tavily-mcp"],
    envKeys: ["TAVILY_API_KEY"],
    description: "Search the web using Tavily API (may download package on first run)",
  },
  {
    name: "Filesystem Access",
    type: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed"],
    envKeys: [],
    description: "Read/write files in allowed directories (may download package on first run)",
  },
  {
    name: "GitHub",
    type: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-github"],
    envKeys: ["GITHUB_PERSONAL_ACCESS_TOKEN"],
    description: "Interact with GitHub repositories (may download package on first run)",
  },
  {
    name: "Brave Search",
    type: "stdio",
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-brave-search"],
    envKeys: ["BRAVE_API_KEY"],
    description: "Search the web using Brave Search API (may download package on first run)",
  },
];

export const ServerTemplatePicker: FC<ServerTemplatePickerProps> = ({ onSelectTemplate }) => {
  return (
    <div className="flex flex-col gap-xs">
      <label className="text-sm font-semibold text-text">Quick Start Templates</label>
      <div className="grid grid-cols-2 gap-sm">
        {SERVER_TEMPLATES.map((template) => (
          <button
            key={template.name}
            type="button"
            className="flex flex-col items-start gap-[2px] px-md py-sm bg-background-secondary border border-border rounded-base cursor-pointer text-left transition-[border-color,background] duration-150 hover:border-primary hover:bg-background-tertiary"
            onClick={() => onSelectTemplate(template)}
          >
            <span className="text-sm font-semibold text-text">{template.name}</span>
            <span className="text-xs text-text-secondary">{template.description}</span>
          </button>
        ))}
      </div>
    </div>
  );
};
