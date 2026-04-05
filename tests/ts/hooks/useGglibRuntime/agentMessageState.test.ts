/**
 * Unit tests for agentMessageState — React state mutation helpers.
 *
 * These tests verify the pure functions that apply targeted updates to
 * GglibMessage arrays during streaming.  A lightweight `setMessages` stub
 * captures the functional updater and applies it, avoiding any React
 * runtime dependency.
 */

import { describe, it, expect } from 'vitest';
import type { GglibMessage, GglibContent, GglibMessagePart } from '../../../../src/types/messages';
import type { AgentToolCallCompleteEvent } from '../../../../src/types/events/agentEvent';
import {
  applyTextDelta,
  applyReasoningDelta,
  addToolCallPart,
  applyToolResult,
  finalizeMessageTiming,
} from '../../../../src/hooks/useGglibRuntime/agentMessageState';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MSG_ID = 'msg-1';
const OTHER_ID = 'msg-other';

/**
 * Minimal stub that captures the React `setMessages` functional updater,
 * applies it to the given messages array, and returns the result.
 */
function applyUpdate(
  messages: GglibMessage[],
  action: (set: React.Dispatch<React.SetStateAction<GglibMessage[]>>) => void,
): GglibMessage[] {
  let result: GglibMessage[] = messages;
  const setter: React.Dispatch<React.SetStateAction<GglibMessage[]>> = (updater) => {
    result = typeof updater === 'function' ? updater(result) : updater;
  };
  action(setter);
  return result;
}

function emptyAssistant(id: string = MSG_ID): GglibMessage {
  return { id, role: 'assistant', content: [] as GglibContent };
}

function partsOf(msg: GglibMessage): GglibMessagePart[] {
  return Array.isArray(msg.content) ? (msg.content as GglibMessagePart[]) : [];
}

// ---------------------------------------------------------------------------
// applyTextDelta
// ---------------------------------------------------------------------------

describe('applyTextDelta', () => {
  it('creates a text part when the message content is empty', () => {
    const msgs = applyUpdate([emptyAssistant()], (set) =>
      applyTextDelta(set, MSG_ID, 'Hello'),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'text', text: 'Hello' });
  });

  it('appends to the existing trailing text part', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [{ type: 'text', text: 'Hel' }] as GglibContent,
    };
    const msgs = applyUpdate([initial], (set) =>
      applyTextDelta(set, MSG_ID, 'lo'),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'text', text: 'Hello' });
  });

  it('leaves non-matching messages untouched', () => {
    const other = emptyAssistant(OTHER_ID);
    const target = emptyAssistant(MSG_ID);
    const msgs = applyUpdate([other, target], (set) =>
      applyTextDelta(set, MSG_ID, 'hi'),
    );
    // Other message has no parts added
    expect(partsOf(msgs[0])).toHaveLength(0);
    expect(partsOf(msgs[1])).toHaveLength(1);
  });

  it('creates a new text part when the tail is not a text part', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [{ type: 'reasoning', text: 'think' }] as GglibContent,
    };
    const msgs = applyUpdate([initial], (set) =>
      applyTextDelta(set, MSG_ID, 'answer'),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(2);
    expect(parts[0]).toMatchObject({ type: 'reasoning', text: 'think' });
    expect(parts[1]).toMatchObject({ type: 'text', text: 'answer' });
  });
});

// ---------------------------------------------------------------------------
// applyReasoningDelta
// ---------------------------------------------------------------------------

describe('applyReasoningDelta', () => {
  it('creates a reasoning part when the message content is empty', () => {
    const msgs = applyUpdate([emptyAssistant()], (set) =>
      applyReasoningDelta(set, MSG_ID, 'thinking...'),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'reasoning', text: 'thinking...' });
  });

  it('appends to the existing trailing reasoning part', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [{ type: 'reasoning', text: 'Let me ' }] as GglibContent,
    };
    const msgs = applyUpdate([initial], (set) =>
      applyReasoningDelta(set, MSG_ID, 'think'),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'reasoning', text: 'Let me think' });
  });

  it('creates a new reasoning part when the tail is a text part', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [{ type: 'text', text: 'hello' }] as GglibContent,
    };
    const msgs = applyUpdate([initial], (set) =>
      applyReasoningDelta(set, MSG_ID, 'hmm'),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(2);
    expect(parts[1]).toMatchObject({ type: 'reasoning', text: 'hmm' });
  });
});

// ---------------------------------------------------------------------------
// addToolCallPart
// ---------------------------------------------------------------------------

describe('addToolCallPart', () => {
  it('adds a pending tool-call part with correct fields', () => {
    const msgs = applyUpdate([emptyAssistant()], (set) =>
      addToolCallPart(set, MSG_ID, 'tc-1', 'search', { q: 'test' }),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({
      type: 'tool-call',
      toolCallId: 'tc-1',
      toolName: 'search',
      args: { q: 'test' },
    });
  });

  it('defaults args to {} for non-object arguments', () => {
    const msgs = applyUpdate([emptyAssistant()], (set) =>
      addToolCallPart(set, MSG_ID, 'tc-2', 'fn', 'not-an-object'),
    );
    const parts = partsOf(msgs[0]);
    expect(parts[0]).toMatchObject({ args: {} });
  });

  it('appends to existing parts without overwriting them', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [{ type: 'text', text: 'I will search.' }] as GglibContent,
    };
    const msgs = applyUpdate([initial], (set) =>
      addToolCallPart(set, MSG_ID, 'tc-3', 'search', {}),
    );
    const parts = partsOf(msgs[0]);
    expect(parts).toHaveLength(2);
    expect(parts[0]).toMatchObject({ type: 'text' });
    expect(parts[1]).toMatchObject({ type: 'tool-call', toolCallId: 'tc-3' });
  });
});

// ---------------------------------------------------------------------------
// applyToolResult
// ---------------------------------------------------------------------------

describe('applyToolResult', () => {
  it('stamps a successful result onto the matching tool-call part', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [
        { type: 'tool-call', toolCallId: 'tc-1', toolName: 'search', args: {} },
      ] as GglibContent,
    };
    const event: AgentToolCallCompleteEvent = {
      type: 'tool_call_complete',
      tool_name: 'search',
      result: { tool_call_id: 'tc-1', content: 'found it', success: true },
      wait_ms: 10,
      execute_duration_ms: 50,
      display_name: 'Search',
      duration_display: '50ms',
    };
    const msgs = applyUpdate([initial], (set) =>
      applyToolResult(set, MSG_ID, event),
    );
    const part = partsOf(msgs[0])[0] as Record<string, unknown>;
    expect(part.result).toBe('found it');
    expect(part.isError).toBe(false);
    expect(part.waitMs).toBe(10);
    expect(part.durationMs).toBe(50);
  });

  it('stamps a failure result with isError: true', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [
        { type: 'tool-call', toolCallId: 'tc-2', toolName: 'fn', args: {} },
      ] as GglibContent,
    };
    const event: AgentToolCallCompleteEvent = {
      type: 'tool_call_complete',
      tool_name: 'fn',
      result: { tool_call_id: 'tc-2', content: 'timeout', success: false },
      wait_ms: 0,
      execute_duration_ms: 30000,
      display_name: 'Fn',
      duration_display: '30.0s',
    };
    const msgs = applyUpdate([initial], (set) =>
      applyToolResult(set, MSG_ID, event),
    );
    const part = partsOf(msgs[0])[0] as Record<string, unknown>;
    expect(part.result).toEqual({ error: 'timeout' });
    expect(part.isError).toBe(true);
  });

  it('does not touch tool-call parts with a different toolCallId', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [
        { type: 'tool-call', toolCallId: 'tc-A', toolName: 'fn', args: {} },
        { type: 'tool-call', toolCallId: 'tc-B', toolName: 'fn', args: {} },
      ] as GglibContent,
    };
    const event: AgentToolCallCompleteEvent = {
      type: 'tool_call_complete',
      tool_name: 'fn',
      result: { tool_call_id: 'tc-B', content: 'ok', success: true },
      wait_ms: 0,
      execute_duration_ms: 5,
      display_name: 'Fn',
      duration_display: '5ms',
    };
    const msgs = applyUpdate([initial], (set) =>
      applyToolResult(set, MSG_ID, event),
    );
    const parts = partsOf(msgs[0]);
    // tc-A is untouched (no result field)
    expect((parts[0] as Record<string, unknown>).result).toBeUndefined();
    // tc-B got stamped
    expect((parts[1] as Record<string, unknown>).result).toBe('ok');
  });
});

// ---------------------------------------------------------------------------
// finalizeMessageTiming
// ---------------------------------------------------------------------------

describe('finalizeMessageTiming', () => {
  it('sets timingFinalized: true on the matching message', () => {
    const msgs = applyUpdate([emptyAssistant()], (set) =>
      finalizeMessageTiming(set, MSG_ID),
    );
    const meta = msgs[0].metadata as { custom?: { timingFinalized?: boolean } };
    expect(meta?.custom?.timingFinalized).toBe(true);
  });

  it('is idempotent — calling twice does not re-create metadata', () => {
    let msgs = [emptyAssistant()];
    msgs = applyUpdate(msgs, (set) => finalizeMessageTiming(set, MSG_ID));
    const first = msgs[0];
    msgs = applyUpdate(msgs, (set) => finalizeMessageTiming(set, MSG_ID));
    // Same object reference returned because the guard short-circuits.
    expect(msgs[0]).toBe(first);
  });

  it('preserves existing custom metadata fields', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [],
      metadata: { custom: { turnId: 'turn-1', iteration: 2 } },
    };
    const msgs = applyUpdate([initial], (set) =>
      finalizeMessageTiming(set, MSG_ID),
    );
    const meta = msgs[0].metadata as { custom?: Record<string, unknown> };
    expect(meta?.custom?.turnId).toBe('turn-1');
    expect(meta?.custom?.iteration).toBe(2);
    expect(meta?.custom?.timingFinalized).toBe(true);
  });
});
