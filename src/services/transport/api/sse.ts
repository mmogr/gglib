/**
 * Reading a POST server-sent-event stream.
 *
 * `EventSource` cannot issue a POST, so every streaming endpoint gglib
 * exposes has to be read by hand off `fetch`. This is that loop, written
 * once: llama install, build-from-source and update all speak the same
 * `event:`/`data:` wire format and differ only in which event names they
 * emit and what the payloads mean.
 */

import { getApiBaseUrl, getAuthHeaders } from './client';

/** One decoded frame: the SSE event name and its raw `data:` payload. */
export interface SseFrame {
  event: string;
  data: string;
}

export interface StreamSseHandlers {
  /** Called for every frame, in arrival order. */
  onFrame: (frame: SseFrame) => void;
  /** Called once when the server closes the stream cleanly. */
  onClose?: () => void;
  /** Transport-level failure. Not called when the caller aborts. */
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

  void fetch(`${getApiBaseUrl()}${path}`, {
    method: 'POST',
    headers: {
      ...getAuthHeaders(),
      Accept: 'text/event-stream',
      ...(body !== undefined && { 'Content-Type': 'application/json' }),
    },
    ...(body !== undefined && { body: JSON.stringify(body) }),
    signal: controller.signal,
  })
    .then(async (response) => {
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      if (!response.body) {
        throw new Error('No response body for SSE stream');
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let currentEvent = '';

      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        // A frame can straddle a chunk boundary, so the trailing partial
        // line stays in the buffer until the next read completes it.
        buffer = lines.pop() ?? '';

        for (const line of lines) {
          if (line.startsWith('event: ')) {
            currentEvent = line.slice(7).trim();
          } else if (line.startsWith('data: ')) {
            handlers.onFrame({ event: currentEvent, data: line.slice(6) });
            currentEvent = '';
          }
        }
      }

      handlers.onClose?.();
    })
    .catch((err: unknown) => {
      if (err instanceof Error && err.name === 'AbortError') return;
      handlers.onError(err instanceof Error ? err.message : String(err));
    });

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
