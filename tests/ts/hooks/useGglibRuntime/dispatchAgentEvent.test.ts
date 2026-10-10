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
} from '../../../../src/hooks/useGglibRuntime/agentEventDispatch';

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
    onSystemWarning: overrides?.onSystemWarning,
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
        display_name: 'Search',
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
      displayName: 'Search',
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
        tool_name: 'search',
        result: { tool_call_id: 'tc-1', content: 'found it', success: true },
        wait_ms: 5,
        execute_duration_ms: 50,
        display_name: 'Search',
        duration_display: '50ms',
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
// system_warning
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — system_warning', () => {
  it('reports the warning and lets the stream continue', () => {
    // Regression guard: this variant used to fall through to the
    // forward-compatibility default and vanish, so retry notices — and the
    // parallel-tool-limit warning that predates them — were invisible in the
    // GUI while the CLI renderer showed them.
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const cleanup = vi.fn();
    const onSystemWarning = vi.fn();
    const deps = makeDeps(store.setMessages, { cleanup, onSystemWarning });

    const done = dispatchAgentEvent(
      { type: 'system_warning', message: 'Model unavailable — retrying in 2.0s (attempt 1)' },
      state,
      deps,
    );

    expect(done).toBe(false);
    expect(onSystemWarning).toHaveBeenCalledWith(
      'Model unavailable — retrying in 2.0s (attempt 1)',
      undefined,
    );
    expect(cleanup).not.toHaveBeenCalled();
  });

  it('forwards a suggested action when the backend supplies one', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const onSystemWarning = vi.fn();
    const deps = makeDeps(store.setMessages, { onSystemWarning });

    dispatchAgentEvent(
      {
        type: 'system_warning',
        message: 'too many parallel tool calls',
        suggested_action: 'gglib config set max_parallel 8',
      },
      state,
      deps,
    );

    expect(onSystemWarning).toHaveBeenCalledWith(
      'too many parallel tool calls',
      'gglib config set max_parallel 8',
    );
  });

  it('does not require a handler to be wired', () => {
    // The CLI-style caller passes no callback; dropping the notice is fine,
    // throwing is not.
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    expect(() =>
      dispatchAgentEvent({ type: 'system_warning', message: 'heads up' }, state, deps),
    ).not.toThrow();
  });

  it('leaves message content untouched', () => {
    // The notice is transient UI, not part of the assistant's answer — it must
    // never reach the persisted transcript.
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages, { onSystemWarning: vi.fn() });

    dispatchAgentEvent({ type: 'system_warning', message: 'heads up' }, state, deps);

    expect(partsOf(store.messages()[0])).toHaveLength(0);
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

// ---------------------------------------------------------------------------
// prompt_progress
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — prompt_progress', () => {
  it('keeps the latest reading on the current message and nowhere else', () => {
    const store = makeMessageStore([emptyAssistant(), emptyAssistant(MSG_ID_2)]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    const progress = (processed: number): AgentEvent => ({
      type: 'prompt_progress',
      processed,
      total: 3420,
      cached: 2100,
      time_ms: 900,
    });
    expect(dispatchAgentEvent(progress(1240), state, deps)).toBe(false);
    dispatchAgentEvent(progress(3420), state, deps);

    const custom = (m: GglibMessage) => (m.metadata as { custom?: Record<string, unknown> } | undefined)?.custom;
    expect(custom(store.messages()[0])?.prompt).toEqual({ processed: 3420, total: 3420, cached: 2100 });
    expect(custom(store.messages()[1])?.prompt).toBeUndefined();
    // The reading is not content: nothing is drawn into the reply's text.
    expect(partsOf(store.messages()[0])).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// tool_progress and waiting
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — tool_progress', () => {
  const start = (id: string): AgentEvent => ({
    type: 'tool_call_start',
    tool_call: { id, name: 'builtin:generate_image', arguments: {} },
    display_name: 'Generate Image',
  });
  const done = (id: string): AgentEvent => ({
    type: 'tool_call_complete',
    tool_name: 'builtin:generate_image',
    result: { tool_call_id: id, content: 'Drew 1 image.', success: true },
    wait_ms: 0,
    execute_duration_ms: 76000,
    display_name: 'Generate Image',
    duration_display: '76s',
  });
  const progressOf = (store: ReturnType<typeof makeMessageStore>, id: string) =>
    (partsOf(store.messages()[0]).find((p) => p.type === 'tool-call' && p.toolCallId === id) as { progress?: unknown })
      .progress;

  it('keeps the latest progress on its own call, continues, and touches no other call', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);
    dispatchAgentEvent(start('c1'), state, deps);
    dispatchAgentEvent(start('c2'), state, deps);

    expect(dispatchAgentEvent({ type: 'tool_progress', tool_call_id: 'c1', stage: 'queued', position: 2 }, state, deps)).toBe(false);
    expect(progressOf(store, 'c1')).toMatchObject({ stage: 'queued', position: 2 });
    dispatchAgentEvent({ type: 'tool_progress', tool_call_id: 'c1', stage: 'sampling', pass: 1, done: 3, total: 20 }, state, deps);

    expect(progressOf(store, 'c1')).toMatchObject({ stage: 'sampling', pass: 1, done: 3, total: 20 });
    // The newer report replaces the older whole: no place in line is left over.
    expect((progressOf(store, 'c1') as { position?: number }).position).toBeUndefined();
    expect(progressOf(store, 'c2')).toBeUndefined();
    expect(partsOf(store.messages()[0])).toHaveLength(2);
  });

  it('a call that has its result keeps no progress, and takes none that arrives late', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);
    dispatchAgentEvent(start('c1'), state, deps);
    dispatchAgentEvent({ type: 'tool_progress', tool_call_id: 'c1', stage: 'decoding' }, state, deps);
    dispatchAgentEvent(done('c1'), state, deps);
    expect(progressOf(store, 'c1')).toBeUndefined();

    dispatchAgentEvent({ type: 'tool_progress', tool_call_id: 'c1', stage: 'finishing' }, state, deps);
    expect(progressOf(store, 'c1')).toBeUndefined();
  });

  it('skips a frame that names no call or no stage', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);
    dispatchAgentEvent(start('c1'), state, deps);

    expect(dispatchAgentEvent({ type: 'tool_progress', stage: 'loading' } as unknown as AgentEvent, state, deps)).toBe(false);
    expect(dispatchAgentEvent({ type: 'tool_progress', tool_call_id: 'c1' } as unknown as AgentEvent, state, deps)).toBe(false);
    expect(progressOf(store, 'c1')).toBeUndefined();
  });
});

describe('dispatchAgentEvent — waiting', () => {
  const custom = (m: GglibMessage) => (m.metadata as { custom?: Record<string, unknown> } | undefined)?.custom;

  it('keeps the latest wait on the current message, continues, and draws nothing into the reply', () => {
    const store = makeMessageStore([emptyAssistant(), emptyAssistant(MSG_ID_2)]);
    const state: DispatchState = { currentId: MSG_ID };
    const cleanup = vi.fn();
    const deps = makeDeps(store.setMessages, { cleanup });

    expect(dispatchAgentEvent({ type: 'waiting', reason: 'image_render', step: 3, total: 20, position: 1 }, state, deps)).toBe(false);
    dispatchAgentEvent({ type: 'waiting', reason: 'image_render', step: 4, total: 20, position: 1 }, state, deps);

    expect(custom(store.messages()[0])?.waiting).toEqual({ reason: 'image_render', step: 4, total: 20, position: 1 });
    expect(custom(store.messages()[1])?.waiting).toBeUndefined();
    expect(partsOf(store.messages()[0])).toHaveLength(0);
    expect(cleanup).not.toHaveBeenCalled();
  });

  it('is over once the prompt is read, and a wait after that is kept beside the reading', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);
    const waiting: AgentEvent = { type: 'waiting', reason: 'model_load', step: 0, total: 0, position: 0 };

    dispatchAgentEvent(waiting, state, deps);
    dispatchAgentEvent({ type: 'prompt_progress', processed: 10, total: 40, cached: 0, time_ms: 5 }, state, deps);
    expect(custom(store.messages()[0])?.waiting).toBeUndefined();
    expect(custom(store.messages()[0])?.prompt).toEqual({ processed: 10, total: 40, cached: 0 });

    dispatchAgentEvent(waiting, state, deps);
    expect(custom(store.messages()[0])?.waiting).toEqual({ reason: 'model_load', step: 0, total: 0, position: 0 });
    expect(custom(store.messages()[0])?.prompt).toEqual({ processed: 10, total: 40, cached: 0 });
  });
});

// ---------------------------------------------------------------------------
// turn_usage
// ---------------------------------------------------------------------------

describe('dispatchAgentEvent — turn_usage', () => {
  it('says how the current turn was made, and nothing of any other', () => {
    const store = makeMessageStore([emptyAssistant(MSG_ID_2), emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    const done = dispatchAgentEvent(
      { type: 'turn_usage', model: 'qwen3', prompt_tokens: 30, duration_ms: 900 },
      state,
      deps,
    );

    expect(done).toBe(false);
    const custom = (m: GglibMessage) => (m.metadata as { custom?: Record<string, unknown> } | undefined)?.custom;
    expect(custom(store.messages()[1])?.made).toEqual({ modelName: 'qwen3', promptTokens: 30, turnDurationMs: 900 });
    expect(custom(store.messages()[0])?.made).toBeUndefined();
  });

  it('takes a frame with none of its figures without failing, and says nothing', () => {
    const store = makeMessageStore([emptyAssistant()]);
    const state: DispatchState = { currentId: MSG_ID };
    const deps = makeDeps(store.setMessages);

    expect(() =>
      dispatchAgentEvent({ type: 'turn_usage' } as AgentEvent, state, deps),
    ).not.toThrow();
    const custom = (store.messages()[0].metadata as { custom?: Record<string, unknown> } | undefined)?.custom;
    expect(custom?.made).toBeUndefined();
  });
});
