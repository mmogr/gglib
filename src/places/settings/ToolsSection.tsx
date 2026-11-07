/**
 * Settings › Tools: the tool servers, whether MCP clients may draw, the
 * agent limits under one disclosure, and the title prompt. Each setting
 * saves on change.
 *
 * Interim until S2 (`feat(gui)/settings-tools`): the tool servers are
 * today's `McpServersPanel` with its add and edit dialog, over one list
 * (ST-8), read when the section opens. "This device" is the chat's
 * per-device limits panel (`AgentLimitsSection`), kept in this browser.
 *
 * @module places/settings/ToolsSection
 */

import { useState, type FC } from 'react';
import { AddMcpServerModal } from '../../components/AddMcpServerModal';
import { McpServersPanel } from '../../components/McpServersPanel';
import { AgentLimitsSection } from '../../components/ToolsPopover/AgentLimitsSection';
import { MAX_STAGNATION_STEPS, MAX_TOOL_ITERATIONS } from '../../constants/settingsDefaults';
import { useMcpServers } from '../../hooks/useMcpServers';
import { DEFAULT_TITLE_GENERATION_PROMPT, type McpServerInfo } from '../../services/transport';
import { Disclosure } from './Disclosure';
import { NumberSetting, TextSetting, ToggleSetting } from './fields';

/** The tool servers, and the dialog that adds or edits one (until S2). */
const ToolServers: FC = () => {
  const mcp = useMcpServers();
  const { addServer, updateServer } = mcp;
  // `null` closed; `'new'` adding; else the server being edited.
  const [sheet, setSheet] = useState<McpServerInfo | 'new' | null>(null);

  return (
    <>
      <McpServersPanel mcp={mcp} onAddServer={() => setSheet('new')} onEditServer={setSheet} />
      {sheet !== null && (
        <AddMcpServerModal
          isOpen
          editingServer={sheet === 'new' ? undefined : sheet}
          onClose={() => setSheet(null)}
          onSave={async (data) => {
            if (sheet === 'new') await addServer(data);
            else await updateServer(sheet.server.id, data);
            setSheet(null);
          }}
        />
      )}
    </>
  );
};

export const ToolsSection: FC = () => (
    <>
      <ToolServers />

      <ToggleSetting k="mcpDrawing" id="mcp-drawing-input" label="Let MCP clients draw">
        Offers clients of the endpoint&apos;s <code>/mcp</code> gglib&apos;s drawing tool. Off unless you
        turn it on: a drawing holds the GPU for minutes.
      </ToggleSetting>

      <Disclosure label="Agent limits">
        <NumberSetting
          k="maxToolIterations"
          id="max-tool-iterations-input"
          label="Max tool rounds"
          spec={MAX_TOOL_ITERATIONS}
          description="How many rounds of tool calls an agent run makes before it stops."
        />
        <NumberSetting
          k="maxStagnationSteps"
          id="max-stagnation-steps-input"
          label="Max stuck steps"
          spec={MAX_STAGNATION_STEPS}
          description="Repeats of one reply, anywhere in the run, before an agent stops. The built-in agent and the endpoint's loop guard share it."
        />
        <section aria-label="This device" className="flex flex-col gap-xs">
          <span className="text-sm font-semibold text-text">This device</span>
          <AgentLimitsSection />
        </section>
      </Disclosure>

      <TextSetting
        k="titleGenerationPrompt"
        id="title-prompt-input"
        label="Title prompt"
        multiline
        resettable
        placeholder={DEFAULT_TITLE_GENERATION_PROMPT}
        description="What a chat's title is generated from. Empty uses the built-in prompt shown."
      />
    </>
);
