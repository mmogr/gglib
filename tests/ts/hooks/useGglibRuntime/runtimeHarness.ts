/**
 * Driving `useGglibRuntime` against `FakeDaemon`: the real transport, over a
 * fake `fetch`.
 */

import { act, renderHook, waitFor } from '@testing-library/react';
import { expect } from 'vitest';
import { useGglibRuntime, type UseGglibRuntimeOptions } from '../../../../src/hooks/useGglibRuntime/useGglibRuntime';
import type { GglibMessage } from '../../../../src/types/messages';

/** A conversation the page has open, with the default prompt. */
export function conversation(id: number) {
  return { id, system_prompt: 'You are a helpful assistant.', created_at: '2026-09-29T00:00:00Z' };
}

/** A message's text, its reasoning and tool calls left out. */
export function textOf(message: GglibMessage): string {
  if (typeof message.content === 'string') return message.content;
  return message.content
    .map((part) => (part.type === 'text' ? part.text : ''))
    .join('');
}

/** What the thread shows: each message's id, role and text. */
export function shown(messages: GglibMessage[]): Array<[string, string, string]> {
  return messages.map((m) => [m.id ?? '', m.role, textOf(m)]);
}

/** Mount the runtime on `conversationId` and wait for its opening to settle. */
export async function mount(options: UseGglibRuntimeOptions) {
  const hook = renderHook((props: UseGglibRuntimeOptions) => useGglibRuntime(props), {
    initialProps: options,
  });
  await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
  return hook;
}

/** Send `text` without waiting for the reply, which ends when the test says. */
export function send(hook: Awaited<ReturnType<typeof mount>>, text: string): void {
  act(() => {
    void hook.result.current.runtime.thread.append({
      role: 'user',
      content: [{ type: 'text', text }],
    });
  });
}

/**
 * Edit the message after `parentId` to `text`, without waiting for the
 * reply; a reply's message is named by `sourceId`, as its edit composer does.
 */
export function edit(
  hook: Awaited<ReturnType<typeof mount>>,
  parentId: string | null,
  text: string,
  reply?: { sourceId: string },
): void {
  act(() => {
    void hook.result.current.runtime.thread.append({
      parentId,
      ...(reply && { sourceId: reply.sourceId }),
      role: reply ? 'assistant' : 'user',
      content: [{ type: 'text', text }],
    });
  });
}

/** Regenerate the reply to the message `parentId`, without waiting for it. */
export function regenerate(hook: Awaited<ReturnType<typeof mount>>, parentId: string): void {
  act(() => {
    void hook.result.current.runtime.thread.startRun({ parentId });
  });
}
