/**
 * Minimal POST-capable SSE reader for the backend agent stream.
 *
 * Yields the trimmed JSON payload from each `data:` line.  Keepalive `ping`
 * frames and blank lines are silently skipped.
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
        const dataLine = rawEvent
          .split('\n')
          .find(l => l.startsWith('data:'));
        if (!dataLine) continue;

        const payload = dataLine.slice(5).trim();
        if (!payload || payload === 'ping') continue;

        yield payload;
      }
    }
  } finally {
    reader.releaseLock();
  }
}
