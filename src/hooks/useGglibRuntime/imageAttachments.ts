/**
 * The composer's images: an assistant-ui attachment adapter over a chat's
 * image store (this machine's, or the far machine's for a far chat).
 *
 * An image is uploaded when it is added (pasted, dropped or picked), never
 * when the message is sent: assistant-ui empties the composer before a
 * send's attachments settle, so an upload that failed there would lose the
 * draft. `add` shows the image at once, uploads it, and marks it ready;
 * `send` only waits for an upload still going, and turns the attachment
 * into the stored image's id. An upload that fails is said at once, since
 * assistant-ui only logs a paste or a drop that throws; its image stays in
 * the composer, marked, and a send that carries it is handed back whole.
 * So is a send of an image another adapter added for another store (the
 * chat list moved to the other machine with it in the composer): it is
 * stored only by an upload to this store, and added back it goes there. One
 * this store already holds (the chat list went there and came back) is sent
 * by that upload.
 *
 * Each upload is remembered by the `File` it was made from, per store, for
 * as long as the page holds that `File`: a message handed back after a
 * refusal, or carried over a model switch, adds the same files again, and
 * they are not uploaded twice.
 *
 * @module imageAttachments
 */

import { useEffect, useMemo, useRef } from 'react';
import type { Attachment, AttachmentAdapter, CompleteAttachment, PendingAttachment } from '@assistant-ui/react';
import type { ChatSource } from '../../services/transport';
import { imageStoreOf } from './chatSource';
import type { AttachmentInfo } from '../../types/generated/AttachmentInfo';
import type { AttachmentUpload } from '../../types/generated/AttachmentUpload';
import { IMAGE_ACCEPT, ImageRefusal, downscaleInBrowser, pixelSizeOf, prepareImage, type Downscale } from './imagePrep';
import { NOT_IN_THIS_STORE, uploadRefusal } from './imageRefusals';

/** What the store answered for an image: its facts, and its cost when uploaded here. */
export type StoredImage = AttachmentInfo & { image_tokens?: number };

/**
 * An image in the composer or a message. `stored` is what the store
 * answered; a sent image without it is one whose upload failed or was made
 * for another store, and `refused` says why.
 */
export type ImageAttachment = Attachment & { stored?: StoredImage; refused?: string };

/** An image a message carries: sent, and complete. */
export type SentImage = CompleteAttachment & { stored?: StoredImage; refused?: string };

export interface ImageAttachmentOptions {
  /** Whose store the images go to. */
  source: ChatSource;
  /** Upload one image, ready to send, to that store. */
  upload: (image: File) => Promise<AttachmentUpload>;
  /** Makes an image too large to send smaller; the browser's canvas by default. */
  downscale?: Downscale;
  /** Tell the person why an image was not taken. */
  onRefused: (sentence: string) => void;
}

export interface ImageAttachmentAdapter extends AttachmentAdapter {
  /** Forget the uploads of `files`, so adding them again uploads them again. */
  forget(files: readonly File[]): void;
}

interface Uploaded {
  file: File;
  upload: AttachmentUpload;
}

/** Each upload, by the file it was made from, per store. */
const known = new WeakMap<File, Map<ChatSource, Promise<Uploaded>>>();

function remember(file: File, source: ChatSource, done: Promise<Uploaded>): void {
  const bySource = known.get(file) ?? new Map<ChatSource, Promise<Uploaded>>();
  bySource.set(source, done);
  known.set(file, bySource);
}

export function createImageAttachmentAdapter(options: ImageAttachmentOptions): ImageAttachmentAdapter {
  const { source } = options;
  const downscale = options.downscale ?? downscaleInBrowser;
  /** The uploads of the images in the composer, by their attachment id. */
  const pending = new Map<string, { done: Promise<Uploaded>; sent: boolean }>();

  const upload = (file: File, ready: Promise<File>): Promise<Uploaded> => {
    const done = ready.then(async (image) => ({ file: image, upload: await options.upload(image) }));
    remember(file, source, done);
    done.then(
      (uploaded) => remember(uploaded.file, source, done),
      () => known.get(file)?.delete(source),
    );
    return done;
  };

  return {
    accept: IMAGE_ACCEPT,

    async *add({ file }) {
      let done = known.get(file)?.get(source);
      if (!done) {
        const size = await pixelSizeOf(file);
        if (!size) {
          options.onRefused(uploadRefusal(new ImageRefusal('unsupported_image', 'not an image'), source));
          return;
        }
        done = upload(file, prepareImage(file, size, downscale));
      }
      const id = `image-${crypto.randomUUID()}`;
      const entry = { done, sent: false };
      pending.set(id, entry);
      const base = { id, type: 'image' as const, name: file.name || 'image', contentType: file.type, file };
      yield { ...base, status: { type: 'running', reason: 'uploading', progress: 0 } } satisfies PendingAttachment;
      let ready: PendingAttachment & { stored?: StoredImage };
      try {
        const uploaded = await done;
        ready = {
          ...base,
          file: uploaded.file,
          contentType: uploaded.upload.mime,
          stored: uploaded.upload,
          status: { type: 'requires-action', reason: 'composer-send' },
        };
      } catch (error) {
        if (!entry.sent) options.onRefused(uploadRefusal(error, source));
        ready = { ...base, status: { type: 'incomplete', reason: 'error' } };
      }
      // Sent or removed while it uploaded: it is no longer the composer's.
      if (!entry.sent && pending.has(id)) yield ready;
    },

    async send(attachment): Promise<SentImage> {
      const entry = pending.get(attachment.id);
      // Stored only by an upload to this store: this adapter's own, or the
      // one it remembers for the file (added by another adapter, before the
      // chat list went to the other machine and came back). An image added
      // for another store carries that store's answer, which this one does
      // not hold. Nothing is uploaded here.
      const sent = { ...attachment, status: { type: 'complete' as const }, content: [], stored: undefined };
      const done = entry?.done ?? (attachment.file ? known.get(attachment.file)?.get(source) : undefined);
      if (!done) return { ...sent, refused: NOT_IN_THIS_STORE };
      if (entry) {
        entry.sent = true;
        pending.delete(attachment.id);
      }
      try {
        const { file, upload: stored } = await done;
        return { ...sent, id: stored.id, file, contentType: stored.mime, stored };
      } catch (error) {
        return { ...sent, refused: uploadRefusal(error, source) };
      }
    },

    async remove(attachment) {
      pending.delete(attachment.id);
    },

    forget(files) {
      for (const file of files) known.get(file)?.delete(source);
    },
  };
}

/**
 * The adapter for `source`'s chat, kept for as long as the chat's source
 * is: the composer's uploads in flight are its own. `onRefused` is read when
 * a refusal happens, so a new one each render does not make a new adapter.
 */
export function useImageAttachments(
  source: ChatSource,
  onRefused: ((sentence: string) => void) | undefined,
  downscale?: Downscale,
): ImageAttachmentAdapter {
  const refusedRef = useRef(onRefused);
  useEffect(() => {
    refusedRef.current = onRefused;
  }, [onRefused]);
  return useMemo(
    () =>
      createImageAttachmentAdapter({
        source,
        upload: imageStoreOf(source).upload,
        downscale,
        onRefused: (sentence) => refusedRef.current?.(sentence),
      }),
    [source, downscale],
  );
}
