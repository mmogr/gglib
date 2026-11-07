/**
 * The add sheet's quick starts: a card each that fills Name, Command,
 * Arguments and the variable it needs. Every one is a stdio server that
 * needs an API key; none reads files, which is built in (a chat's folder,
 * ADR 0023), so none can fail on a placeholder path (ST-10).
 *
 * @module places/settings/tools/TemplatePicker
 */

import type { FC } from 'react';
import { Label, Stack } from '../../../components/primitives';

export interface ServerTemplate {
  name: string;
  command: string;
  args: string[];
  envKeys: string[];
  description: string;
}

export const SERVER_TEMPLATES: ServerTemplate[] = [
  {
    name: 'Tavily Web Search',
    command: 'npx',
    args: ['-y', 'tavily-mcp'],
    envKeys: ['TAVILY_API_KEY'],
    description: 'Search the web using Tavily API (may download package on first run)',
  },
  {
    name: 'GitHub',
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-github'],
    envKeys: ['GITHUB_PERSONAL_ACCESS_TOKEN'],
    description: 'Interact with GitHub repositories (may download package on first run)',
  },
  {
    name: 'Brave Search',
    command: 'npx',
    args: ['-y', '@modelcontextprotocol/server-brave-search'],
    envKeys: ['BRAVE_API_KEY'],
    description: 'Search the web using Brave Search API (may download package on first run)',
  },
];

export const TemplatePicker: FC<{ onPick: (template: ServerTemplate) => void }> = ({ onPick }) => (
  <Stack gap="xs">
    <Label size="sm">Quick start</Label>
    <div className="grid grid-cols-2 gap-sm">
      {SERVER_TEMPLATES.map((template) => (
        // eslint-disable-next-line no-restricted-syntax -- multi-line selectable card, not a Button-shaped control
        <button
          key={template.name}
          type="button"
          className="flex flex-col items-start gap-[2px] px-md py-sm bg-surface-elevated rounded-md cursor-pointer text-left transition-colors duration-150 hover:bg-surface-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
          onClick={() => onPick(template)}
        >
          <span className="text-sm font-semibold text-text">{template.name}</span>
          <span className="text-xs text-text-secondary">{template.description}</span>
        </button>
      ))}
    </div>
    <span className="text-xs text-text-secondary">
      Reading files is built in: choose a folder in a chat. Every template here needs an API key.
    </span>
  </Stack>
);
