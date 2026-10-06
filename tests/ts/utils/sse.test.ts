/**
 * `readSse`, the one reader every event stream in the app is read with.
 *
 * It replaced four: the event bus's, a run's, the setup streams' and the
 * benchmark's, which framed the same bytes four ways. What is pinned here is
 * the framing all of them now share: a frame split across reads, CRLF line
 * endings, a `data:` field of several lines, a comment line, and a character
 * split across reads.
 */

import { describe, it, expect, vi } from 'vitest';
import { createSSEStream, readSse, SSEHttpError, type SseEvent } from '../../../src/utils/sse';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** A Response whose body arrives as `chunks`, one per read. */
function makeResponse(chunks: (string | Uint8Array)[]): Response {
  const encoder = new TextEncoder();
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(typeof chunk === 'string' ? encoder.encode(chunk) : chunk);
      }
      controller.close();
    },
  });
  return new Response(stream);
}

/** Every event of the response. */
async function events(response: Response, signal?: AbortSignal): Promise<SseEvent[]> {
  const all: SseEvent[] = [];
  for await (const event of readSse(response, signal)) all.push(event);
  return all;
}

/** Every event's data. */
async function collect(response: Response, signal?: AbortSignal): Promise<string[]> {
  return (await events(response, signal)).map((event) => event.data);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('readSse', () => {
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
    expect(payloads).toEqual(['{"type":"text_delta","content":"a"}', '{"type":"text_delta","content":"b"}']);
  });

  it('reassembles a frame split across reads, wherever the split falls', async () => {
    const frame = 'event: log\nid: 3\ndata: {"type":"text_delta","content":"hello"}\n\n';
    for (let at = 1; at < frame.length; at++) {
      const response = makeResponse([frame.slice(0, at), frame.slice(at)]);
      expect(await events(response), `split at ${at}`).toEqual([
        { event: 'log', id: '3', data: '{"type":"text_delta","content":"hello"}' },
      ]);
    }
  });

  it('reads CRLF line endings as it reads LF, with no carriage return left in a value', async () => {
    const response = makeResponse(['event: log\r\nid: 3\r\ndata: {"ok":true}\r\n\r\ndata: second\r\n\r\n']);
    expect(await events(response)).toEqual([
      { event: 'log', id: '3', data: '{"ok":true}' },
      { data: 'second' },
    ]);
  });

  it('reads a CRLF whose two bytes arrive in different reads', async () => {
    const response = makeResponse(['data: {"ok":true}\r', '\n\r', '\ndata: second\r\n', '\r\n']);
    expect(await collect(response)).toEqual(['{"ok":true}', 'second']);
  });

  it('joins the data: lines of one event with a newline, each less the one space after its colon', async () => {
    const response = makeResponse(['data: line1\ndata: line2\ndata:line3\ndata:  indented\n\n']);
    expect(await collect(response)).toEqual(['line1\nline2\nline3\n indented']);
  });

  it('gives a JSON payload spread over several data: lines back as one payload', async () => {
    const response = makeResponse(['data: {"a":\ndata: 1}\n\n']);
    const [payload] = await collect(response);
    expect(JSON.parse(payload)).toEqual({ a: 1 });
  });

  it('yields nothing for a comment line, alone or inside an event', async () => {
    const response = makeResponse([': keepalive\n\n', ':ping\n\n', ':\n\n', 'data: {"ok":true}\n: mid-event\n\n']);
    expect(await events(response)).toEqual([{ data: '{"ok":true}' }]);
  });

  it('decodes a character whole when its bytes are split across reads', async () => {
    const bytes = new TextEncoder().encode('data: {"text":"héllo → 日本語 🚀"}\n\n');
    for (let at = 1; at < bytes.length; at++) {
      const response = makeResponse([bytes.slice(0, at), bytes.slice(at)]);
      expect(await collect(response), `split at byte ${at}`).toEqual(['{"text":"héllo → 日本語 🚀"}']);
    }
  });

  it('does not treat a ping payload as special: what a keepalive is, is its caller\'s to say', async () => {
    const response = makeResponse(['data: ping\n\n', 'data: {"type":"final_answer","content":"done"}\n\n']);
    expect(await collect(response)).toEqual(['ping', '{"type":"final_answer","content":"done"}']);
  });

  it('skips blank events (empty data)', async () => {
    const response = makeResponse([
      '\n\n',
      'event: named\n\n',
      'data:\n\n',
      'data: {"ok":true}\n\n',
    ]);
    expect(await events(response)).toEqual([{ data: '{"ok":true}' }]);
  });

  it('carries an event\'s id and name beside its data, and ignores retry:', async () => {
    const response = makeResponse([
      'event: run\nid: 42\nretry: 3000\ndata: {"ok":true}\n\n',
      'id:7\ndata: {"n":7}\n\n',
    ]);
    expect(await events(response)).toEqual([
      { id: '42', event: 'run', data: '{"ok":true}' },
      { id: '7', data: '{"n":7}' },
    ]);
  });

  it('gives an event only the name and id of its own lines, never the last event\'s', async () => {
    const response = makeResponse(['event: run\nid: 1\ndata: a\n\ndata: b\n\n']);
    const [first, second] = await events(response);
    expect(first).toEqual({ event: 'run', id: '1', data: 'a' });
    expect(second).toEqual({ data: 'b' });
    expect('event' in second).toBe(false);
    expect('id' in second).toBe(false);
  });

  it('ignores an id that holds a NUL', async () => {
    const response = makeResponse(['id: a\0b\ndata: x\n\n']);
    expect(await events(response)).toEqual([{ data: 'x' }]);
  });

  it('drops an event the stream closes in the middle of', async () => {
    const response = makeResponse(['data: whole\n\ndata: cut off\n']);
    expect(await collect(response)).toEqual(['whole']);
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
    let cancelled = false;
    const stream = new ReadableStream<Uint8Array>({
      pull(ctrl) {
        if (index < chunks.length) {
          ctrl.enqueue(chunks[index++]);
        } else {
          ctrl.close();
        }
      },
      cancel() {
        cancelled = true;
      },
    });
    const response = new Response(stream);

    // Abort immediately after consuming the first event.
    const payloads: string[] = [];
    for await (const event of readSse(response, controller.signal)) {
      payloads.push(event.data);
      controller.abort();
    }

    // Only the first event should have been consumed — the reader checks
    // the abort signal before the next `reader.read()` call.
    expect(payloads).toHaveLength(1);
    expect(payloads[0]).toBe('{"n":1}');
    // And the body is cancelled, so nothing goes on reading it.
    expect(cancelled).toBe(true);
  });

  it('reads on past a fired signal it was not given, and leaves the body uncancelled', async () => {
    const controller = new AbortController();
    const encoder = new TextEncoder();
    const chunks = [encoder.encode('data: {"n":1}\n\n'), encoder.encode('data: {"n":2}\n\n')];
    let index = 0;
    let cancelled = false;
    const stream = new ReadableStream<Uint8Array>({
      pull(ctrl) {
        if (index < chunks.length) ctrl.enqueue(chunks[index++]);
        else ctrl.close();
      },
      cancel() {
        cancelled = true;
      },
    });

    const payloads: string[] = [];
    for await (const event of readSse(new Response(stream))) {
      payloads.push(event.data);
      controller.abort();
    }

    expect(payloads).toEqual(['{"n":1}', '{"n":2}']);
    expect(cancelled).toBe(false);
  });

  it('throws what the body throws when a read fails', async () => {
    const broken = new TypeError('network error');
    let sent = false;
    const stream = new ReadableStream<Uint8Array>({
      pull(ctrl) {
        if (sent) return ctrl.error(broken);
        sent = true;
        ctrl.enqueue(new TextEncoder().encode('data: {"n":1}\n\n'));
      },
    });

    const payloads: string[] = [];
    const reading = (async () => {
      for await (const event of readSse(new Response(stream))) payloads.push(event.data);
    })();

    await expect(reading).rejects.toBe(broken);
    expect(payloads).toEqual(['{"n":1}']);
  });
});

describe('createSSEStream', () => {
  it('opens the URL with the headers and signal it was given, and reads the reply with readSse', async () => {
    const fetchMock = vi.fn(async () => makeResponse(['data: {"n":1}\r\n\r\n']));
    vi.stubGlobal('fetch', fetchMock);
    try {
      const signal = new AbortController().signal;
      const all: SseEvent[] = [];
      for await (const event of createSSEStream('http://127.0.0.1:8080/s', { headers: { Authorization: 'Bearer k' }, signal })) {
        all.push(event);
      }

      expect(all).toEqual([{ data: '{"n":1}' }]);
      expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:8080/s', { headers: { Authorization: 'Bearer k' }, signal });
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('throws the status of a stream that was refused', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('no', { status: 401, statusText: 'Unauthorized' })));
    try {
      const reading = (async () => {
        for await (const event of createSSEStream('http://127.0.0.1:8080/s')) void event;
      })();

      await expect(reading).rejects.toBeInstanceOf(SSEHttpError);
      await expect(reading).rejects.toMatchObject({ status: 401, message: 'SSE request failed: 401 Unauthorized' });
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
