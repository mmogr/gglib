/**
 * The images a user's turn carries, from send to give-back: read off a
 * message, checked before anything is sent, and handed back to the composer
 * with the text when the send does not go.
 *
 * @module turnImages
 */

import type { ComposerRuntime } from '@assistant-ui/react';
import type { GglibContent } from '../../types/messages';
import type { SentImage } from './imageAttachments';

/** The images of `message`, a user's: none for any other. */
export function imagesOf(message: { attachments?: readonly unknown[] } | undefined): SentImage[] {
  return [...((message?.attachments ?? []) as readonly SentImage[])];
}

/**
 * Why `images` cannot be sent, or `null`: an image the chat's store does not
 * hold, its upload failed or made for another store.
 */
export function unsentImage(images: readonly SentImage[]): string | null {
  const failed = images.find((image) => !image.stored);
  return failed ? (failed.refused ?? 'An image was not uploaded. Add it again.') : null;
}

/**
 * Put a draft back in `composer` rather than lose it: its text when it is
 * plain text, and each of its images, added again. An image this page holds
 * the file of is not uploaded again; one opened from a saved row is read
 * from its store (`blob`) and added as a file.
 */
export function giveDraftBack(
  composer: ComposerRuntime | undefined,
  content: GglibContent,
  images: readonly SentImage[],
  blob: (id: string) => Promise<Blob>,
): void {
  if (!composer) return;
  const [only, ...more] = typeof content === 'string' ? [{ type: 'text', text: content } as const] : content;
  if (more.length === 0 && only?.type === 'text') composer.setText(only.text);
  for (const image of images) {
    const file = image.file
      ? Promise.resolve(image.file)
      : blob(image.id).then((bytes) => new File([bytes], image.name, { type: image.contentType }));
    file.then((f) => composer.addAttachment(f)).catch(() => {});
  }
}
