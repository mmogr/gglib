/**
 * The sentence a person reads when an image is refused: at its upload, or
 * with the send that carries it. Each is said by the refusal's code, never
 * with the image, and says what to do next.
 *
 * @module imageRefusals
 */

import { TransportError } from '../../services/transport/errors';
import { ImageRefusal } from './imagePrep';
import { formatError } from '../../utils/errors';

/**
 * A send carried an image uploaded for another store than its chat's (the
 * chat list moved to the other machine with it in the composer): the
 * message comes back, and its images are added to this chat's store.
 */
export const NOT_IN_THIS_STORE =
  "An image was not uploaded to this chat's store. The message is back with its images, added to this chat: send it once more.";

const SENTENCES: Record<string, string> = {
  model_cannot_read_images:
    'This model cannot read images: it has no projector. Link one in its inspector, or send the text alone.',
  attachment_not_found:
    'An image was no longer stored. The message is back with its images, uploaded again: send it once more.',
  request_images_too_large:
    "This chat's images, with the ones before, are over the 16 MiB one request may carry. Start a new chat, or send fewer or smaller images.",
  image_too_large: 'An image is over the 8 MiB one image may be. Send a smaller one.',
  unsupported_image: 'Only PNG and JPEG images can be sent.',
  request_too_large: 'The message is too large to send. Send fewer or smaller images.',
};

/** The daemon's code for `error`, or the page's own for an image it refused. */
export function codeOf(error: unknown): string | null {
  if (error instanceof ImageRefusal) return error.code;
  if (!TransportError.isTransportError(error)) return null;
  const type = (error.details as { type?: unknown } | undefined)?.type;
  return typeof type === 'string' ? type : null;
}

/**
 * What a person reads when the upload of an image failed: by its code, or
 * the failure as it was told (a paired machine too old to take one says to
 * update it, in the daemon's words).
 */
export function uploadRefusal(error: unknown): string {
  const code = codeOf(error);
  if (code && SENTENCES[code]) return SENTENCES[code];
  const told = formatError(error);
  return `The image could not be uploaded: ${told}`;
}

/**
 * What a person reads when a send carrying images was refused, by its code,
 * or `null` when the refusal is not about them.
 */
export function sendRefusal(error: unknown): string | null {
  const code = codeOf(error);
  return code && SENTENCES[code] ? SENTENCES[code] : null;
}
