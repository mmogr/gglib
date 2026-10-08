/**
 * Reading Server-Sent Events (SSE) off a `fetch` response.
 *
 * `EventSource` can send neither an `Authorization` header nor a POST, so
 * every stream the app reads is a `fetch` whose body is read here. `readSse`
 * is the one reader: the event bus, a run's events, the server log, the setup
 * streams, the benchmark streams and the proxy dashboard all call it, so a
 * change in how a stream is framed reaches every one of them or none.
 */

/** One event: its data, and its name and id when the stream gave them. */
export interface SseEvent {
  /** The event's `data:` lines, joined with a newline. Never empty. */
  data: string;
  /** The `event:` field. */
  event?: string;
  /** The `id:` field. */
  id?: string;
}

/**
 * Options for opening an SSE stream.
 */
export interface SSEStreamOptions {
  /** HTTP headers (typically includes Authorization) */
  headers?: HeadersInit;
  /** Abort signal for canceling the stream */
  signal?: AbortSignal;
}

/** The stream was refused with an HTTP status. */
export class SSEHttpError extends Error {
  readonly status: number;

  constructor(status: number, statusText: string) {
    super(`SSE request failed: ${status} ${statusText}`);
    this.name = 'SSEHttpError';
    this.status = status;
  }
}

/**
 * Reads the events of a response body until it closes.
 *
 * Framed as the specification frames them: a blank line ends an event, a line
 * ends in LF or CRLF, a line starting with `:` is a comment, the `data:`
 * lines of one event are joined with a newline, and one space after a field's
 * colon is not part of its value. An event with no data is not yielded, and
 * an event the stream closes in the middle of is dropped. Bytes are decoded as
 * a stream, so a character split across two reads arrives whole.
 *
 * A body from a `fetch` that was given a signal rejects its pending read when
 * the signal fires, and that rejection is thrown from here. `signal` is for
 * the caller that wants the reading to end quietly instead once it has
 * fired: it is checked before each read, and the body is then cancelled.
 */
export async function* readSse(
  response: Response,
  signal?: AbortSignal,
): AsyncGenerator<SseEvent> {
  if (!response.body) throw new Error('SSE: no response body');

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let data: string[] = [];
  let event: string | undefined;
  let id: string | undefined;

  try {
    while (!signal?.aborted) {
      const { done, value: bytes } = await reader.read();
      if (done) break;

      buffer += decoder.decode(bytes, { stream: true });
      const lines = buffer.split('\n');
      // The last piece is a line still arriving: it waits for its ending.
      buffer = lines.pop() ?? '';

      for (const raw of lines) {
        const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw;

        if (line === '') {
          const joined = data.join('\n');
          if (joined !== '') {
            yield { data: joined, ...(event !== undefined && { event }), ...(id !== undefined && { id }) };
          }
          data = [];
          event = undefined;
          id = undefined;
          continue;
        }

        // A comment's field name is empty, so it matches nothing below.
        const colon = line.indexOf(':');
        const field = colon === -1 ? line : line.slice(0, colon);
        const value = colon === -1 ? '' : line.slice(colon + 1).replace(/^ /, '');
        if (field === 'data') data.push(value);
        else if (field === 'event') event = value;
        else if (field === 'id' && !value.includes('\0')) id = value;
      }
    }
  } finally {
    // A body left open after its signal fired would go on being consumed.
    if (signal?.aborted) await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

/**
 * Opens a GET event stream at `url` and reads it with `readSse`.
 *
 * For a stream that is not this session's daemon's, such as a running
 * proxy's dashboard, which has its own origin and its own credential. A
 * stream of the daemon's is opened with `apiFetch` instead.
 *
 * @example
 * ```typescript
 * const headers = { Authorization: 'Bearer token' };
 * const signal = new AbortController().signal;
 *
 * for await (const message of createSSEStream(url, { headers, signal })) {
 *   console.log(message.event, message.data);
 * }
 * ```
 */
export async function* createSSEStream(
  url: string,
  options: SSEStreamOptions = {}
): AsyncGenerator<SseEvent, void, unknown> {
  const { headers, signal } = options;

  const response = await fetch(url, { headers, signal });
  if (!response.ok) {
    throw new SSEHttpError(response.status, response.statusText);
  }

  yield* readSse(response);
}
