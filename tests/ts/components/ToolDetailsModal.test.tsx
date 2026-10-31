/**
 * ToolDetailsModal: a call names the MCP server its tool belongs to, beside
 * its raw name, and a call to one of gglib's own tools names none. A saved
 * call reloaded shows the same as the live one.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import type { ComponentProps } from 'react';
import ToolDetailsModal from '../../../src/components/ToolUsageBadge/ToolDetailsModal';
import { buildLoadedMessage, foldToolMessages } from '../../../src/hooks/useChatPersistence/buildLoadedMessage';
import type { ChatMessage } from '../../../src/services/transport';

type Calls = ComponentProps<typeof ToolDetailsModal>['toolCalls'];

const calls = [
  {
    type: 'tool-call',
    toolCallId: 'tc-1',
    toolName: '3:tavily_search',
    args: {},
    displayName: 'Tavily Search',
    server: 'Tavily Web Search',
  },
  {
    type: 'tool-call',
    toolCallId: 'tc-2',
    toolName: 'builtin:get_current_time',
    args: {},
    displayName: 'Get Current Time',
  },
] as unknown as Calls;

describe('ToolDetailsModal', () => {
  it('names the MCP server beside the raw name', () => {
    render(<ToolDetailsModal toolCalls={calls.slice(0, 1)} onClose={() => {}} />);

    expect(screen.getByText('(3:tavily_search)')).toBeInTheDocument();
    expect(screen.getByText('on Tavily Web Search')).toBeInTheDocument();
  });

  it('names no server for a call that has none', () => {
    render(<ToolDetailsModal toolCalls={calls.slice(1)} onClose={() => {}} />);

    expect(screen.getByText('(builtin:get_current_time)')).toBeInTheDocument();
    expect(screen.queryByText(/^on\b/)).not.toBeInTheDocument();
  });

  it('names the MCP server and display name of a reloaded call, as the daemon saved them', () => {
    const saved: ChatMessage[] = [
      {
        id: 1,
        conversation_id: 1,
        role: 'assistant',
        content: '',
        created_at: '2026-10-31T00:00:00Z',
        metadata: {
          tool_calls: [
            { id: 'tc-1', name: '3:tavily_search', arguments: {}, display_name: 'Tavily Search', server: 'Tavily Web Search' },
          ],
        },
      },
      { id: 2, conversation_id: 1, role: 'tool', content: 'found', created_at: '2026-10-31T00:00:00Z', metadata: { tool_call_id: 'tc-1' } },
    ];
    const loaded = buildLoadedMessage(foldToolMessages(saved)[0], 1).content as unknown as Calls;
    render(<ToolDetailsModal toolCalls={loaded} onClose={() => {}} />);

    expect(screen.getByText('Tavily Search')).toBeInTheDocument();
    expect(screen.getByText('on Tavily Web Search')).toBeInTheDocument();
  });
});
