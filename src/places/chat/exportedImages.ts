/**
 * The images of an exported chat that are in the file by their id alone:
 * the daemon writes each image's bytes beside its info as a `data:` URL
 * (`GET /api/conversations/{id}/export`), and leaves the URL out of one its
 * store could not give. The page saves the file's text as it comes, and
 * says how many it could not read.
 *
 * @module places/chat/exportedImages
 */

import type { ChatExport } from '../../types/generated/ChatExport';

/** How many images the file holds by their id alone, each counted once however many rows carry it. */
export function unreadImages(file: ChatExport): number {
  const unread = new Set<string>();
  for (const image of file.messages.flatMap((message) => message.images ?? [])) {
    if (image.data_url === undefined) unread.add(image.id);
  }
  return unread.size;
}

/** What the page says of the images in the file by their id alone; `null` when there are none. */
export function unreadSentence(unread: number): string | null {
  if (unread === 0) return null;
  return unread === 1
    ? '1 image could not be read. It is in the file by its id only.'
    : `${unread} images could not be read. They are in the file by their ids only.`;
}
