/**
 * The images a chat's messages carry, stored by the id an upload answers:
 * this machine's store at `/api/attachments`, or the paired machine's at
 * `/api/remote/attachments`, which this machine's daemon forwards through
 * the tunnel. A body is the image's own bytes, not JSON; so is a read.
 *
 * Nothing here logs an image or puts its bytes in an error.
 */

import type { AttachmentUpload } from '../../../types/generated/AttachmentUpload';
import { ATTACHMENTS_PATH, REMOTE_ATTACHMENTS_PATH } from '../../api/routes';
import { readData } from '../errors';
import type { ChatSource } from '../types/chat';
import { getAuthenticatedFetchConfig } from './client';

function storeOf(source: ChatSource): string {
  return source === 'far' ? REMOTE_ATTACHMENTS_PATH : ATTACHMENTS_PATH;
}

/**
 * Store `image` in `source`'s store, and answer its id, its size and what
 * it costs to send. The same bytes twice are one image and the same answer.
 * Refused with the store's coded error: `image_too_large` (413) and
 * `unsupported_image` (400).
 */
export async function uploadAttachment(source: ChatSource, image: Blob): Promise<AttachmentUpload> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(`${baseUrl}${storeOf(source)}`, {
    method: 'POST',
    headers: { ...(headers as Record<string, string>), 'Content-Type': image.type || 'application/octet-stream' },
    body: image,
  });
  return readData<AttachmentUpload>(response);
}

/**
 * The bytes of the image `id` in `source`'s store, read with this page's
 * credential: an `<img>` cannot send it, so the page shows the blob.
 */
export async function fetchAttachmentBlob(source: ChatSource, id: string): Promise<Blob> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(`${baseUrl}${storeOf(source)}/${encodeURIComponent(id)}`, {
    headers: headers as Record<string, string>,
  });
  if (!response.ok) await readData(response);
  return response.blob();
}
