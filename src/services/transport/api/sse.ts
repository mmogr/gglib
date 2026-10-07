/**
 * Starting a POST server-sent-event stream and handing its frames to
 * callbacks.
 *
 * `EventSource` cannot issue a POST, so llama install and update are a
 * `fetch` (`apiFetch`) whose body the shared reader (`utils/sse`) reads. They
 * speak the same `event:`/`data:` wire format and differ only in which event
 * names they emit and what the payloads mean.
 */

import { formatError, isAbortError } from '../../../utils/errors';
import { readSse } from '../../../utils/sse';
import { apiFetch } from './client';

/** One decoded frame: the SSE event name ('' when it has none) and its raw `data:` payload. */
export interface SseFrame {
  event: string;
  data: string;
}

export interface StreamSseHandlers {
  /** Called for every frame, in arrival order. */
  onFrame: (frame: SseFrame) => void;
  /** Called once when the server closes the stream cleanly. */
  onClose?: () => void;
  /**
   * Transport-level failure, in the daemon's words when it refused the
   * request. Not called when the caller aborts.
   */
  onError: (message: string) => void;
}

/**
 * POST to `path` and read the SSE response until it closes.
 *
 * Returns an abort function. Aborting stops reading and disconnects; it does
 * not stop whatever the server started, so callers whose work continues
 * server-side after a disconnect should say so in the UI.
 */
export function streamSse(
  path: string,
  handlers: StreamSseHandlers,
  body?: unknown,
): () => void {
  const controller = new AbortController();

  void (async () => {
    try {
      const response = await apiFetch(path, {
        method: 'POST',
        headers: {
          Accept: 'text/event-stream',
          ...(body !== undefined && { 'Content-Type': 'application/json' }),
        },
        ...(body !== undefined && { body: JSON.stringify(body) }),
        signal: controller.signal,
      });
      for await (const { event = '', data } of readSse(response)) {
        handlers.onFrame({ event, data });
      }
      handlers.onClose?.();
    } catch (err) {
      if (isAbortError(err)) return;
      handlers.onError(formatError(err));
    }
  })();

  return () => controller.abort();
}

/** Parse a frame's payload, returning undefined rather than throwing. */
export function parseFrame<T>(frame: SseFrame): T | undefined {
  try {
    return JSON.parse(frame.data) as T;
  } catch {
    return undefined;
  }
}
