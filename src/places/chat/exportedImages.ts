/**
 * A chat's saved rows as Export writes them: each image's bytes beside its
 * id, type and size, as a `data:` URL, so the file shows what was asked
 * about on a machine whose store never held the image. The bytes are read
 * from this machine's store with the page's credential, each image once
 * however many rows carry it.
 *
 * Nothing here logs an image or puts its bytes in an error.
 *
 * @module places/chat/exportedImages
 */

import { getTransport } from '../../services/transport';
import type { ChatMessage } from '../../services/transport';
import type { AttachmentInfo } from '../../types/generated/AttachmentInfo';
import { base64Of } from '../../utils/pictureMask';

/** An image in an exported chat: its info, and its bytes when the store gave them. */
type ExportedImage = AttachmentInfo & { data_url?: string };

/** A saved row as the export writes it. */
type ExportedMessage = Omit<ChatMessage, 'images'> & { images?: ExportedImage[] };

/** The image's bytes as a `data:` URL under the type the store recorded; `null` when they cannot be read. */
async function dataUrlOf(image: AttachmentInfo): Promise<string | null> {
  try {
    const blob = await getTransport().fetchAttachmentBlob('this', image.id);
    return `data:${image.mime};base64,${await base64Of(blob)}`;
  } catch {
    return null;
  }
}

/**
 * `messages` with every image's bytes beside its info, and how many images
 * the store could not give: those stay in the file as their info alone.
 */
export async function withImageBytes(messages: ChatMessage[]): Promise<{ messages: ExportedMessage[]; unread: number }> {
  const urls = new Map<string, string | null>();
  for (const image of messages.flatMap((message) => message.images ?? [])) {
    if (!urls.has(image.id)) urls.set(image.id, await dataUrlOf(image));
  }
  return {
    messages: messages.map((message) => {
      if (!message.images) return message;
      const images = message.images.map((image) => {
        const url = urls.get(image.id);
        return url ? { ...image, data_url: url } : image;
      });
      return { ...message, images };
    }),
    unread: [...urls.values()].filter((url) => url === null).length,
  };
}
