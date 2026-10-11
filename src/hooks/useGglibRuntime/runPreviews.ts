/**
 * A run's preview frames: the picture a running tool is making, as far as it
 * has got.
 *
 * A frame comes beside the run's log (`event: preview`, no seq), not in it,
 * so it is held beside the messages and never in them: nothing that saves,
 * sends or exports a message can carry one. Each tool call keeps only its
 * newest frame, and loses it when its result arrives; a run that ends, or
 * is no longer read, keeps none.
 *
 * @module runPreviews
 */

import type { RunPreview } from '../../types/events/agentEvent';

/** One frame, as the page shows it. */
export type PreviewFrame = RunPreview['frame'];

/** The frames held, by the tool call each belongs to. */
export type RunPreviews = ReadonlyMap<string, PreviewFrame>;

/** No frames: what every reading starts and ends with. */
export const NO_PREVIEWS: RunPreviews = new Map();

/** How the reader changes the frames held. */
export type SetRunPreviews = (update: (held: RunPreviews) => RunPreviews) => void;

const IMAGE_TYPE = /^image\/[a-z0-9.+-]+$/;

/** A `preview` event's payload, or `null` when its data is not one. */
export function parsePreview(data: string): RunPreview | null {
  let parsed: Partial<RunPreview> | null;
  try {
    parsed = JSON.parse(data) as Partial<RunPreview> | null;
  } catch {
    return null;
  }
  const frame = parsed?.frame;
  if (typeof parsed?.tool_call_id !== 'string' || !frame) return null;
  if (typeof frame.b64 !== 'string' || frame.b64 === '') return null;
  if (typeof frame.mime !== 'string' || !IMAGE_TYPE.test(frame.mime)) return null;
  return { tool_call_id: parsed.tool_call_id, frame };
}

/** `held` with `preview` as its call's frame, in place of any older one. */
export function withPreview(held: RunPreviews, preview: RunPreview): RunPreviews {
  return new Map(held).set(preview.tool_call_id, preview.frame);
}

/** `held` without the frame of `toolCallId`; `held` itself when it has none. */
export function withoutPreview(held: RunPreviews, toolCallId: string): RunPreviews {
  if (!held.has(toolCallId)) return held;
  const left = new Map(held);
  left.delete(toolCallId);
  return left;
}

/** A frame as an image's `src`. */
export function previewSrc(frame: PreviewFrame): string {
  return `data:${frame.mime};base64,${frame.b64}`;
}
