/**
 * The composer's image adapter, without a page: an image is uploaded when
 * it is added and never when it is sent, a failed upload is said at once
 * and never throws out of a send (which would lose the draft), a file
 * already uploaded, or still uploading, is not uploaded again, and an image
 * is sent as stored only by an upload to this adapter's store.
 */

import { describe, it, expect, vi } from 'vitest';
import type { CompleteAttachment, PendingAttachment } from '@assistant-ui/react';
import {
  createImageAttachmentAdapter,
  type ImageAttachmentOptions,
  type ImageAttachment,
  type SentImage,
} from '../../../../src/hooks/useGglibRuntime/imageAttachments';
import { FAR_CANNOT_TAKE_IMAGES, NOT_IN_THIS_STORE } from '../../../../src/hooks/useGglibRuntime/imageRefusals';
import { MAX_IMAGE_EDGE, type Downscale } from '../../../../src/hooks/useGglibRuntime/imagePrep';
import { TransportError } from '../../../../src/services/transport/errors';
import type { AttachmentUpload } from '../../../../src/types/generated/AttachmentUpload';
import { pngFile } from '../../fixtures/fakeImageStore';

const ID = 'a'.repeat(64);

function answer(id = ID): AttachmentUpload {
  return { id, mime: 'image/png', width: 640, height: 480, image_tokens: 300 };
}

function adapter(overrides: Partial<ImageAttachmentOptions> = {}) {
  const upload = vi.fn(async (_image: File) => answer());
  const onRefused = vi.fn();
  const downscale = vi.fn<Downscale>(async (image) => image);
  const options = { source: 'this' as const, upload, onRefused, downscale, ...overrides };
  return { images: createImageAttachmentAdapter(options), ...options };
}

/** Every state `add` yields for `file`, in order. */
async function added(images: ReturnType<typeof adapter>['images'], file: File): Promise<PendingAttachment[]> {
  const states: PendingAttachment[] = [];
  const steps = images.add({ file }) as AsyncGenerator<PendingAttachment, void>;
  for await (const state of steps) states.push(state);
  return states;
}

describe('the image adapter', () => {
  it('uploads in add: shown at once as uploading, then ready with what the store answered', async () => {
    const { images, upload } = adapter();
    const file = pngFile(640, 480);

    const states = await added(images, file);

    expect(states.map((s) => s.status.type)).toEqual(['running', 'requires-action']);
    expect(states[0].id).toBe(states[1].id);
    expect(upload).toHaveBeenCalledTimes(1);
    expect(upload).toHaveBeenCalledWith(file);
    expect((states[1] as ImageAttachment).stored).toEqual(answer());
  });

  it('send never uploads: it turns the attachment into the stored image by its id', async () => {
    const { images, upload } = adapter();
    const [, ready] = await added(images, pngFile(640, 480));
    expect(upload).toHaveBeenCalledTimes(1);

    const sent = (await images.send(ready)) as SentImage;

    expect(upload).toHaveBeenCalledTimes(1);
    expect(sent).toMatchObject({ id: ID, status: { type: 'complete' }, content: [], stored: answer() });
  });

  it('a send while the upload is still going waits for it, and the add then yields nothing more', async () => {
    let finish!: (upload: AttachmentUpload) => void;
    const upload = vi.fn(() => new Promise<AttachmentUpload>((resolve) => (finish = resolve)));
    const { images } = adapter({ upload });
    const steps = images.add({ file: pngFile(640, 480) }) as AsyncGenerator<PendingAttachment, void>;
    const running = (await steps.next()).value as PendingAttachment;
    await vi.waitFor(() => expect(upload).toHaveBeenCalledTimes(1));

    const sending = images.send(running);
    const rest = steps.next();
    finish(answer());

    expect(await sending).toMatchObject({ id: ID, status: { type: 'complete' } });
    expect(await rest).toEqual({ done: true, value: undefined });
    expect(upload).toHaveBeenCalledTimes(1);
  });

  it('a failed upload is said at once, stays marked, and its send hands back no stored image rather than throw', async () => {
    const upload = vi.fn(async () => {
      throw new TransportError('VALIDATION', 'too big', { status: 413, type: 'image_too_large' });
    });
    const { images, onRefused } = adapter({ upload });

    const states = await added(images, pngFile(640, 480));

    expect(states.map((s) => s.status.type)).toEqual(['running', 'incomplete']);
    expect(onRefused).toHaveBeenCalledWith('An image is over the 8 MiB one image may be. Send a smaller one.');
    const sent = (await images.send(states[1])) as SentImage;
    expect(sent.stored).toBeUndefined();
    expect(sent.refused).toBe('An image is over the 8 MiB one image may be. Send a smaller one.');
  });

  it('a file that is not a PNG or a JPEG is refused by name, never shown and never uploaded', async () => {
    const { images, upload, onRefused } = adapter();

    const states = await added(images, new File(['%PDF-1.7'], 'notes.pdf', { type: 'application/pdf' }));

    expect(onRefused).toHaveBeenCalledWith('Only PNG and JPEG images can be sent.');
    expect(states).toEqual([]);
    expect(upload).not.toHaveBeenCalled();
  });

  it('an image removed while it uploads is not put back when its upload ends', async () => {
    let finish!: (upload: AttachmentUpload) => void;
    const upload = vi.fn(() => new Promise<AttachmentUpload>((resolve) => (finish = resolve)));
    const { images } = adapter({ upload });
    const steps = images.add({ file: pngFile(640, 480) }) as AsyncGenerator<PendingAttachment, void>;
    const running = (await steps.next()).value as PendingAttachment;

    await images.remove(running);
    const rest = steps.next();
    finish(answer());

    expect(await rest).toEqual({ done: true, value: undefined });
  });

  it('a file already uploaded to the store is not uploaded again, by this adapter or the next', async () => {
    const file = pngFile(640, 480);
    const first = adapter();
    await added(first.images, file);
    const next = adapter();

    const states = await added(next.images, file);

    expect(first.upload).toHaveBeenCalledTimes(1);
    expect(next.upload).not.toHaveBeenCalled();
    expect((states[1] as ImageAttachment).stored).toEqual(answer());
  });

  it('a file whose upload is still going, added by the next adapter, waits for that upload and is not uploaded again', async () => {
    let finish!: (upload: AttachmentUpload) => void;
    const file = pngFile(640, 480);
    const first = adapter({ upload: vi.fn(() => new Promise<AttachmentUpload>((resolve) => (finish = resolve))) });
    const steps = first.images.add({ file }) as AsyncGenerator<PendingAttachment, void>;
    await steps.next();
    await vi.waitFor(() => expect(first.upload).toHaveBeenCalledTimes(1));
    const next = adapter();

    const adding = added(next.images, file);
    finish(answer());
    const states = await adding;

    expect(states.map((s) => s.status.type)).toEqual(['running', 'requires-action']);
    expect((states[1] as ImageAttachment).stored).toEqual(answer());
    expect(first.upload).toHaveBeenCalledTimes(1);
    expect(next.upload).not.toHaveBeenCalled();
  });

  it('the smaller file a downscale made, added again, is not uploaded again', async () => {
    const smaller = pngFile(2560, 1440, 'smaller.png');
    const { images, upload } = adapter({ downscale: vi.fn<Downscale>(async () => smaller) });
    const [, ready] = await added(images, pngFile(5120, 2880));
    expect(ready.file).toBe(smaller);

    const states = await added(images, smaller);

    expect((states[1] as ImageAttachment).stored).toEqual(answer());
    expect(upload).toHaveBeenCalledTimes(1);
  });

  it('the same file is uploaded again to another store, and again once forgotten', async () => {
    const file = pngFile(640, 480);
    const here = adapter();
    await added(here.images, file);
    const far = adapter({ source: 'far' });
    await added(far.images, file);
    expect(far.upload).toHaveBeenCalledTimes(1);

    here.images.forget([file]);
    await added(here.images, file);

    expect(here.upload).toHaveBeenCalledTimes(2);
  });

  it('an image over the edge is downscaled before the upload, and the smaller file is sent and kept', async () => {
    const smaller = pngFile(2560, 1440, 'smaller.png');
    const downscale = vi.fn<Downscale>(async () => smaller);
    const { images, upload } = adapter({ downscale });

    const [, ready] = await added(images, pngFile(5120, 2880));

    expect(downscale).toHaveBeenCalledWith(expect.any(File), { width: 5120, height: 2880 });
    expect(upload).toHaveBeenCalledWith(smaller);
    expect(ready.file).toBe(smaller);
  });

  it('an image within the edge and the cap is sent as it is', async () => {
    const { images, upload, downscale } = adapter();
    const file = pngFile(MAX_IMAGE_EDGE, 100);

    await added(images, file);

    expect(downscale).not.toHaveBeenCalled();
    expect(upload).toHaveBeenCalledWith(file);
  });

  it('an older far gglib, with no store, is named as that', async () => {
    const upload = vi.fn(async () => {
      throw new TransportError('NOT_FOUND', 'no route', { status: 404, type: 'not_found' });
    });
    const { images, onRefused } = adapter({ source: 'far', upload });

    await added(images, pngFile(640, 480));

    expect(onRefused).toHaveBeenCalledWith(FAR_CANNOT_TAKE_IMAGES);
  });

  it('a send of an image another adapter added for this store is sent by that upload, never uploaded again', async () => {
    const first = adapter();
    const [, ready] = await added(first.images, pngFile(640, 480));
    const next = adapter();

    const sent = (await next.images.send(ready)) as SentImage;

    expect(sent).toMatchObject({ id: ID, status: { type: 'complete' }, stored: answer() });
    expect(sent.refused).toBeUndefined();
    expect(first.upload).toHaveBeenCalledTimes(1);
    expect(next.upload).not.toHaveBeenCalled();
  });

  it('a send of an attachment it never added hands back no stored image, even one another store answered', async () => {
    const { images } = adapter();
    const stranger = {
      id: 'image-x', type: 'image', name: 'x.png', contentType: 'image/png', file: pngFile(1, 1),
      status: { type: 'requires-action', reason: 'composer-send' }, stored: answer('f'.repeat(64)),
    } as PendingAttachment;

    const sent = (await images.send(stranger)) as CompleteAttachment & SentImage;

    expect(sent.stored).toBeUndefined();
    expect(sent.refused).toBe(NOT_IN_THIS_STORE);
  });
});
