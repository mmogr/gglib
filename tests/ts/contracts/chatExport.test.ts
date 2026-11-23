/**
 * The chat page's Export saves the file `gglib chat export` writes.
 *
 * `contracts/chats/export.json` is written by `gglib-core`'s chat-export
 * tests: a chat with an image the store gave and one it could not, and a
 * reply's figure written as serde writes a float, the file's text as the
 * daemon serves it (`GET /api/conversations/{id}/export`) and the CLI writes
 * it, the name it is saved by, and the sentence said of the unread image.
 * Served that text, the page saves it as it comes, by the same name, and
 * says the same sentence: a round trip through `JSON.stringify` would write
 * the figure `2.0` as `2`.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { ChatExport } from '../../../src/types/generated/ChatExport';
import { rust } from './rustSource';

const CONTRACT = JSON.parse(rust('contracts/chats/export.json')) as { name: string; text: string; unread: string | null };
const SERVED = JSON.parse(CONTRACT.text) as ChatExport;

const transport = vi.hoisted(() => ({
  exportConversation: vi.fn(async (_id: number): Promise<string> => {
    throw new Error('no file was set for this test');
  }),
}));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

import { useConversationActions } from '../../../src/places/chat/useConversationActions';

afterEach(() => {
  vi.restoreAllMocks();
});

/** Read a Blob's text: jsdom's Blob has no `text()`. */
function textOf(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsText(blob);
  });
}

describe('chat export contract', () => {
  it('records a chat with an image the store gave and one it could not', () => {
    const images = SERVED.messages.flatMap((message) => message.images ?? []);
    expect(images.some((image) => image.data_url?.startsWith(`data:${image.mime};base64,`))).toBe(true);
    expect(images.some((image) => image.data_url === undefined)).toBe(true);
    expect(CONTRACT.unread).not.toBeNull();
    // A whole-number float, which only a verbatim save keeps as written.
    expect(CONTRACT.text).toContain('"thinkingDurationSeconds": 2.0');
  });

  it('the page saves the text the CLI writes, by the same name, and says the same of the unread image', async () => {
    transport.exportConversation.mockResolvedValueOnce(CONTRACT.text);
    const made = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => 'blob:export');
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    let name = '';
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      name = this.download;
    });
    const onError = vi.fn();
    const { result } = renderHook(() =>
      useConversationActions({
        far: false,
        activeConversation: SERVED.conversation,
        confirm: vi.fn(async () => true),
        onError,
        onOpened: vi.fn(),
      }),
    );

    await result.current.exportChat();

    expect(transport.exportConversation).toHaveBeenCalledWith(SERVED.conversation.id);
    expect(await textOf(made.mock.calls[0][0] as Blob)).toBe(CONTRACT.text);
    expect(name).toBe(CONTRACT.name);
    expect(onError.mock.calls).toEqual([[CONTRACT.unread]]);
  });
});
