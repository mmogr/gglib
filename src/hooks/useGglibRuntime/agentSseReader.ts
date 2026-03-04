/**
 * Minimal POST-capable SSE reader for the backend agent stream.
 *
 * Yields the trimmed JSON payload from each `data:` line.  Keepalive `ping`
 * frames and blank lines are silently skipped.
 *
 * **Note:** only `data:` lines are processed.  Standard SSE fields `event:`,
 * `id:`, and `retry:` are silently ignored because the backend agent stream
 * uses plain `data:`-only events with JSON payloads — no named event types
 * or reconnection directives are emitted.
 *
 * @module agentSseReader
 */

/**
 * Reads raw SSE data payloads from a POST response body.
 *
 * Yields the trimmed JSON string from each `data:` line.  Keepalive `ping`
 * frames and blank lines are silently skipped.
 */
export async function* readAgentSSE(
  response: Response,
  abortSignal?: AbortSignal,
): AsyncGenerator<string> {
  if (!response.body) throw new Error('Agent SSE: no response body');

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
        // RFC 8895 §9.2: multiple `data:` lines in one event are concatenated
        // with a newline. Use filter+join rather than .find() to handle this
        // correctly and avoid silently dropping multi-line payloads.
        const payload = rawEvent
          .split('\n')
          .filter(l => l.startsWith('data:'))
          .map(l => l.slice(5))
          .join('\n')
          .trim();
        if (!payload || payload === 'ping') continue;

        yield payload;
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
