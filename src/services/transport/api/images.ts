/**
 * Whether a message sent with Draw pressed can draw, and why not: this
 * machine's answer at `/api/images/drawing`, or the far machine's at
 * `/api/remote/images/drawing`, which this machine's daemon asks through the
 * tunnel. The answer greys the composer's Draw button, its `reason` the
 * button's title. A send says `draw` only after an answer of `available`.
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
}

/**
 * Whether a message in a chat of `source` can draw. This machine is told
 * what `chat` says of the model; the far machine is asked nothing more, as
 * the chat and its model are both its own.
 */
export async function drawingAvailability(
  source: ChatSource,
  chat: DrawingChat = {},
): Promise<DrawingAvailability> {
  if (source === 'far') return get<DrawingAvailability>('/api/remote/images/drawing');
  const query = new URLSearchParams();
  if (chat.far) query.set('far', 'true');
  if (chat.callsTools != null) query.set('calls_tools', String(chat.callsTools));
  const asked = query.toString();
  return get<DrawingAvailability>(`/api/images/drawing${asked ? `?${asked}` : ''}`);
}
