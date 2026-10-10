/**
 * A turn's images, through the real composer and transport against the
 * fake daemons: uploaded to the chat's store when added, named by id in the
 * run (every user message of the history again), kept by a regenerate and
 * an edit, shown again on reopening, and handed back with the text when a
 * send does not go. A far chat's go to the far machine's store, and an image
 * drafted for one store is never sent to the other by its composer id, but
 * is sent at once when the chat comes back to its store.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, waitFor } from '@testing-library/react';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ getEnabledDefinitions: () => [], getBackendName: (n: string) => n }),
}));

import { FakeDaemon } from '../../fixtures/fakeDaemon';
import { FakeFarDaemon } from '../../fixtures/fakeFarDaemon';
import { bytesOf, png, pngFile } from '../../fixtures/fakeImageStore';
import { conversation, mount, regenerate, textOf } from './runtimeHarness';
import { resetRemoteState } from '../../../../src/services/remoteRegistry';
import { FAR_CANNOT_TAKE_IMAGES, NOT_IN_THIS_STORE } from '../../../../src/hooks/useGglibRuntime/imageRefusals';
import type { SentImage } from '../../../../src/hooks/useGglibRuntime/imageAttachments';
import type { UseGglibRuntimeOptions } from '../../../../src/hooks/useGglibRuntime/useGglibRuntime';

type Hook = Awaited<ReturnType<typeof mount>>;

let daemon: FakeDaemon;
const onError = vi.fn();
const onImageRefused = vi.fn();

beforeEach(() => {
  daemon = new FakeDaemon();
  vi.stubGlobal('fetch', vi.fn(daemon.fetch));
  resetRemoteState();
  onError.mockClear();
  onImageRefused.mockClear();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

const open: UseGglibRuntimeOptions = {
  conversationId: 1,
  conversation: conversation(1),
  selectedServerPort: 9000,
  onError,
  onImageRefused,
};

const composer = (hook: Hook) => hook.result.current.runtime.thread.composer;

/** Attach `file` as a paste or a drop does, and wait for its upload to end. */
async function attach(hook: Hook, file: File): Promise<void> {
  await act(async () => {
    await composer(hook).addAttachment(file);
  });
}

async function sendDraft(hook: Hook, text: string): Promise<void> {
  await act(async () => {
    composer(hook).setText(text);
    await composer(hook).send();
  });
}

/** An image stored in `daemon`'s store, by its id. */
async function stored(store: FakeDaemon, fill: number): Promise<string> {
  const response = await store.images.upload(new Blob([png(64, 64, fill)], { type: 'image/png' }));
  return ((await response.json()) as { id: string }).id;
}

/** The composer's draft: its text, and how each image stands. */
function draft(hook: Hook) {
  const state = composer(hook).getState();
  return { text: state.text, images: state.attachments.map((a) => a.status.type) };
}

describe('useGglibRuntime images', () => {
  it('uploads an image when it is added, and the run names it and the history\'s by id', async () => {
    const old = await stored(daemon, 1);
    daemon.save(1, { role: 'user', content: 'this one', images: daemon.images.infos([old]) });
    daemon.save(1, { role: 'assistant', content: 'seen' });
    const hook = await mount(open);
    const file = pngFile(800, 600);

    await attach(hook, file);
    expect(daemon.count('POST', '/api/attachments')).toBe(1);
    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    const uploaded = daemon.requests.find((r) => r.url === '/api/attachments')!;
    expect(await bytesOf(uploaded.body as Blob)).toEqual(await bytesOf(file));

    await sendDraft(hook, 'and this?');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const [id] = [...daemon.images.stored.keys()].filter((k) => k !== old);
    expect(daemon.only().request!.messages).toEqual([
      { role: 'system', content: 'You are a helpful assistant.' },
      { role: 'user', content: 'this one', images: [old] },
      { role: 'assistant', content: 'seen' },
      { role: 'user', content: 'and this?', images: [id] },
    ]);
    // The send uploaded nothing more.
    expect(daemon.count('POST', '/api/attachments')).toBe(1);
    const asked = hook.result.current.messages.findLast((m) => m.role === 'user')!;
    expect(asked.attachments?.map((a) => a.id)).toEqual([id]);
    expect(daemon.saved(1).at(-1)!.images?.map((i) => i.id)).toEqual([id]);
  });

  it('an image alone is a turn', async () => {
    const hook = await mount(open);
    await attach(hook, pngFile(800, 600));

    await sendDraft(hook, '');

    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    const last = daemon.only().request!.messages.at(-1)!;
    expect(last).toMatchObject({ role: 'user', content: '' });
    expect(last.role === 'user' && last.images).toHaveLength(1);
  });

  it('a send refused by code gives back the text and the images, uploading nothing again', async () => {
    const hook = await mount(open);
    await attach(hook, pngFile(800, 600));
    daemon.refuseNext = { status: 400, type: 'model_cannot_read_images', error: 'qwen cannot read images' };

    await sendDraft(hook, 'what is this?');

    await waitFor(() => expect(draft(hook)).toEqual({ text: 'what is this?', images: ['requires-action'] }));
    expect(onError.mock.calls.at(-1)![0].message).toMatch(/^This model cannot read images/);
    expect(daemon.count('POST', '/api/attachments')).toBe(1);
  });

  it('images over 16 MiB together are refused by name, and given back', async () => {
    const hook = await mount(open);
    await attach(hook, pngFile(800, 600));
    daemon.refuseNext = { status: 400, type: 'request_images_too_large', error: 'over 16 MiB' };

    await sendDraft(hook, 'and these?');

    await waitFor(() => expect(draft(hook).images).toEqual(['requires-action']));
    expect(onError.mock.calls.at(-1)![0].message).toMatch(/over the 16 MiB one request may carry/);
  });

  it('an image the store no longer holds is uploaded again when it is given back', async () => {
    const hook = await mount(open);
    await attach(hook, pngFile(800, 600));
    daemon.images.stored.clear();

    await sendDraft(hook, 'look');

    await waitFor(() => expect(daemon.count('POST', '/api/attachments')).toBe(2));
    await waitFor(() => expect(draft(hook)).toEqual({ text: 'look', images: ['requires-action'] }));
    expect(daemon.images.stored.size).toBe(1);
    expect(daemon.count('PUT', '/api/runs/')).toBe(1);
    expect(daemon.runs.size).toBe(0);
  });

  it('a failed upload is said at once; its send sends nothing and gives the draft back', async () => {
    const hook = await mount(open);
    daemon.images.refuseNextUpload = { status: 413, type: 'image_too_large', error: 'too large' };

    await attach(hook, pngFile(800, 600));
    expect(onImageRefused).toHaveBeenCalledWith('An image is over the 8 MiB one image may be. Send a smaller one.');
    expect(draft(hook).images).toEqual(['incomplete']);

    await sendDraft(hook, 'look');

    expect(daemon.count('PUT', '/api/runs/')).toBe(0);
    expect(onError.mock.calls.at(-1)![0].message).toBe('An image is over the 8 MiB one image may be. Send a smaller one.');
    // Back with the text, and added again: the store takes it this time.
    await waitFor(() => expect(draft(hook)).toEqual({ text: 'look', images: ['requires-action'] }));
  });

  it('a regenerate and an edit keep the turn\'s images', async () => {
    const id = await stored(daemon, 2);
    daemon.save(1, { role: 'user', content: 'what is this?', images: daemon.images.infos([id]) });
    daemon.save(1, { role: 'assistant', content: 'a cat' });
    const hook = await mount(open);
    const imagesIn = (cid: number) => daemon.saved(cid).map((r) => [r.content, (r.images ?? []).map((i) => i.id)]);

    regenerate(hook, 'db-1');
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(1));
    expect(imagesIn(100)).toEqual([['what is this?', [id]]]);
    await waitFor(() => expect(hook.result.current.isRunning).toBe(false));

    const asked = hook.result.current.messages.find((m) => m.role === 'user')!;
    act(() => {
      void hook.result.current.runtime.thread.append({
        parentId: 'system-1',
        role: 'user',
        content: [{ type: 'text', text: 'and the breed?' }],
        attachments: asked.attachments,
      });
    });
    await waitFor(() => expect(daemon.count('PUT', '/api/runs/')).toBe(2));
    const change = daemon.requests.filter((r) => r.url.endsWith('/changes')).at(-1)!;
    expect(change.body).toEqual({ kind: 'edit', message_id: 1, content: 'and the breed?', images: [id] });
    expect(imagesIn(101)).toEqual([['and the breed?', [id]]]);
  });

  it('reopening a chat shows its saved images, by id with their facts', async () => {
    const id = await stored(daemon, 3);
    daemon.save(1, { role: 'user', content: '', images: daemon.images.infos([id]) });

    const hook = await mount(open);

    const [image] = hook.result.current.messages[1].attachments as SentImage[];
    expect(image).toMatchObject({ id, type: 'image', status: { type: 'complete' }, contentType: 'image/png' });
    expect(image.stored).toEqual({ id, mime: 'image/png', width: 64, height: 64 });
    expect(image.file).toBeUndefined();
    expect(textOf(hook.result.current.messages[1])).toBe('');
  });
});

describe('useGglibRuntime images on a far chat', () => {
  let daemons: FakeFarDaemon;
  beforeEach(() => {
    daemons = new FakeFarDaemon();
    vi.stubGlobal('fetch', vi.fn(daemons.fetch));
  });
  const far = { conversationId: 1, source: 'far' as const, onError, onImageRefused };

  it('uploads to the far machine\'s store, and the far turn names the image by its id there', async () => {
    const hook = await mount(far);

    await attach(hook, pngFile(800, 600));
    await sendDraft(hook, 'what is this?');

    await waitFor(() => expect(daemons.farCount('PUT', '/api/remote/chats/1/turns/')).toBe(1));
    expect(daemons.farCount('POST', '/api/remote/attachments')).toBe(1);
    const [id] = daemons.hub.images.stored.keys();
    expect(daemons.farRequests.find((r) => r.method === 'PUT')!.body).toEqual({ content: 'what is this?', images: [id] });
    expect(daemons.hub.saved(1).at(-1)!.images?.map((i) => i.id)).toEqual([id]);
    expect(daemons.here.requests).toEqual([]);
  });

  it('a far gglib from before images is named as that, at the upload and at the turn', async () => {
    const hook = await mount(far);
    await attach(hook, pngFile(800, 600));
    daemons.noImages = true;

    await sendDraft(hook, 'what is this?');
    await waitFor(() => expect(onError).toHaveBeenCalledWith(new Error(FAR_CANNOT_TAKE_IMAGES)));
    await waitFor(() => expect(draft(hook).text).toBe('what is this?'));

    await attach(hook, pngFile(640, 480));
    expect(onImageRefused).toHaveBeenCalledWith(FAR_CANNOT_TAKE_IMAGES);
  });

  /** The ids of the composer's images: each a composer id until it is sent. */
  const drafted = (hook: Hook) => composer(hook).getState().attachments.map((a) => a.id);

  it('an image drafted on this machine\'s chat is never sent to the far one by its composer id: it comes back, uploaded there', async () => {
    const hook = await mount(open);
    await attach(hook, pngFile(800, 600));
    expect(daemons.here.count('POST', '/api/attachments')).toBe(1);
    hook.rerender(far);
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    expect(draft(hook)).toEqual({ text: '', images: ['requires-action'] });

    await sendDraft(hook, 'look');

    await waitFor(() => expect(draft(hook)).toEqual({ text: 'look', images: ['requires-action'] }));
    expect(daemons.farCount('PUT', '/api/remote/chats/1/turns/')).toBe(0);
    expect(daemons.here.count('PUT', '/api/runs/')).toBe(0);
    expect(onError).toHaveBeenCalledWith(new Error(NOT_IN_THIS_STORE));
    expect(daemons.farCount('POST', '/api/remote/attachments')).toBe(1);

    await sendDraft(hook, 'look');
    await waitFor(() => expect(daemons.farCount('PUT', '/api/remote/chats/1/turns/')).toBe(1));
    const [id] = daemons.hub.images.stored.keys();
    expect(daemons.farRequests.find((r) => r.method === 'PUT')!.body).toEqual({ content: 'look', images: [id] });
  });

  it('an image drafted on a far chat is never sent to this machine\'s by its composer id: it comes back, uploaded here', async () => {
    const hook = await mount(far);
    await attach(hook, pngFile(800, 600));
    expect(daemons.farCount('POST', '/api/remote/attachments')).toBe(1);
    hook.rerender(open);
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    expect(drafted(hook)).toHaveLength(1);

    await sendDraft(hook, 'look');

    await waitFor(() => expect(draft(hook)).toEqual({ text: 'look', images: ['requires-action'] }));
    expect(daemons.here.count('PUT', '/api/runs/')).toBe(0);
    expect(onError).toHaveBeenCalledWith(new Error(NOT_IN_THIS_STORE));
    expect(daemons.here.count('POST', '/api/attachments')).toBe(1);

    await sendDraft(hook, 'look');
    await waitFor(() => expect(daemons.here.count('PUT', '/api/runs/')).toBe(1));
    const [id] = daemons.here.images.stored.keys();
    expect(daemons.here.only().request!.messages.at(-1)).toEqual({ role: 'user', content: 'look', images: [id] });
    expect(drafted(hook)).toEqual([]);
  });

  it('an image drafted here, the chat list gone to the far machine and back, is sent by this store\'s id at once', async () => {
    const hook = await mount(open);
    await attach(hook, pngFile(800, 600));
    hook.rerender(far);
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    hook.rerender(open);
    await waitFor(() => expect(hook.result.current.isLoading).toBe(false));
    expect(draft(hook)).toEqual({ text: '', images: ['requires-action'] });

    await sendDraft(hook, 'look');

    await waitFor(() => expect(daemons.here.count('PUT', '/api/runs/')).toBe(1));
    const [id] = daemons.here.images.stored.keys();
    expect(daemons.here.only().request!.messages.at(-1)).toEqual({ role: 'user', content: 'look', images: [id] });
    expect(daemons.here.count('POST', '/api/attachments')).toBe(1);
    expect(onError).not.toHaveBeenCalled();
    expect(draft(hook)).toEqual({ text: '', images: [] });
  });
});
