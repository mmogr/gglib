/**
 * Tests for `streamSse`, the POST whose reply is an event stream: llama
 * install and update. It is the session's authenticated fetch and the shared
 * reader behind three callbacks, so what is pinned here is the callbacks: a
 * frame for each event, one clean close, an error in the daemon's words, and
 * silence when the caller aborts.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { setApiSession } from '../../../src/services/transport/api/client';
import { streamSse, parseFrame } from '../../../src/services/transport/api/sse';

/** A Response whose body yields the given string chunks, in order. */
function streamingResponse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  return new Response(
    new ReadableStream<Uint8Array>({
      start(controller) {
        for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
        controller.close();
      },
    }),
    { status: 200, headers: { 'content-type': 'text/event-stream' } },
  );
}

/** A `fetch` that never answers, and rejects as `fetch` does when its signal fires. */
function hangingFetch(_url: string, init?: RequestInit): Promise<Response> {
  return new Promise((_resolve, reject) => {
    init?.signal?.addEventListener('abort', () => reject(init.signal!.reason));
  });
}

describe('streamSse', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiSession('', 'page-key');
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    setApiSession('', undefined);
  });

  it('decodes event name and data for each frame', async () => {
    fetchMock.mockResolvedValue(streamingResponse(['event: log\ndata: {"message":"hi"}\n\n']));

    const frames: { event: string; data: string }[] = [];
    const onClose = vi.fn();
    streamSse('/x', { onFrame: (f) => frames.push(f), onClose, onError: vi.fn() });
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());

    expect(frames).toEqual([{ event: 'log', data: '{"message":"hi"}' }]);
  });

  it('reassembles a frame split across chunk boundaries', async () => {
    fetchMock.mockResolvedValue(streamingResponse(['event: comp', 'leted\ndata: {"vers', 'ion":"b1"}\n\n']));

    const frames: { event: string; data: string }[] = [];
    const onClose = vi.fn();
    streamSse('/x', { onFrame: (f) => frames.push(f), onClose, onError: vi.fn() });
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());

    expect(frames).toEqual([{ event: 'completed', data: '{"version":"b1"}' }]);
  });

  it('gives a frame the stream did not name an empty event name, and skips a keepalive comment', async () => {
    fetchMock.mockResolvedValue(streamingResponse([':ping\n\n', 'data: {"type":"progress"}\n\n']));

    const frames: { event: string; data: string }[] = [];
    const onClose = vi.fn();
    streamSse('/x', { onFrame: (f) => frames.push(f), onClose, onError: vi.fn() });
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());

    expect(frames).toEqual([{ event: '', data: '{"type":"progress"}' }]);
  });

  it('posts to the path with the session\'s token, and a JSON body only when given one', async () => {
    fetchMock.mockImplementation(async () => streamingResponse([]));
    const onClose = vi.fn();

    streamSse('/api/config/system/update-llama', { onFrame: vi.fn(), onClose, onError: vi.fn() });
    streamSse('/api/config/system/install-llama', { onFrame: vi.fn(), onClose, onError: vi.fn() }, { force: true });
    await vi.waitFor(() => expect(onClose).toHaveBeenCalledTimes(2));

    const [bareUrl, bare] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(bareUrl).toBe('/api/config/system/update-llama');
    expect(bare.method).toBe('POST');
    expect(bare.headers).toEqual({ Accept: 'text/event-stream', Authorization: 'Bearer page-key' });
    expect(bare.body).toBeUndefined();

    const [, withBody] = fetchMock.mock.calls[1] as [string, RequestInit];
    expect(withBody.headers).toEqual({
      Accept: 'text/event-stream',
      'Content-Type': 'application/json',
      Authorization: 'Bearer page-key',
    });
    expect(withBody.body).toBe('{"force":true}');
  });

  it('reports a refused request in the daemon\'s own words, and never as a close', async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify({ error: 'llama.cpp is not installed', status: 500 }), {
        status: 500,
        statusText: 'Internal Server Error',
        headers: { 'content-type': 'application/json' },
      }),
    );

    const onError = vi.fn();
    const onClose = vi.fn();
    streamSse('/x', { onFrame: vi.fn(), onClose, onError });
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());

    expect(onError).toHaveBeenCalledWith('llama.cpp is not installed');
    expect(onClose).not.toHaveBeenCalled();
  });

  it('reports a refusal that carries no sentence by its status text', async () => {
    fetchMock.mockResolvedValue(new Response('boom', { status: 500, statusText: 'Internal Server Error' }));

    const onError = vi.fn();
    streamSse('/x', { onFrame: vi.fn(), onError });
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());

    expect(onError).toHaveBeenCalledWith('Internal Server Error');
  });

  it('reports a stream that breaks part-way, after the frames it had', async () => {
    let sent = false;
    fetchMock.mockResolvedValue(
      new Response(
        new ReadableStream<Uint8Array>({
          pull(controller) {
            if (sent) return controller.error(new TypeError('network error'));
            sent = true;
            controller.enqueue(new TextEncoder().encode('data: {"n":1}\n\n'));
          },
        }),
        { status: 200 },
      ),
    );

    const onFrame = vi.fn();
    const onError = vi.fn();
    const onClose = vi.fn();
    streamSse('/x', { onFrame, onClose, onError });
    await vi.waitFor(() => expect(onError).toHaveBeenCalled());

    expect(onFrame).toHaveBeenCalledWith({ event: '', data: '{"n":1}' });
    expect(onError).toHaveBeenCalledWith('network error');
    expect(onClose).not.toHaveBeenCalled();
  });

  it('stays quiet when the caller aborts', async () => {
    const abortError = new Error('aborted');
    abortError.name = 'AbortError';
    fetchMock.mockRejectedValue(abortError);

    const onError = vi.fn();
    const abort = streamSse('/x', { onFrame: vi.fn(), onError });
    abort();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onError).not.toHaveBeenCalled();
  });

  it('recognises the abort a real signal makes: no error, and no close', async () => {
    fetchMock.mockImplementation(hangingFetch);

    const onError = vi.fn();
    const onClose = vi.fn();
    const abort = streamSse('/x', { onFrame: vi.fn(), onClose, onError });
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
    abort();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect((fetchMock.mock.calls[0][1] as RequestInit).signal?.aborted).toBe(true);
    expect(onError).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('an abort made while a frame is being handled is still an abort: no close, no error', async () => {
    // A live stream, tied to the request's signal as a real one is.
    fetchMock.mockImplementation(async (_url: string, init: RequestInit) => {
      return new Response(
        new ReadableStream<Uint8Array>({
          start(body) {
            body.enqueue(new TextEncoder().encode('data: {"n":1}\n\n'));
            init.signal!.addEventListener('abort', () => body.error(init.signal!.reason));
          },
        }),
        { status: 200 },
      );
    });

    const onError = vi.fn();
    const onClose = vi.fn();
    const onFrame = vi.fn(() => abort());
    const abort = streamSse('/x', { onFrame, onClose, onError });
    await vi.waitFor(() => expect(onFrame).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onClose).not.toHaveBeenCalled();
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
