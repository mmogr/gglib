/**
 * Whether a message sent with Draw pressed can draw, and why not: this
 * machine's answer at `/api/images/drawing`, or the far machine's at
 * `/api/remote/images/drawing`, which this machine's daemon asks through the
 * tunnel. The answer greys the composer's Draw button, its `reason` the
 * button's title. A send says `draw` only after an answer of `available`.
 * The answer also lists that machine's image models, and with `imageModel`
 * judges the one a person chose, as a run that names it would be.
 */

import type { DrawingAvailability } from '../../../types/generated/DrawingAvailability';
import type { ChatSource } from '../types/chat';
import { get } from './client';

/** What this machine is told of the chat's model, which it cannot know itself. */
export interface DrawingChat {
  /** The chat's model is on the paired machine, run from here. */
  far?: boolean;
  /** Whether the chat's model calls tools; left out when unknown. */
  callsTools?: boolean | null;
  /** The image model chosen for the chat, by its id on the machine asked; left out when none is. */
  imageModel?: number | null;
}

/**
 * Whether a message in a chat of `source` can draw. This machine is told
 * what `chat` says of the model; the far machine is asked nothing of that, as
 * the chat and its model are both its own. Either is asked about the chosen
 * image model, when there is one.
 */
export async function drawingAvailability(
  source: ChatSource,
  chat: DrawingChat = {},
): Promise<DrawingAvailability> {
  const query = new URLSearchParams();
  if (source !== 'far') {
    if (chat.far) query.set('far', 'true');
    if (chat.callsTools != null) query.set('calls_tools', String(chat.callsTools));
  }
  if (chat.imageModel != null) query.set('image_model', String(chat.imageModel));
  const asked = query.toString();
  const route = source === 'far' ? '/api/remote/images/drawing' : '/api/images/drawing';
  return get<DrawingAvailability>(`${route}${asked ? `?${asked}` : ''}`);
}
