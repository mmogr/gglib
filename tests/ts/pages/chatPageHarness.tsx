/**
 * What the chat page tests share: a transport stub the test shapes, the
 * providers the page needs, and the jsdom gaps its tree falls into.
 *
 * Each test file mocks the transport module itself (vi.mock is hoisted per
 * file) and hands `getTransport` whatever `chatTransport` built.
 */

import { vi } from 'vitest';
import type { ReactNode } from 'react';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import type { ChatMessage, ConversationSummary } from '../../../src/services/transport';
import type { RunInfo } from '../../../src/types/generated/RunInfo';

// jsdom has no ResizeObserver and assistant-ui's composer measures itself
// with one.
class NoopResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= NoopResizeObserver as unknown as typeof ResizeObserver;
// Nor does it scroll: the thread's viewport scrolls itself to the latest turn.
Element.prototype.scrollTo ??= function scrollTo() {};

export const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>
    <ConfirmProvider>
      <SettingsProvider showToast={() => {}}>{children}</SettingsProvider>
    </ConfirmProvider>
  </ToastProvider>
);

export function conversation(id: number, title: string): ConversationSummary {
  return {
    id,
    title,
    model_id: null,
    system_prompt: null,
    settings: null,
    created_at: '2026-09-01T09:00:00Z',
    updated_at: '2026-09-01T09:00:00Z',
  };
}

export function agentRun(id: string, conversationId: number, status: RunInfo['status']): RunInfo {
  return {
    id,
    kind: 'agent',
    status,
    created_at_ms: 1,
    conversation_id: conversationId,
    last_seq: 0,
    ...(status !== 'queued' && status !== 'in_progress' && { finished_at_ms: 2 }),
  };
}

/** A run's frames, then a stream held open until the reader leaves. */
export async function* framesThenWait(frames: object[], signal: AbortSignal) {
  let seq = 0;
  for (const frame of frames) {
    seq += 1;
    yield { type: 'frame' as const, seq, data: JSON.stringify(frame) };
  }
  await new Promise<void>((resolve) => signal.addEventListener('abort', () => resolve()));
}

export interface ChatFixture {
  conversations: ConversationSummary[];
  rows: Record<number, ChatMessage[]>;
  runs: RunInfo[];
  frames: Record<string, object[]>;
}

/** A transport over `fixture`, which a test may change as it goes. */
export function chatTransport(fixture: ChatFixture) {
  return {
    getServerToolSupport: vi.fn(async () => ({ supports_tool_calls: true, detected_format: null })),
    getModel: vi.fn(async () => ({ quantization: 'Q8_0' })),
    // The composer's model picker lists these; none unless a test says so.
    listModels: vi.fn(async () => []),
    // A copy, as a fetch gives: the page must not see the fixture change under it.
    listConversations: vi.fn(async () => [...fixture.conversations]),
    createConversation: vi.fn(async (params: { title: string }) => {
      const id = Math.max(0, ...fixture.conversations.map((c) => c.id)) + 1;
      fixture.conversations.unshift(conversation(id, params.title));
      return id;
    }),
    deleteConversation: vi.fn(async (id: number) => {
      fixture.conversations = fixture.conversations.filter((c) => c.id !== id);
    }),
    getMessages: vi.fn(async (id: number) => fixture.rows[id] ?? []),
    listRuns: vi.fn(async () => fixture.runs),
    readRunEvents: (id: string, _after: number, signal: AbortSignal) =>
      framesThenWait(fixture.frames[id] ?? [], signal),
    getSettings: vi.fn(async () => ({})),
    subscribe: vi.fn(() => () => {}),
  };
}
