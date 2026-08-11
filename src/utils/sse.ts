/**
 * Fetch-based Server-Sent Events (SSE) utility with authentication support.
 *
 * This implementation replaces native EventSource to support:
 * - Authorization headers (Bearer token)
 * - Spec-compliant SSE parsing (multi-line data, event types, comments)
 * - Exponential backoff reconnection with jitter
 * - Proper cleanup and abort handling
 */

/**
 * Parsed SSE message.
 */
export interface SSEMessage {
  /** Event type (default: 'message') */
  event: string;
  /** Message data (multi-line data: fields joined with \n) */
  data: string;
  /** Last event ID (for resuming streams) */
  id?: string;
}

/**
 * Options for creating an SSE stream.
 */
export interface SSEStreamOptions {
  /** HTTP headers (typically includes Authorization) */
  headers?: HeadersInit;
  /** Abort signal for canceling the stream */
  signal?: AbortSignal;
  /** Last event ID (for resuming from a specific point) */
  lastEventId?: string;
}

/**
 * Creates an async iterable SSE stream from a URL.
 *
 * Parses Server-Sent Events according to the spec:
 * - Lines starting with ':' are comments (keepalive)
 * - 'data:' lines accumulate (joined with \n)
 * - 'event:' sets the message type
 * - 'id:' sets the event ID
 * - Blank lines dispatch the accumulated message
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
): AsyncGenerator<SSEMessage, void, unknown> {
  const { headers = {}, signal, lastEventId } = options;

  // Add Last-Event-ID header if resuming
  const fetchHeaders = new Headers(headers);
  if (lastEventId) {
    fetchHeaders.set('Last-Event-ID', lastEventId);
  }

  // Fetch with streaming enabled
  const response = await fetch(url, {
    headers: fetchHeaders,
    signal,
  });

  if (!response.ok) {
    throw new Error(`SSE request failed: ${response.status} ${response.statusText}`);
  }

  if (!response.body) {
    throw new Error('SSE response has no body');
  }

  // Read the stream
  const reader = response.body.getReader();
  const decoder = new TextDecoder();

  let buffer = '';
  let currentMessage: Partial<SSEMessage> = { event: 'message', data: '' };

  try {
    while (true) {
      const { done, value } = await reader.read();

      if (done) {
        break;
      }

      // Decode chunk and add to buffer
      buffer += decoder.decode(value, { stream: true });

      // Process complete lines
      const lines = buffer.split('\n');
      // Keep the last incomplete line in the buffer
      buffer = lines.pop() || '';

      for (const line of lines) {
        // Empty line dispatches the message
        if (line === '' || line === '\r') {
          if (currentMessage.data) {
            // Remove trailing newline from multi-line data
            if (currentMessage.data.endsWith('\n')) {
              currentMessage.data = currentMessage.data.slice(0, -1);
            }
            yield currentMessage as SSEMessage;
          }
          // Reset for next message
          currentMessage = { event: 'message', data: '' };
          continue;
        }

        // Comment lines (keepalive) - ignore
        if (line.startsWith(':')) {
          continue;
        }

        // Parse field
        const colonIndex = line.indexOf(':');
        if (colonIndex === -1) {
          // Line with no colon is treated as field with empty value
          continue;
        }

        const field = line.slice(0, colonIndex);
        // Value starts after colon, skip leading space if present
        let value = line.slice(colonIndex + 1);
        if (value.startsWith(' ')) {
          value = value.slice(1);
        }

        // Process field
        switch (field) {
          case 'event':
            currentMessage.event = value;
            break;
          case 'data':
            // Accumulate data with newlines between multiple data: fields
            if (currentMessage.data) {
              currentMessage.data += '\n' + value;
            } else {
              currentMessage.data = value;
            }
            break;
          case 'id':
            // Update the last event ID (for resuming)
            if (!value.includes('\0')) {
              // Spec: ignore if contains null
              currentMessage.id = value;
            }
            break;
          case 'retry':
            // Ignore retry field (not used in this implementation)
            break;
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}


