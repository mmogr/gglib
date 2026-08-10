/**
 * Tests for the shared POST-SSE reader. The interesting property is that a
 * frame may straddle a network chunk boundary — the naive version of this
 * loop drops the split frame, and all three llama endpoints depend on it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { streamSse, parseFrame } from '../../../src/services/transport/api/sse';

vi.mock('../../../src/services/transport/api/client', () => ({
  getApiBaseUrl: () => 'http://localhost:1234',
  getAuthHeaders: () => ({}),
}));

/** A Response whose body yields the given string chunks, in order. */
function streamingResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  let i = 0;
  return {
    ok: true,
    body: {
      getReader: () => ({
        read: () =>
          Promise.resolve(
            i < chunks.length
              ? { done: false, value: encoder.encode(chunks[i++]) }
              : { done: true, value: undefined },
          ),
      }),
    },
  } as unknown as Response;
}

/** Let the reader drain — each chunk costs a microtask turn. */
const drain = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('streamSse', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('decodes event name and data for each frame', async () => {
    vi.mocked(fetch).mockResolvedValue(
      streamingResponse(['event: log\ndata: {"message":"hi"}\n\n']),
    );

    const frames: { event: string; data: string }[] = [];
    streamSse('/x', { onFrame: (f) => frames.push(f), onError: vi.fn() });
    await drain();

    expect(frames).toEqual([{ event: 'log', data: '{"message":"hi"}' }]);
  });

  it('reassembles a frame split across chunk boundaries', async () => {
    vi.mocked(fetch).mockResolvedValue(
      streamingResponse(['event: comp', 'leted\ndata: {"vers', 'ion":"b1"}\n\n']),
    );

    const frames: { event: string; data: string }[] = [];
    streamSse('/x', { onFrame: (f) => frames.push(f), onError: vi.fn() });
    await drain();

    expect(frames).toEqual([{ event: 'completed', data: '{"version":"b1"}' }]);
  });

  it('reports a non-ok response as an error rather than silence', async () => {
    vi.mocked(fetch).mockResolvedValue({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
    } as Response);

    const onError = vi.fn();
    streamSse('/x', { onFrame: vi.fn(), onError });
    await drain();

    expect(onError).toHaveBeenCalledWith(expect.stringContaining('500'));
  });

  it('stays quiet when the caller aborts', async () => {
    const abortError = new Error('aborted');
    abortError.name = 'AbortError';
    vi.mocked(fetch).mockRejectedValue(abortError);

    const onError = vi.fn();
    const abort = streamSse('/x', { onFrame: vi.fn(), onError });
    abort();
    await drain();

    expect(onError).not.toHaveBeenCalled();
  });
});

describe('parseFrame', () => {
  it('returns undefined for malformed payloads instead of throwing', () => {
    expect(parseFrame({ event: 'log', data: '{not json' })).toBeUndefined();
  });

  it('parses a well-formed payload', () => {
    expect(parseFrame<{ a: number }>({ event: 'log', data: '{"a":1}' })).toEqual({ a: 1 });
  });
});
