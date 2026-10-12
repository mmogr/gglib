/**
 * Whether a message sent with Draw pressed can draw, and why not: this
 * machine's answer at `/api/images/drawing`, or the far machine's at
 * `/api/remote/images/drawing`, which this machine's daemon asks through the
 * tunnel. The answer greys the composer's Draw button, its `reason` the
 * button's title. A send says `draw` only after an answer of `available`.
 * The answer also lists that machine's image models, and with `imageModel`
 * judges the one a person chose, as a run that names it would be.
 *
 * And a picture drawn again from itself (`redrawPicture`): this machine's
 * `/api/images/generations`, sent the picture, a strength, and a mask when
 * only part of it is to change, and read as the stream it answers.
 */

import type { DrawingAvailability } from '../../../types/generated/DrawingAvailability';
import type { ChatSource } from '../types/chat';
import { get } from './client';
import { parseFrame, streamSse } from './sse';

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

/** A picture to draw again: what `/api/images/generations` reads of one. */
export interface PictureRedraw {
  /** What the picture, or the part under the mask, should show. */
  prompt: string;
  /** The picture, a PNG, as base64. */
  image: string;
  /** A PNG the picture's size, white over what to draw again; left out to draw all of it again. */
  mask?: string;
  /** How far from the picture the result may go, from 0.05 to 1. */
  strength: number;
  /** The image model to draw with, by id; left out for the machine's own choice. */
  model?: number;
}

/** Where a redraw has got to, as the stream last said. */
export interface RedrawProgress {
  /** `queued`, `loading`, `sampling`, `decoding` or `finishing`. */
  stage: string;
  /** The step finished, while sampling. */
  step?: number;
  /** The steps the pass takes, while sampling. */
  total?: number;
  /** The place in line, while queued. */
  position?: number;
  /** The step's preview, a small PNG as base64. It is the machine's own frame: for a masked redraw, not yet laid over the picture. */
  frame?: string;
}

export interface RedrawHandlers {
  onProgress: (progress: RedrawProgress) => void;
  /** The picture drawn, a PNG as base64. The stream ends after it. */
  onDrawn: (png: string) => void;
  /** Why nothing was drawn, in the machine's words. Not called when the caller stops it. */
  onFailed: (message: string) => void;
}

interface ProgressFrame {
  stage?: string;
  step?: number;
  total?: number;
  position?: number;
  frame_b64?: string;
}

/**
 * Draw `redraw`'s picture again on this machine and report the render as it
 * goes. Returns the function that stops it: the reader leaves, and the
 * machine stops a render nobody reads. A stream that ends with no picture
 * and no error is a failure.
 */
export function redrawPicture(redraw: PictureRedraw, handlers: RedrawHandlers): () => void {
  let settled = false;
  const fail = (message: string) => {
    if (settled) return;
    settled = true;
    handlers.onFailed(message);
  };
  const { model, ...rest } = redraw;
  const leave = streamSse(
    '/api/images/generations',
    {
      onFrame: (frame) => {
        if (settled) return;
        if (frame.event === 'image_generation.progress') {
          const said = parseFrame<ProgressFrame>(frame);
          if (typeof said?.stage !== 'string') return;
          handlers.onProgress({
            stage: said.stage,
            ...(said.step != null && { step: said.step }),
            ...(said.total != null && { total: said.total }),
            ...(said.position != null && { position: said.position }),
            ...(said.frame_b64 && { frame: said.frame_b64 }),
          });
        } else if (frame.event === 'image_generation.completed') {
          const png = parseFrame<{ b64_json?: string }>(frame)?.b64_json;
          if (!png) return fail('The picture that came back could not be read.');
          settled = true;
          handlers.onDrawn(png);
        } else if (frame.event === 'error') {
          const said = parseFrame<{ error?: { message?: string } }>(frame)?.error?.message;
          fail(said ?? 'The picture could not be drawn.');
        }
      },
      onClose: () => fail('The render ended without a picture.'),
      onError: fail,
    },
    { ...rest, ...(model != null && { model: String(model) }), stream: true },
  );
  return () => {
    settled = true;
    leave();
  };
}
