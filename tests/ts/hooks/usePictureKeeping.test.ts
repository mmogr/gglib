/**
 * Where a drawn picture can be fixed or redone, and the chat read again
 * once a newer one is kept: a saved chat of this machine, and never under
 * a reply that is still being written.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ThreadRuntime } from '@assistant-ui/react';

const transport = vi.hoisted(() => ({ getThread: vi.fn() }));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => transport }));
vi.mock('../../../src/hooks/useChatPersistence/buildThreadMessages', () => ({
  buildThreadMessages: vi.fn((rows: unknown, conversation: unknown, id: number) => ({ built: rows, conversation, id })),
}));

import {
  usePictureKeeping,
  type UsePictureKeepingOptions,
} from '../../../src/components/ChatMessagesPanel/hooks/usePictureKeeping';

const reset = vi.fn();
let running = false;
const threadRuntime = { getState: () => ({ isRunning: running }), reset } as unknown as ThreadRuntime;
const conversation = { id: 7, title: 'A fox' } as UsePictureKeepingOptions['activeConversation'];
const models = [{ id: 2, name: 'qwen-image', family: 'qwen-image-2.1', complete: true, missing: [], default: true, resident: false }];

function keeping(over: Partial<UsePictureKeepingOptions> = {}) {
  const options: UsePictureKeepingOptions = {
    threadRuntime,
    activeConversationId: 7,
    activeConversation: conversation,
    source: 'this',
    models: models as UsePictureKeepingOptions['models'],
    picked: 2,
    ...over,
  };
  return renderHook(() => usePictureKeeping(options)).result.current;
}

beforeEach(() => {
  vi.clearAllMocks();
  running = false;
  transport.getThread.mockResolvedValue({ messages: ['row'] });
});

describe('usePictureKeeping', () => {
  it('offers a saved chat of this machine its models and the one it chose', () => {
    expect(keeping()).toMatchObject({ conversationId: 7, models, picked: 2 });
    expect(keeping({ models: undefined, picked: undefined })).toMatchObject({ models: [], picked: null });
  });

  it('offers nothing on a far chat, an unsaved chat, or with no thread to show it in', () => {
    expect(keeping({ source: 'far' })).toBeUndefined();
    expect(keeping({ activeConversationId: null })).toBeUndefined();
    expect(keeping({ threadRuntime: null })).toBeUndefined();
  });

  it('reads the chat again and shows it once a picture is kept', async () => {
    await keeping()!.onKept();

    expect(transport.getThread).toHaveBeenCalledWith(7);
    expect(reset).toHaveBeenCalledWith({ built: ['row'], conversation, id: 7 });
  });

  it('leaves the thread alone while a reply is being written', async () => {
    running = true;

    await keeping()!.onKept();

    expect(transport.getThread).not.toHaveBeenCalled();
    expect(reset).not.toHaveBeenCalled();
  });
});
