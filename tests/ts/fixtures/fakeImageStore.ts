/**
 * A daemon's image store in memory, answering as `handlers/attachments.rs`
 * and `AttachmentService` do:
 *
 * - `POST` takes the image as the raw body: a PNG or a JPEG whose size its
 *   header says, stored under the SHA-256 of its bytes, answered with its
 *   facts and `image_tokens` (one per 32 px square, at most 4096). The same
 *   bytes twice are one image. Anything else is `400 unsupported_image`.
 * - `GET /{id}` answers the bytes as they were sent, or `404
 *   attachment_not_found`.
 * - `check` refuses a run's messages as the pre-run check does: an id not
 *   stored, then images over 16 MiB together, history included.
 *
 * A read's body is a page `Blob` (jsdom's), as a browser's `fetch` gives one.
 */

import { pixelSize } from '../../../src/hooks/useGglibRuntime/imagePrep';
import type { AttachmentInfo } from '../../../src/types/generated/AttachmentInfo';
import type { AttachmentUpload } from '../../../src/types/generated/AttachmentUpload';

const MAX_REQUEST_IMAGE_BYTES = 16 * 1024 * 1024;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
}

function refusal(status: number, type: string, error: string): Response {
  return json({ error, status, type }, status);
}

/** The bytes of a page `Blob`, read as the page reads them. */
export function bytesOf(blob: Blob): Promise<Uint8Array<ArrayBuffer>> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(new Uint8Array(reader.result as ArrayBuffer));
    reader.onerror = () => reject(reader.error);
    reader.readAsArrayBuffer(blob);
  });
}

async function sha256(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** A PNG of `width` by `height` pixels: its signature and `IHDR`, then `fill` up to `length` bytes. */
export function png(width: number, height: number, fill = 0, length = 33): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(Math.max(length, 33)).fill(fill);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  const view = new DataView(bytes.buffer);
  view.setUint32(16, width);
  view.setUint32(20, height);
  bytes.set([8, 6, 0, 0, 0], 24);
  return bytes;
}

/** A pasted PNG of `width` by `height`, as the clipboard hands it over. */
export function pngFile(width: number, height: number, name = 'screenshot.png', fill = 0): File {
  return new File([png(width, height, fill)], name, { type: 'image/png' });
}

export class FakeImageStore {
  stored = new Map<string, { info: AttachmentInfo; bytes: Uint8Array<ArrayBuffer> }>();
  /** The answer the next upload gets instead, once. */
  refuseNextUpload: { status: number; type: string; error: string } | null = null;

  async upload(body: unknown): Promise<Response> {
    if (this.refuseNextUpload) {
      const { status, type, error } = this.refuseNextUpload;
      this.refuseNextUpload = null;
      return refusal(status, type, error);
    }
    const bytes = await bytesOf(body as Blob);
    const size = pixelSize(bytes);
    if (!size) return refusal(400, 'unsupported_image', 'The body is not a PNG or a JPEG whose size can be read.');
    const id = await sha256(bytes);
    const mime = bytes[0] === 0x89 ? 'image/png' : 'image/jpeg';
    const info: AttachmentInfo = { id, mime, ...size };
    this.stored.set(id, { info, bytes });
    const tokens = Math.min(4096, Math.ceil(size.width / 32) * Math.ceil(size.height / 32));
    return json({ ...info, image_tokens: tokens } satisfies AttachmentUpload);
  }

  read(id: string): Response {
    const held = this.stored.get(id);
    if (!held) return refusal(404, 'attachment_not_found', 'No stored image has that id.');
    const response = new Response(null, { status: 200, headers: { 'content-type': held.info.mime } });
    const blob = new Blob([held.bytes], { type: held.info.mime });
    return Object.assign(response, { blob: async () => blob });
  }

  /** The facts of each id, as a saved row lists them. */
  infos(ids: readonly string[]): AttachmentInfo[] {
    return ids.map((id) => this.stored.get(id)!.info);
  }

  /** The pre-run check of every image the messages name: a refusal, or `null`. */
  check(messages: ReadonlyArray<{ images?: string[] }>): Response | null {
    let total = 0;
    for (const id of messages.flatMap((m) => m.images ?? [])) {
      const held = this.stored.get(id);
      if (!held) return refusal(400, 'attachment_not_found', `No stored image has the id ${id}.`);
      total += held.bytes.length;
      if (total > MAX_REQUEST_IMAGE_BYTES) {
        return refusal(400, 'request_images_too_large', "The chat's images are over 16 MiB together.");
      }
    }
    return null;
  }
}
