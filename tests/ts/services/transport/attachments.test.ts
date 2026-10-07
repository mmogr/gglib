/**
 * The image stores as the page reaches them: an upload is the image's raw
 * bytes, posted with the page's credential to this machine's store or the
 * far machine's, and a read is the bytes back with that credential. The
 * transport object has both, under names no other module spreads over.
 *
 * Also the title of a chat whose questions are images alone, which the
 * model is not asked for: it is sent the text alone.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { createApiTransport } from '../../../../src/services/transport/api';
import * as attachments from '../../../../src/services/transport/api/attachments';
import { fetchAttachmentBlob, uploadAttachment } from '../../../../src/services/transport/api/attachments';
import { generateChatTitle } from '../../../../src/services/transport/api/chat';
import { setApiSession } from '../../../../src/services/transport/api/client';
import { TransportError } from '../../../../src/services/transport/errors';
import { FakeFarDaemon } from '../../fixtures/fakeFarDaemon';
import { bytesOf, pngFile } from '../../fixtures/fakeImageStore';

let daemons: FakeFarDaemon;
let fetchSpy: ReturnType<typeof vi.fn>;

beforeEach(() => {
  daemons = new FakeFarDaemon();
  fetchSpy = vi.fn(daemons.fetch);
  vi.stubGlobal('fetch', fetchSpy);
  setApiSession('', 'page-key');
});
afterEach(() => {
  vi.unstubAllGlobals();
  setApiSession('', undefined);
});

describe('image stores', () => {
  it('uploads this machine\'s image as its raw bytes, with the page\'s credential, and reads it back', async () => {
    const file = pngFile(800, 600);

    const stored = await uploadAttachment('this', file);

    expect(stored).toMatchObject({ mime: 'image/png', width: 800, height: 600, image_tokens: 475 });
    const [url, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/attachments');
    expect(init.method).toBe('POST');
    expect(init.body).toBe(file);
    expect(init.headers).toMatchObject({ Authorization: 'Bearer page-key', 'Content-Type': 'image/png' });

    const blob = await fetchAttachmentBlob('this', stored.id);
    expect(await bytesOf(blob)).toEqual(await bytesOf(file));
    const [readUrl, readInit] = fetchSpy.mock.calls[1] as [string, RequestInit];
    expect(readUrl).toBe(`/api/attachments/${stored.id}`);
    expect(readInit.headers).toMatchObject({ Authorization: 'Bearer page-key' });
  });

  it('a far chat\'s image goes to the far machine\'s store, and nothing to this one\'s', async () => {
    const stored = await uploadAttachment('far', pngFile(800, 600));
    await fetchAttachmentBlob('far', stored.id);

    expect(daemons.farRequests.map((r) => `${r.method} ${r.url}`)).toEqual([
      'POST /api/remote/attachments',
      `GET /api/remote/attachments/${stored.id}`,
    ]);
    expect(daemons.hub.images.stored.has(stored.id)).toBe(true);
    expect(daemons.here.images.stored.size).toBe(0);
  });

  it('a refused upload or read is the store\'s coded error', async () => {
    daemons.here.images.refuseNextUpload = { status: 413, type: 'image_too_large', error: 'too large' };

    const upload = uploadAttachment('this', pngFile(800, 600));
    await expect(upload).rejects.toBeInstanceOf(TransportError);
    await expect(upload).rejects.toMatchObject({ details: { status: 413, type: 'image_too_large' } });
    await expect(fetchAttachmentBlob('this', '0'.repeat(64))).rejects.toMatchObject({
      code: 'NOT_FOUND',
      details: { type: 'attachment_not_found' },
    });
  });

  it('the transport carries both, and no other module spreads over them', () => {
    const transport = createApiTransport();
    expect(transport.uploadAttachment).toBe(attachments.uploadAttachment);
    expect(transport.fetchAttachmentBlob).toBe(attachments.fetchAttachmentBlob);
  });
});

describe('generateChatTitle', () => {
  const row = (role: 'user' | 'assistant', content: string, images = 0) => ({
    id: 1, conversation_id: 1, role, content, created_at: '2026-10-04T00:00:00Z',
    ...(images > 0 && { images: Array.from({ length: images }, (_, i) => ({ id: `${i}`, mime: 'image/png', width: 1, height: 1 })) }),
  });

  it('titles a chat whose questions are images alone without asking the model', async () => {
    const title = await generateChatTitle({ serverPort: 9000, messages: [row('user', '', 1), row('assistant', 'A cat.')] });

    expect(title).toBe('Image chat');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('asks the model when a question has text, images or not', async () => {
    fetchSpy.mockResolvedValueOnce(new Response(JSON.stringify('Cat breeds')));

    const title = await generateChatTitle({ serverPort: 9000, messages: [row('user', 'what breed?', 1), row('assistant', 'A tabby.')] });

    expect(title).toBe('Cat breeds');
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
});
