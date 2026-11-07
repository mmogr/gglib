/**
 * What changes a conversation acts on this machine's daemon, so on a far
 * chat, whose ids are the far machine's, none of it reaches the transport.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ConversationSummary } from '../../../../src/services/transport';
import type { ModelRef } from '../../../../src/types/generated/ModelRef';

const transport = vi.hoisted(() => ({
  deleteConversation: vi.fn(async () => {}),
  createConversation: vi.fn(async () => 5),
  updateConversationTitle: vi.fn(async () => {}),
  updateConversationSystemPrompt: vi.fn(async () => {}),
  getThread: vi.fn(async () => ({ messages: [] })),
  listConversations: vi.fn(async () => []),
}));
vi.mock('../../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

import { useConversationActions } from '../../../../src/places/chat/useConversationActions';

const open: ConversationSummary = {
  id: 1,
  title: 'Why the build broke',
  model_id: null,
  system_prompt: null,
  created_at: '2026-09-30 09:12:30',
  updated_at: '2026-09-30 09:13:07',
};

function actions(far: boolean, pairedModel?: ModelRef) {
  const confirm = vi.fn(async () => true);
  const onOpened = vi.fn();
  const onDeleted = vi.fn();
  const onError = vi.fn();
  const { result } = renderHook(() =>
    useConversationActions({ far, activeConversation: open, confirm, onError, onOpened, onDeleted, pairedModel }),
  );
  return { ...result.current, confirm, onOpened, onDeleted, onError };
}

/** Every transport call made, by name. */
function called(): string[] {
  return Object.entries(transport)
    .filter(([, fn]) => fn.mock.calls.length > 0)
    .map(([name]) => name);
}

beforeEach(() => {
  Object.values(transport).forEach((fn) => fn.mockClear());
});

describe('useConversationActions', () => {
  it('on a far chat, delete, rename, restart, export and the prompt reach nothing', async () => {
    const far = actions(true);

    await far.remove();
    await far.rename('renamed');
    await far.startOver();
    await far.exportChat();
    await far.updateSystemPrompt('be brief');

    expect(called()).toEqual([]);
    expect(far.confirm).not.toHaveBeenCalled();
    expect(far.onOpened).not.toHaveBeenCalled();
    expect(far.onDeleted).not.toHaveBeenCalled();
  });

  it('on this machine’s chat they reach its daemon', async () => {
    const here = actions(false);

    await here.remove();
    expect(transport.deleteConversation).toHaveBeenCalledWith(1);
    expect(here.onDeleted).toHaveBeenCalledTimes(1);

    await here.rename('renamed');
    expect(transport.updateConversationTitle).toHaveBeenCalledWith(1, 'renamed');

    // Start over: the chat goes, a fresh copy with its title and prompt opens.
    await here.startOver();
    expect(transport.createConversation).toHaveBeenCalledWith(expect.objectContaining({ title: 'Why the build broke' }));
    expect(here.onOpened).toHaveBeenCalledWith(5);

    await here.updateSystemPrompt('be brief');
    expect(transport.updateConversationSystemPrompt).toHaveBeenCalledWith(1, 'be brief');
  });

  it("Start over on a chat with the paired machine's model makes the copy for that model too", async () => {
    const far: ModelRef = { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 };
    await actions(false, far).startOver();
    expect(transport.createConversation).toHaveBeenCalledWith(expect.objectContaining({ model: far }));
  });
});
