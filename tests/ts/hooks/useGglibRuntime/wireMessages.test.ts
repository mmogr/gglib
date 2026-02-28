/**
 * Unit tests for wireMessages — convertToWireMessages().
 *
 * Covers the translation from GglibMessage[] (UI representation) to the flat
 * AgentWireMessage[] wire format expected by the backend, including tool-result
 * injection and content-part extraction edge-cases.
 */

import { describe, it, expect } from 'vitest';
import {
  convertToWireMessages,
  type AgentWireMessage,
} from '../../../../src/hooks/useGglibRuntime/wireMessages';
import type { GglibMessage } from '../../../../src/types/messages';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function systemMsg(content: string): GglibMessage {
  return { id: '1', role: 'system', content };
}

function userMsg(content: GglibMessage['content']): GglibMessage {
  return { id: '2', role: 'user', content };
}

function assistantMsg(content: GglibMessage['content']): GglibMessage {
  return { id: '3', role: 'assistant', content };
}

// ---------------------------------------------------------------------------
// system / user messages
// ---------------------------------------------------------------------------

describe('convertToWireMessages — system/user', () => {
  it('passes a system string message through unchanged', () => {
    const wire = convertToWireMessages([systemMsg('You are helpful.')]);
    expect(wire).toEqual<AgentWireMessage[]>([
      { role: 'system', content: 'You are helpful.' },
    ]);
  });

  it('passes a user string message through unchanged', () => {
    const wire = convertToWireMessages([userMsg('Hello!')]);
    expect(wire).toEqual<AgentWireMessage[]>([
      { role: 'user', content: 'Hello!' },
    ]);
  });

  it('extracts text from a user parts array', () => {
    const wire = convertToWireMessages([
      userMsg([
        { type: 'text', text: 'Hello ' },
        { type: 'text', text: 'world' },
      ]),
    ]);
    expect(wire).toEqual<AgentWireMessage[]>([
      { role: 'user', content: 'Hello world' },
    ]);
  });

  it('strips non-text parts when building user content string', () => {
    // Cast to unknown first — image parts are not in the current GglibMessagePart
    // union but may appear in real UI state (e.g. from multimodal inputs). The
    // test verifies that convertToWireMessages filters them out silently.
    const wire = convertToWireMessages([
      userMsg([
        { type: 'image', image: 'data:image/png;base64,abc' } as unknown as Parameters<typeof userMsg>[0],
        { type: 'text', text: 'describe this' },
      ]),
    ]);
    expect(wire).toEqual<AgentWireMessage[]>([
      { role: 'user', content: 'describe this' },
    ]);
  });
});

// ---------------------------------------------------------------------------
// assistant messages — text only
// ---------------------------------------------------------------------------

describe('convertToWireMessages — assistant text', () => {
  it('emits content: null for an empty parts array', () => {
    const wire = convertToWireMessages([assistantMsg([])]);
    expect(wire).toEqual<AgentWireMessage[]>([
      { role: 'assistant', content: null },
    ]);
  });

  it('emits the joined text for a single text part', () => {
    const wire = convertToWireMessages([
      assistantMsg([{ type: 'text', text: 'Hi there' }]),
    ]);
    expect(wire).toEqual<AgentWireMessage[]>([
      { role: 'assistant', content: 'Hi there' },
    ]);
  });

  it('joins multiple text parts into one string', () => {
    const wire = convertToWireMessages([
      assistantMsg([
        { type: 'text', text: 'Part A ' },
        { type: 'text', text: 'Part B' },
      ]),
    ]);
    expect(wire[0]).toMatchObject({ role: 'assistant', content: 'Part A Part B' });
  });
});

// ---------------------------------------------------------------------------
// assistant messages — tool calls
// ---------------------------------------------------------------------------

describe('convertToWireMessages — assistant tool calls', () => {
  it('emits tool_calls array for tool-call parts', () => {
    const wire = convertToWireMessages([
      assistantMsg([
        {
          type: 'tool-call',
          toolCallId: 'tc1',
          toolName: 'search',
          args: { q: 'foo' },
        },
      ]),
    ]);
    expect(wire).toEqual<AgentWireMessage[]>([
      {
        role: 'assistant',
        content: null,
        tool_calls: [{ id: 'tc1', name: 'search', arguments: { q: 'foo' } }],
      },
    ]);
  });

  it('falls back to parsing argsText when args is absent', () => {
    // Cast to unknown: argsText is a gglib extension on ToolCallPart that is
    // not yet reflected in the GglibToolCallPart interface. The test verifies
    // that wireMessages falls back to JSON-parsing argsText when args is absent.
    const wire = convertToWireMessages([
      assistantMsg([
        {
          type: 'tool-call',
          toolCallId: 'tc2',
          toolName: 'calc',
          argsText: '{"x":1}',
        } as unknown as Parameters<typeof assistantMsg>[0],
      ]),
    ]);
    const msg = wire[0] as Extract<AgentWireMessage, { role: 'assistant' }>;
    expect(msg.tool_calls?.[0].arguments).toEqual({ x: 1 });
  });

  it('omits tool_calls key when there are no tool-call parts', () => {
    const wire = convertToWireMessages([
      assistantMsg([{ type: 'text', text: 'OK' }]),
    ]);
    const msg = wire[0] as Extract<AgentWireMessage, { role: 'assistant' }>;
    expect('tool_calls' in msg).toBe(false);
  });

  it('does NOT emit a tool entry when result is absent', () => {
    const wire = convertToWireMessages([
      assistantMsg([
        {
          type: 'tool-call',
          toolCallId: 'tc3',
          toolName: 'fn',
          args: {},
          // result intentionally omitted — call is still in-flight
        },
      ]),
    ]);
    expect(wire).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// tool result injection
// ---------------------------------------------------------------------------

describe('convertToWireMessages — tool result injection', () => {
  it('emits a tool entry immediately after the assistant entry when result is present', () => {
    const wire = convertToWireMessages([
      assistantMsg([
        {
          type: 'tool-call',
          toolCallId: 'tc4',
          toolName: 'fn',
          args: {},
          result: 'done',
        },
      ]),
    ]);
    expect(wire).toHaveLength(2);
    expect(wire[1]).toEqual<AgentWireMessage>({
      role: 'tool',
      tool_call_id: 'tc4',
      content: 'done',
    });
  });

  it('JSON-stringifies non-string results', () => {
    const wire = convertToWireMessages([
      assistantMsg([
        {
          type: 'tool-call',
          toolCallId: 'tc5',
          toolName: 'fn',
          args: {},
          result: { val: 42 },
        },
      ]),
    ]);
    const toolEntry = wire[1] as Extract<AgentWireMessage, { role: 'tool' }>;
    expect(toolEntry.content).toBe('{"val":42}');
  });

  it('emits one tool entry per tool-call with a result', () => {
    const wire = convertToWireMessages([
      assistantMsg([
        { type: 'tool-call', toolCallId: 'a', toolName: 'f1', args: {}, result: 'r1' },
        { type: 'tool-call', toolCallId: 'b', toolName: 'f2', args: {} },       // no result
        { type: 'tool-call', toolCallId: 'c', toolName: 'f3', args: {}, result: 'r3' },
      ]),
    ]);
    // 1 assistant entry + 2 tool entries (b has no result)
    expect(wire).toHaveLength(3);
    const toolIds = wire.slice(1).map(
      m => (m as Extract<AgentWireMessage, { role: 'tool' }>).tool_call_id,
    );
    expect(toolIds).toEqual(['a', 'c']);
  });
});

// ---------------------------------------------------------------------------
// multi-turn conversation
// ---------------------------------------------------------------------------

describe('convertToWireMessages — multi-turn', () => {
  it('converts a realistic system + user + assistant + tool sequence', () => {
    const messages: GglibMessage[] = [
      systemMsg('You are an assistant.'),
      userMsg('What is 2+2?'),
      assistantMsg([
        { type: 'text', text: 'Let me calculate.' },
        { type: 'tool-call', toolCallId: 'calc1', toolName: 'add', args: { a: 2, b: 2 }, result: '4' },
      ]),
      userMsg('Thanks!'),
    ];

    const wire = convertToWireMessages(messages);

    expect(wire).toEqual<AgentWireMessage[]>([
      { role: 'system',    content: 'You are an assistant.' },
      { role: 'user',      content: 'What is 2+2?' },
      { role: 'assistant', content: 'Let me calculate.', tool_calls: [{ id: 'calc1', name: 'add', arguments: { a: 2, b: 2 } }] },
      { role: 'tool',      tool_call_id: 'calc1', content: '4' },
      { role: 'user',      content: 'Thanks!' },
    ]);
  });
});
