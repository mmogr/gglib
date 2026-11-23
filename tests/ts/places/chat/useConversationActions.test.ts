/**
 * What changes a conversation acts on this machine's daemon, so on a far
 * chat, whose ids are the far machine's, none of it reaches the transport.
 * An export saves the file the daemon writes, its images in it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ConversationSummary } from '../../../../src/services/transport';
import type { ChatExport } from '../../../../src/types/generated/ChatExport';
import type { ExportedMessage } from '../../../../src/types/generated/ExportedMessage';
import type { ModelRef } from '../../../../src/types/generated/ModelRef';

const transport = vi.hoisted(() => ({
  deleteConversation: vi.fn(async () => {}),
  createConversation: vi.fn(async () => 5),
  updateConversationTitle: vi.fn(async () => {}),
  updateConversationSystemPrompt: vi.fn(async () => {}),
  exportConversation: vi.fn(async (_id: number): Promise<string> => {
    throw new Error('no file was set for this test');
  }),
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

function actions(far: boolean, model?: ModelRef) {
  const confirm = vi.fn(async () => true);
  const onOpened = vi.fn();
  const onDeleted = vi.fn();
  const onError = vi.fn();
  const { result } = renderHook(() =>
    useConversationActions({ far, activeConversation: open, confirm, onError, onOpened, onDeleted, model }),
  );
  return { ...result.current, confirm, onOpened, onDeleted, onError };
}

/** Every transport call made, by name. */
function called(): string[] {
  return Object.entries(transport)
    .filter(([, fn]) => fn.mock.calls.length > 0)
    .map(([name]) => name);
}

/** A stored image, by the id an upload answers. */
const SHOT = { id: 'a'.repeat(64), mime: 'image/png', width: 2560, height: 1440 };

function row(id: number, role: ExportedMessage['role']): ExportedMessage {
  return { id, conversation_id: 1, role, content: `row ${id}`, created_at: '2026-09-30 09:12:30' };
}

/** Export this machine's chat, and read back the file the page was handed. */
async function exported(here: ReturnType<typeof actions>) {
  const made = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => 'blob:export');
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  const clicked = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
  await here.exportChat();
  expect(clicked).toHaveBeenCalledTimes(1);
  // jsdom's Blob has no `text()`.
  const text = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(made.mock.calls[0][0] as Blob);
  });
  return JSON.parse(text) as ChatExport;
}

beforeEach(() => {
  Object.values(transport).forEach((fn) => fn.mockClear());
});

afterEach(() => {
  vi.restoreAllMocks();
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

  it("Start over on a chat with this machine's model makes the copy for that model, so its next turn runs there", async () => {
    const here: ModelRef = { machine: { kind: 'local' }, id: 7 };
    await actions(false, here).startOver();
    expect(transport.createConversation).toHaveBeenCalledWith(expect.objectContaining({ model: here }));
  });

  it('Export saves the file the daemon writes, as it comes, and says nothing when every image is in it', async () => {
    const file: ChatExport = {
      conversation: open,
      messages: [{ ...row(1, 'user'), images: [{ ...SHOT, data_url: 'data:image/png;base64,cG5n' }] }, row(2, 'assistant')],
    };
    transport.exportConversation.mockResolvedValueOnce(JSON.stringify(file, null, 2));
    const here = actions(false);

    const saved = await exported(here);

    expect(transport.exportConversation).toHaveBeenCalledWith(1);
    expect(saved).toEqual(file);
    expect(here.onError).not.toHaveBeenCalled();
  });
});
