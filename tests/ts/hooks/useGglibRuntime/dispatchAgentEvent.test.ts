/**
 * Unit tests for dispatchAgentEvent — SSE event dispatch logic.
 *
 * Verifies that each AgentEvent variant correctly mutates React message state,
 * returns the right continue/stop sentinel, and throws on error events.
 *
 * Uses the same `setMessages` stub pattern as agentMessageState.test.ts.
 */

import { describe, it, expect, vi } from 'vitest';
import type { GglibMessage, GglibContent, GglibMessagePart } from '../../../../src/types/messages';
import type { AgentEvent } from '../../../../src/types/events/agentEvent';

// Mock the appLogger used inside dispatchAgentEvent to suppress log calls in tests.
vi.mock('../../../../src/services/platform', () => ({
  appLogger: {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

import {
  dispatchAgentEvent,
  type DispatchState,
  type DispatchDeps,
} from '../../../../src/hooks/useGglibRuntime/streamAgentChat';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MSG_ID = 'msg-1';
const MSG_ID_2 = 'msg-2';

function emptyAssistant(id: string = MSG_ID): GglibMessage {
  return { id, role: 'assistant', content: [] as GglibContent };
}

function partsOf(msg: GglibMessage): GglibMessagePart[] {
  return Array.isArray(msg.content) ? (msg.content as GglibMessagePart[]) : [];
}

/**
 * Capture React `setMessages` calls and apply them to the provided messages.
 */
function makeMessageStore(initial: GglibMessage[]): {
  messages: () => GglibMessage[];
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>;
} {
  let messages = initial;
  const setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>> = (updater) => {
    messages = typeof updater === 'function' ? updater(messages) : updater;
  };
  return { messages: () => messages, setMessages };
}

/** Build standard DispatchDeps with a given setMessages and optional overrides. */
function makeDeps(
  setMessages: React.Dispatch<React.SetStateAction<GglibMessage[]>>,
  overrides?: Partial<DispatchDeps>,
): DispatchDeps {
  return {
    setMessages,
    timingTracker: undefined,
    makeNextMessage: overrides?.makeNextMessage ?? ((iter: number) => `msg-iter-${iter}`),
    cleanup: overrides?.cleanup ?? vi.fn(),
  };
}

// ---------------------------------------------------------------------------
// text_delta
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — text_delta', () => {
  it('returns false (continue) and appends text to the current message', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    const done = dispatchAgentEvent(
      { type: 'text_delta', content: 'Hello' },
      state,
      deps,
    );

    expect(done).toBe(false);
    const parts = partsOf(store.messages()[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'text', text: 'Hello' });
  });

  it('appends multiple deltas to the same text part', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    dispatchAgentEvent({ type: 'text_delta', content: 'Hel' }, state, deps);
    dispatchAgentEvent({ type: 'text_delta', content: 'lo' }, state, deps);

    const parts = partsOf(store.messages()[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'text', text: 'Hello' });
  });
});

// ---------------------------------------------------------------------------
// reasoning_delta
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — reasoning_delta', () => {
  it('returns false and appends a reasoning part', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    const done = dispatchAgentEvent(
      { type: 'reasoning_delta', content: 'Let me think' },
      state,
      deps,
    );

    expect(done).toBe(false);
    const parts = partsOf(store.messages()[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'reasoning', text: 'Let me think' });
  });
});

// ---------------------------------------------------------------------------
// tool_call_start
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — tool_call_start', () => {
  it('returns false and adds a tool-call part', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    const done = dispatchAgentEvent(
      {
        type: 'tool_call_start',
        tool_call: { id: 'tc-1', name: 'search', arguments: { q: 'test' } },
      },
      state,
      deps,
    );

    expect(done).toBe(false);
    const parts = partsOf(store.messages()[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({
      type: 'tool-call',
      toolCallId: 'tc-1',
      toolName: 'search',
      args: { q: 'test' },
    });
  });
});

// ---------------------------------------------------------------------------
// tool_call_complete
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — tool_call_complete', () => {
  it('returns false and stamps result onto the matching tool-call part', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [
        { type: 'tool-call', toolCallId: 'tc-1', toolName: 'search', args: {} },
      ] as GglibContent,
    };
    const store = makeMessageStore([initial]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    const done = dispatchAgentEvent(
      {
        type: 'tool_call_complete',
        result: { tool_call_id: 'tc-1', content: 'found it', success: true },
        wait_ms: 5,
        execute_duration_ms: 50,
      },
      state,
      deps,
    );

    expect(done).toBe(false);
    const part = partsOf(store.messages()[0])[0] as Record<string, unknown>;
    expect(part.result).toBe('found it');
    expect(part.isError).toBe(false);
    expect(part.waitMs).toBe(5);
    expect(part.durationMs).toBe(50);
  });
});

// ---------------------------------------------------------------------------
// iteration_complete
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — iteration_complete', () => {
  it('returns false, calls cleanup, and creates a new message', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const cleanup = vi.fn();
    const makeNextMessage = vi.fn(() => MSG_ID_2);
    const deps = makeDeps(store.setMessages, { cleanup, makeNextMessage });

    const done = dispatchAgentEvent(
      { type: 'iteration_complete', iteration: 1, tool_calls: 2 },
      state,
      deps,
    );

    expect(done).toBe(false);
    expect(cleanup).toHaveBeenCalledOnce();
    expect(makeNextMessage).toHaveBeenCalledWith(2); // iteration + 1
    expect(state.currentId).toBe(MSG_ID_2);
  });
});

// ---------------------------------------------------------------------------
// final_answer
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — final_answer', () => {
  it('returns true (done) and calls cleanup', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const cleanup = vi.fn();
    const deps = makeDeps(store.setMessages, { cleanup });

    const done = dispatchAgentEvent(
      { type: 'final_answer', content: 'The answer is 42' },
      state,
      deps,
    );

    expect(done).toBe(true);
    expect(cleanup).toHaveBeenCalledOnce();
  });

  it('sets full text on the message', () => {
    const initial: GglibMessage = {
      id: MSG_ID,
      role: 'assistant',
      content: [{ type: 'text', text: 'partial' }] as GglibContent,
    };
    const store = makeMessageStore([initial]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    dispatchAgentEvent(
      { type: 'final_answer', content: 'The full answer' },
      state,
      deps,
    );

    const parts = partsOf(store.messages()[0]);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({ type: 'text', text: 'The full answer' });
  });
});

// ---------------------------------------------------------------------------
// error
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — error', () => {
  it('throws an Error with the event message', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const cleanup = vi.fn();
    const deps = makeDeps(store.setMessages, { cleanup });

    expect(() =>
      dispatchAgentEvent(
        { type: 'error', message: 'loop limit reached' },
        state,
        deps,
      ),
    ).toThrow('loop limit reached');
    expect(cleanup).toHaveBeenCalledOnce();
  });
});

// ---------------------------------------------------------------------------
// unknown event type (forward compatibility)
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — unknown event type', () => {
  it('returns false and does not throw', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    const done = dispatchAgentEvent(
      { type: 'some_future_event' } as unknown as AgentEvent,
      state,
      deps,
    );

    expect(done).toBe(false);
  });
});
