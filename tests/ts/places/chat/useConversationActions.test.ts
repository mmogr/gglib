/**
 * What changes a conversation acts on this machine's daemon, so on a far
 * chat, whose ids are the far machine's, none of it reaches the transport.
 * An export carries the chat's images in the file, each read from the store
 * once.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ChatMessage, ConversationSummary } from '../../../../src/services/transport';
import type { ModelRef } from '../../../../src/types/generated/ModelRef';

const transport = vi.hoisted(() => ({
  deleteConversation: vi.fn(async () => {}),
  createConversation: vi.fn(async () => 5),
  updateConversationTitle: vi.fn(async () => {}),
  updateConversationSystemPrompt: vi.fn(async () => {}),
  getThread: vi.fn(async (): Promise<{ messages: unknown[] }> => ({ messages: [] })),
  fetchAttachmentBlob: vi.fn(async (_source: string, _id: string): Promise<Blob> => new Blob([])),
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

/** Two stored images, by the ids an upload answers. */
const SHOT = { id: 'a'.repeat(64), mime: 'image/png', width: 2560, height: 1440 };
const DRAWN = { id: 'b'.repeat(64), mime: 'image/jpeg', width: 512, height: 512 };

function row(id: number, role: ChatMessage['role'], images?: (typeof SHOT)[]): ChatMessage {
  return { id, conversation_id: 1, role, content: `row ${id}`, created_at: '2026-09-30 09:12:30', ...(images && { images }) };
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
  return JSON.parse(text) as { conversation: unknown; messages: ChatMessage[] };
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

  it('Export writes each image into the file as a data: URL beside its info, read from the store once', async () => {
    // A user's attachment, a reply with none, and a tool's row with the same image and one of its own.
    transport.getThread.mockResolvedValueOnce({
      messages: [row(1, 'user', [SHOT]), row(2, 'assistant'), row(3, 'tool', [SHOT, DRAWN])],
    });
    transport.fetchAttachmentBlob.mockImplementation(
      async (_source, id) => new Blob([id === SHOT.id ? 'png' : 'jpeg'], { type: 'application/octet-stream' }),
    );
    const here = actions(false);

    const file = await exported(here);

    expect(transport.fetchAttachmentBlob.mock.calls).toEqual([
      ['this', SHOT.id],
      ['this', DRAWN.id],
    ]);
    expect(file.conversation).toEqual(open);
    // "png" and "jpeg" in base64, under the type the store recorded for each.
    expect(file.messages[0].images).toEqual([{ ...SHOT, data_url: 'data:image/png;base64,cG5n' }]);
    expect(file.messages[1]).toEqual(row(2, 'assistant'));
    expect(file.messages[2].images).toEqual([
      { ...SHOT, data_url: 'data:image/png;base64,cG5n' },
      { ...DRAWN, data_url: 'data:image/jpeg;base64,anBlZw==' },
    ]);
    expect(here.onError).not.toHaveBeenCalled();
  });

  it('an image the store cannot give stays in the file by its id, the file is still handed over, and the page says so', async () => {
    transport.getThread.mockResolvedValueOnce({ messages: [row(1, 'user', [SHOT, DRAWN]), row(2, 'tool', [DRAWN])] });
    transport.fetchAttachmentBlob.mockImplementation(async (_source, id) => {
      if (id === DRAWN.id) throw new Error('No stored image has that id.');
      return new Blob(['png']);
    });
    const here = actions(false);

    const file = await exported(here);

    expect(file.messages[0].images).toEqual([{ ...SHOT, data_url: 'data:image/png;base64,cG5n' }, DRAWN]);
    expect(file.messages[1].images).toEqual([DRAWN]);
    expect(transport.fetchAttachmentBlob).toHaveBeenCalledTimes(2);
    expect(here.onError).toHaveBeenCalledWith('1 image could not be read. It is in the file by its id only.');
  });

  it('a chat with no image exports as its saved rows, and reads no image', async () => {
    transport.getThread.mockResolvedValueOnce({ messages: [row(1, 'user'), row(2, 'assistant')] });
    const here = actions(false);

    const file = await exported(here);

    expect(file).toEqual({ conversation: open, messages: [row(1, 'user'), row(2, 'assistant')] });
    expect(transport.fetchAttachmentBlob).not.toHaveBeenCalled();
    expect(here.onError).not.toHaveBeenCalled();
  });
});
