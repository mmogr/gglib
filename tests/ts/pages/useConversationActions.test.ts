/**
 * What changes a conversation acts on this machine's daemon, so on a far
 * chat, whose ids are the far machine's, none of it reaches the transport.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ConversationSummary } from '../../../src/services/transport';

const transport = vi.hoisted(() => ({
  deleteConversation: vi.fn(async () => {}),
  createConversation: vi.fn(async () => 5),
  updateConversationTitle: vi.fn(async () => {}),
  updateConversationSystemPrompt: vi.fn(async () => {}),
  getMessages: vi.fn(async () => []),
}));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

import { useConversationActions } from '../../../src/pages/useConversationActions';

const open: ConversationSummary = {
  id: 1,
  title: 'Why the build broke',
  model_id: null,
  system_prompt: null,
  settings: null,
  created_at: '2026-09-30 09:12:30',
  updated_at: '2026-09-30 09:13:07',
};

function actions(far: boolean) {
  const confirm = vi.fn(async () => true);
  const syncConversations = vi.fn(async () => {});
  const onError = vi.fn();
  const { result } = renderHook(() =>
    useConversationActions({ far, activeConversation: open, confirm, syncConversations, onError }),
  );
  return { ...result.current, confirm, syncConversations, onError };
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

    await far.handleDeleteConversation(1);
    await far.handleRenameConversation('renamed');
    await far.handleClearConversation();
    await far.handleExportConversation();
    await far.handleUpdateSystemPrompt('be brief');

    expect(called()).toEqual([]);
    expect(far.confirm).not.toHaveBeenCalled();
    expect(far.syncConversations).not.toHaveBeenCalled();
  });

  it('on this machine’s chat they reach its daemon', async () => {
    const here = actions(false);

    await here.handleDeleteConversation(1);
    await here.handleRenameConversation('renamed');
    await here.handleClearConversation();
    await here.handleUpdateSystemPrompt('be brief');

    expect(transport.deleteConversation).toHaveBeenCalledWith(1);
    expect(transport.updateConversationTitle).toHaveBeenCalledWith(1, 'renamed');
    expect(transport.createConversation).toHaveBeenCalledTimes(1);
    expect(transport.updateConversationSystemPrompt).toHaveBeenCalledWith(1, 'be brief');
  });
});
