/**
 * Unit tests for sseEvents — the SSE reader a run's events are read with.
 *
 * Uses a mock ReadableStream to verify:
 * - Well-formed single events
 * - Multiple events in one chunk
 * - Events split across chunks
 * - CRLF normalization
 * - Multi-line data: concatenation (RFC 8895 §9.2)
 * - Keepalive ping filtering
 * - Empty body rejection
 * - Abort signal handling
 */

import { describe, it, expect } from 'vitest';
import { readSseEvents } from '../../../../src/services/transport/api/sseEvents';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a Response with a ReadableStream body from string chunks. */
function makeResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(encoder.encode(chunk));
      }
      controller.close();
    },
  });
  return new Response(stream);
}

/** Collect all payloads from the async generator. */
async function collect(response: Response, signal?: AbortSignal): Promise<string[]> {
  const payloads: string[] = [];
  for await (const event of readSseEvents(response, signal)) {
    payloads.push(event.data);
  }
  return payloads;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('readSseEvents', () => {
  it('throws when response has no body', async () => {
    const response = new Response(null);
    await expect(collect(response)).rejects.toThrow('no response body');
  });

  it('parses a single well-formed SSE event', async () => {
    const response = makeResponse(['data: {"type":"text_delta","content":"hi"}\n\n']);
    const payloads = await collect(response);
    expect(payloads).toEqual(['{"type":"text_delta","content":"hi"}']);
  });

  it('parses multiple events in a single chunk', async () => {
    const response = makeResponse([
      'data: {"type":"text_delta","content":"a"}\n\n' +
      'data: {"type":"text_delta","content":"b"}\n\n',
    ]);
    const payloads = await collect(response);
    expect(payloads).toHaveLength(2);
    expect(payloads[0]).toContain('"a"');
    expect(payloads[1]).toContain('"b"');
  });

  it('reassembles events split across chunks', async () => {
    const response = makeResponse([
      'data: {"type":"text_del',
      'ta","content":"hello"}\n\n',
    ]);
    const payloads = await collect(response);
    expect(payloads).toEqual(['{"type":"text_delta","content":"hello"}']);
  });

  it('normalizes CRLF to LF', async () => {
    const response = makeResponse(['data: {"ok":true}\r\n\r\n']);
    const payloads = await collect(response);
    expect(payloads).toEqual(['{"ok":true}']);
  });

  it('concatenates multiple data: lines per RFC 8895 §9.2', async () => {
    // Two data: lines in the same event should be joined with \n.
    // Note: the reader uses `.slice(5)` which keeps the leading space
    // after "data:", so inner lines retain a space prefix after join.
    // `.trim()` only strips the outer whitespace from the full payload.
    const response = makeResponse([
      'data:line1\ndata:line2\n\n',
    ]);
    const payloads = await collect(response);
    expect(payloads).toEqual(['line1\nline2']);
  });

  it('skips keepalive ping frames', async () => {
    const response = makeResponse([
      'data: ping\n\n',
      'data: {"type":"final_answer","content":"done"}\n\n',
    ]);
    const payloads = await collect(response);
    expect(payloads).toHaveLength(1);
    expect(payloads[0]).toContain('final_answer');
  });

  it('skips blank events (empty data)', async () => {
    const response = makeResponse([
      '\n\n',
      'data: {"ok":true}\n\n',
    ]);
    const payloads = await collect(response);
    expect(payloads).toEqual(['{"ok":true}']);
  });

  it('carries an event\'s id and name beside its data, and ignores retry:', async () => {
    const response = makeResponse([
      'event: run\nid: 42\nretry: 3000\ndata: {"ok":true}\n\n',
      'id:7\ndata: {"n":7}\n\n',
    ]);
    const events = [];
    for await (const event of readSseEvents(response)) events.push(event);
    expect(events).toEqual([
      { id: '42', event: 'run', data: '{"ok":true}' },
      { id: '7', data: '{"n":7}' },
    ]);
  });

  it('skips keepalive comments', async () => {
    const response = makeResponse([':\n\n', 'data: {"ok":true}\n\n']);
    expect(await collect(response)).toEqual(['{"ok":true}']);
  });

  it('returns empty array for an empty stream', async () => {
    const response = makeResponse([]);
    const payloads = await collect(response);
    expect(payloads).toEqual([]);
  });

  it('stops reading when abort signal fires', async () => {
    const controller = new AbortController();
    // Build a *pull-based* stream that serves one event per read() call.
    // This guarantees the reader loops back to the while-check between events.
    const encoder = new TextEncoder();
    const chunks = [
      encoder.encode('data: {"n":1}\n\n'),
      encoder.encode('data: {"n":2}\n\n'),
    ];
    let index = 0;
    const stream = new ReadableStream<Uint8Array>({
      pull(ctrl) {
        if (index < chunks.length) {
          ctrl.enqueue(chunks[index++]);
        } else {
          ctrl.close();
        }
      },
    });
    const response = new Response(stream);

    // Abort immediately after consuming the first event.
    const payloads: string[] = [];
    for await (const event of readSseEvents(response, controller.signal)) {
      payloads.push(event.data);
      controller.abort();
    }

    // Only the first event should have been consumed — the reader checks
    // the abort signal before the next `reader.read()` call.
    expect(payloads).toHaveLength(1);
    expect(payloads[0]).toBe('{"n":1}');
  });
});
