/**
 * Deleting a message on a far chat: its rows are the far machine's, and a
 * delete here would reach this machine's daemon with their ids. Nothing is
 * deleted, and nothing is asked.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { ThreadRuntime } from '@assistant-ui/react';

const transport = vi.hoisted(() => ({ getThread: vi.fn(async () => ({ messages: [] })), deleteMessage: vi.fn(async () => 1) }));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

import { useMessageDeletion } from '../../../src/components/ChatMessagesPanel/hooks/useMessageDeletion';

const threadRuntime = { getState: () => ({ messages: [] }), reset: vi.fn() } as unknown as ThreadRuntime;

const confirm = vi.fn(async () => true);

function mount(readOnly: boolean) {
  return renderHook(
    ({ readOnly: shown }: { readOnly: boolean }) =>
      useMessageDeletion({
        threadRuntime,
        activeConversationId: 1,
        activeConversation: null,
        syncConversations: vi.fn(async () => {}),
        showToast: vi.fn(),
        confirm,
        readOnly: shown,
      }),
    { initialProps: { readOnly } },
  );
}

beforeEach(() => {
  transport.deleteMessage.mockClear();
  transport.getThread.mockClear();
  confirm.mockReset();
  confirm.mockImplementation(async () => true);
});

describe('useMessageDeletion on a far chat', () => {
  it('asks nothing and deletes nothing', async () => {
    const { result } = mount(true);

    await act(async () => result.current('db-40'));

    expect(confirm).not.toHaveBeenCalled();
    expect(transport.deleteMessage).not.toHaveBeenCalled();
    expect(transport.getThread).not.toHaveBeenCalled();
  });

  it('on this machine’s chat, asked and answered, deletes the row', async () => {
    const { result } = mount(false);

    await act(async () => result.current('db-40'));

    expect(confirm).toHaveBeenCalledTimes(1);
    expect(transport.deleteMessage).toHaveBeenCalledWith(40);
  });

  it('a question answered after the page turns to a far chat deletes nothing', async () => {
    let answer: (yes: boolean) => void = () => {};
    confirm.mockImplementation(() => new Promise<boolean>((resolve) => { answer = resolve; }));
    const { result, rerender } = mount(false);
    let deleting: Promise<void> = Promise.resolve();
    act(() => { deleting = result.current('db-40'); });

    rerender({ readOnly: true });
    await act(async () => {
      answer(true);
      await deleting;
    });

    expect(transport.deleteMessage).not.toHaveBeenCalled();
    expect(transport.getThread).not.toHaveBeenCalled();
  });
});
