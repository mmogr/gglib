/**
 * Settings › Tools: the tool servers and + Add a server, whether MCP
 * clients may draw, the agent limits under one disclosure, the title
 * prompt, and a footnote to what MCP is. Each setting saves on change.
 *
 * The tool servers are read once here and handed down, one list for the
 * rows and the sheet alike (ST-8). "This device" is the chat's per-device
 * limits (`AgentLimitsSection`), kept in this browser.
 *
 * @module places/settings/ToolsSection
 */

import type { FC } from 'react';
import { AgentLimitsSection } from '../../components/AgentLimitsSection';
import { MAX_STAGNATION_STEPS, MAX_TOOL_ITERATIONS } from '../../constants/settingsDefaults';
import { useMcpServers } from '../../hooks/useMcpServers';
import { DEFAULT_TITLE_GENERATION_PROMPT } from '../../services/transport';
import { Disclosure } from './Disclosure';
import { NumberSetting, TextSetting, ToggleSetting } from './fields';
import { ToolServers } from './tools/ToolServers';

export const ToolsSection: FC = () => {
  const mcp = useMcpServers();
  return (
    <>
      <ToolServers mcp={mcp} />

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

      <a
        href="https://modelcontextprotocol.io/introduction"
        target="_blank"
        rel="noopener noreferrer"
        className="self-start text-xs text-text-secondary underline underline-offset-2 hover:text-text"
      >
        Learn more about MCP
      </a>
    </>
  );
};
