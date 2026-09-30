/**
 * Deleting a message on a far chat: its rows are the far machine's, and a
 * delete here would reach this machine's daemon with their ids. Nothing is
 * deleted, and nothing is asked.
 */

import { describe, it, expect, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import type { ThreadRuntime } from '@assistant-ui/react';

const transport = vi.hoisted(() => ({ getMessages: vi.fn(async () => []), deleteMessage: vi.fn(async () => 1) }));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

import { useMessageDeletion } from '../../../src/components/ChatMessagesPanel/hooks/useMessageDeletion';

const threadRuntime = { getState: () => ({ messages: [] }), reset: vi.fn() } as unknown as ThreadRuntime;

function mount(readOnly: boolean) {
  return renderHook(() =>
    useMessageDeletion({
      threadRuntime,
      activeConversationId: 1,
      activeConversation: null,
      syncConversations: vi.fn(async () => {}),
      showToast: vi.fn(),
      readOnly,
    }),
  );
}

describe('useMessageDeletion on a far chat', () => {
  it('opens no confirmation and deletes nothing', async () => {
    const { result } = mount(true);

    act(() => result.current.initiateDelete('db-40'));
    await act(async () => result.current.confirmDelete());

    expect(result.current.isDeleteModalOpen).toBe(false);
    expect(transport.deleteMessage).not.toHaveBeenCalled();
    expect(transport.getMessages).not.toHaveBeenCalled();
  });

  it('on this machine’s chat, the same two steps delete the row', async () => {
    const { result } = mount(false);

    act(() => result.current.initiateDelete('db-40'));
    expect(result.current.isDeleteModalOpen).toBe(true);
    await act(async () => result.current.confirmDelete());

    expect(transport.deleteMessage).toHaveBeenCalledWith(40);
  });
});
