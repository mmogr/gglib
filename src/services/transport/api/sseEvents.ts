/**
 * Reading server-sent events off a `fetch` response.
 *
 * Yields each event's `data:` payload with its `id:` and `event:` fields, so
 * a run's reader can tell a numbered frame from the named `run` event that
 * ends it. Keepalive comments, `ping` payloads and blank events are skipped.
 *
 * @module sseEvents
 */

/** One event: its data, and its id and name when it has them. */
export interface SseEvent {
  id?: string;
  event?: string;
  data: string;
}

/** The value of `field:` on one of an event's lines, or undefined. */
function fieldValue(line: string, field: string): string | undefined {
  if (!line.startsWith(`${field}:`)) return undefined;
  return line.slice(field.length + 1).trim();
}

/**
 * Reads the events of a response body until it closes or `abortSignal`
 * fires.
 */
export async function* readSseEvents(
  response: Response,
  abortSignal?: AbortSignal,
): AsyncGenerator<SseEvent> {
  if (!response.body) throw new Error('SSE: no response body');

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  try {
    while (true) {
      if (abortSignal?.aborted) break;

      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Normalise CRLF → LF so the split below works regardless of whether the
      // server (or a proxy) uses \r\n or \n as SSE line terminators.
      // SSE events are separated by blank lines (\n\n after normalization).
      const rawEvents = buffer.replace(/\r\n/g, '\n').split('\n\n');
      buffer = rawEvents.pop() ?? ''; // keep the trailing partial event

      for (const rawEvent of rawEvents) {
        const lines = rawEvent.split('\n');
        // RFC 8895 §9.2: multiple `data:` lines in one event are concatenated
        // with a newline. Use filter+join rather than .find() to handle this
        // correctly and avoid silently dropping multi-line payloads.
        const data = lines
          .filter(l => l.startsWith('data:'))
          .map(l => l.slice(5))
          .join('\n')
          .trim();
        if (!data || data === 'ping') continue;

        let id: string | undefined;
        let event: string | undefined;
        for (const line of lines) {
          id = fieldValue(line, 'id') ?? id;
          event = fieldValue(line, 'event') ?? event;
        }
        yield { data, ...(id !== undefined && { id }), ...(event !== undefined && { event }) };
      }
    }
  } finally {
    // If the abort signal fired, cancel the underlying stream so the browser
    // doesn't continue consuming a stale chunk that arrived after the break.
    if (abortSignal?.aborted) {
      await reader.cancel().catch(() => {});
    }
    reader.releaseLock();
  }
}
