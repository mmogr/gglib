/**
 * The benchmark client's four streams (compare, perf, tune, agentic eval)
 * and its gated apply.
 *
 * The four were one fetch-and-read block written four times, with an error
 * shape of their own. They are now one function over the transport's
 * authenticated fetch and the shared reader, so a refusal is the transport's
 * `TransportError` and a broken stream rejects as a run's stream does. Both
 * are checked here against the transport's own run reader, fed the same
 * replies.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import {
  applyTuneRun,
  startAgenticRun,
  startCompareRun,
  startPerfRun,
  startTuneRun,
} from '../../../../src/services/clients/benchmark';
import { setApiSession } from '../../../../src/services/transport/api/client';
import { readRunEvents } from '../../../../src/services/transport/api/runs';
import { TransportError } from '../../../../src/services/transport/errors';
import type { BenchmarkEvent } from '../../../../src/types/benchmark';
import { isAbortError } from '../../../../src/utils/errors';

type Start = (config: never, onEvent: (event: BenchmarkEvent) => void, signal?: AbortSignal) => Promise<void>;

/** Each stream with the route it posts to. */
const STREAMS: [name: string, start: Start, path: string][] = [
  ['compare', startCompareRun as Start, '/api/benchmark/compare'],
  ['perf', startPerfRun as Start, '/api/benchmark/perf'],
  ['tune', startTuneRun as Start, '/api/benchmark/tune'],
  ['agentic eval', startAgenticRun as Start, '/api/benchmark/agentic'],
];

const config = { model_ids: [1], prompt: 'hi' } as never;

function stream(chunks: (string | Uint8Array)[]): Response {
  const encoder = new TextEncoder();
  return new Response(
    new ReadableStream<Uint8Array>({
      start(controller) {
        for (const chunk of chunks) controller.enqueue(typeof chunk === 'string' ? encoder.encode(chunk) : chunk);
        controller.close();
      },
    }),
    { status: 200, headers: { 'content-type': 'text/event-stream' } },
  );
}

/** A reply that sends one event and then breaks. */
function brokenStream(error: unknown): Response {
  let sent = false;
  return new Response(
    new ReadableStream<Uint8Array>({
      pull(controller) {
        if (sent) return controller.error(error);
        sent = true;
        controller.enqueue(new TextEncoder().encode('id: 1\ndata: {"type":"run_started","run_id":1}\n\n'));
      },
    }),
    { status: 200 },
  );
}

function refusal(status: number, body: unknown, statusText = ''): Response {
  return new Response(typeof body === 'string' ? body : JSON.stringify(body), {
    status,
    statusText,
    headers: { 'content-type': typeof body === 'string' ? 'text/plain' : 'application/json' },
  });
}

/** What reading a run rejects with, for the same reply: the transport's shape. */
async function transportRejection(reply: () => Response, fetchMock: ReturnType<typeof vi.fn>): Promise<unknown> {
  fetchMock.mockImplementationOnce(async () => reply());
  try {
    for await (const item of readRunEvents('r1', 0, new AbortController().signal)) void item;
  } catch (error) {
    return error;
  }
  throw new Error('the run read did not reject');
}

describe('benchmark streams', () => {
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

  describe.each(STREAMS)('%s', (_name, start, path) => {
    it('posts its config as JSON to its own route, with the session\'s token and the caller\'s signal', async () => {
      fetchMock.mockResolvedValue(stream([]));
      const signal = new AbortController().signal;

      await start(config, vi.fn(), signal);

      expect(fetchMock).toHaveBeenCalledTimes(1);
      const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
      expect(url).toBe(path);
      expect(init.method).toBe('POST');
      expect(init.headers).toEqual({ 'Content-Type': 'application/json', Authorization: 'Bearer page-key' });
      expect(JSON.parse(init.body as string)).toEqual({ model_ids: [1], prompt: 'hi' });
      expect(init.signal).toBe(signal);
    });

    it('hands over each event in order and resolves when the stream ends', async () => {
      fetchMock.mockResolvedValue(
        stream([
          ':ping\n\n',
          'data: {"type":"run_started","run_id":7}\n\n',
          'data: {"type":"run_comp',
          'lete","run_id":7}\n\n',
        ]),
      );
      const seen: BenchmarkEvent[] = [];

      await expect(start(config, (event) => seen.push(event))).resolves.toBeUndefined();

      expect(seen).toEqual([
        { type: 'run_started', run_id: 7 },
        { type: 'run_complete', run_id: 7 },
      ]);
    });

    it('reads a stream framed with CRLF, which it used to read nothing from', async () => {
      fetchMock.mockResolvedValue(stream(['data: {"type":"run_started","run_id":7}\r\n\r\n']));
      const seen: BenchmarkEvent[] = [];

      await start(config, (event) => seen.push(event));

      expect(seen).toEqual([{ type: 'run_started', run_id: 7 }]);
    });

    it('skips a payload that is not JSON and carries on', async () => {
      fetchMock.mockResolvedValue(stream(['data: ping\n\n', 'data: {"type":"run_started","run_id":7}\n\n']));
      const seen: BenchmarkEvent[] = [];

      await start(config, (event) => seen.push(event));

      expect(seen).toEqual([{ type: 'run_started', run_id: 7 }]);
    });

    it('rejects a refused request with the transport\'s coded error, as a run\'s read does', async () => {
      const reply = () => refusal(429, { error: 'a benchmark is already running', status: 429, type: 'benchmark_busy' });
      fetchMock.mockImplementationOnce(async () => reply());

      const error = await start(config, vi.fn()).catch((e: unknown) => e);

      expect(error).toBeInstanceOf(TransportError);
      expect(error).toMatchObject({
        name: 'TransportError',
        code: 'INTERNAL',
        message: 'a benchmark is already running',
        details: { status: 429, type: 'benchmark_busy' },
      });
      const transports = await transportRejection(reply, fetchMock);
      expect(transports).toBeInstanceOf(TransportError);
      expect({ ...(error as TransportError), message: (error as Error).message }).toEqual({
        ...(transports as TransportError),
        message: (transports as Error).message,
      });
    });

    it('rejects a refusal that carries no sentence with its status, coded the transport\'s way', async () => {
      const reply = () => refusal(404, 'not here', 'Not Found');
      fetchMock.mockImplementationOnce(async () => reply());

      const error = await start(config, vi.fn()).catch((e: unknown) => e);

      expect(error).toMatchObject({ name: 'TransportError', code: 'NOT_FOUND', message: 'Not Found', details: { status: 404 } });
      expect(error).toEqual(await transportRejection(reply, fetchMock));
    });

    it('rejects a stream that breaks part-way with what the read threw, after the events it had', async () => {
      const broken = new TypeError('network error');
      fetchMock.mockImplementationOnce(async () => brokenStream(broken));
      const seen: BenchmarkEvent[] = [];

      const error = await start(config, (event) => seen.push(event)).catch((e: unknown) => e);

      expect(seen).toEqual([{ type: 'run_started', run_id: 1 }]);
      expect(error).toBe(broken);
      expect(await transportRejection(() => brokenStream(broken), fetchMock)).toBe(broken);
    });

    it('rejects with an abort, recognised as one, when its signal fires', async () => {
      // As `fetch` does: reject with the signal's reason once it has fired.
      fetchMock.mockImplementation(
        (_url: string, init: RequestInit) =>
          new Promise((_resolve, reject) => {
            init.signal!.addEventListener('abort', () => reject(init.signal!.reason));
          }),
      );
      const controller = new AbortController();

      const running = start(config, vi.fn(), controller.signal).catch((e: unknown) => e);
      await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
      controller.abort();

      expect(isAbortError(await running)).toBe(true);
    });

    it('rejects with an abort when its signal fires while the stream is being read', async () => {
      const controller = new AbortController();
      fetchMock.mockImplementation(async (_url: string, init: RequestInit) => {
        return new Response(
          new ReadableStream<Uint8Array>({
            start(body) {
              body.enqueue(new TextEncoder().encode('data: {"type":"run_started","run_id":7}\n\n'));
              init.signal!.addEventListener('abort', () => body.error(init.signal!.reason));
            },
          }),
          { status: 200 },
        );
      });
      const seen: BenchmarkEvent[] = [];

      const running = start(config, (event) => seen.push(event), controller.signal).catch((e: unknown) => e);
      await vi.waitFor(() => expect(seen).toHaveLength(1));
      controller.abort();

      expect(isAbortError(await running)).toBe(true);
    });

    it('rejects with an abort, and never resolves, when its signal fires while an event is being handled', async () => {
      // A run the screen has replaced must not look like one that ended cleanly.
      const controller = new AbortController();
      fetchMock.mockImplementation(async (_url: string, init: RequestInit) => {
        return new Response(
          new ReadableStream<Uint8Array>({
            start(body) {
              body.enqueue(new TextEncoder().encode('data: {"type":"run_started","run_id":7}\n\n'));
              init.signal!.addEventListener('abort', () => body.error(init.signal!.reason));
            },
          }),
          { status: 200 },
        );
      });

      const outcome = await start(config, () => controller.abort(), controller.signal).then(
        () => 'resolved',
        (e: unknown) => e,
      );

      expect(outcome).not.toBe('resolved');
      expect(isAbortError(outcome)).toBe(true);
    });
  });
});

describe('applyTuneRun', () => {
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

  it('is a plain POST through the client, and answers the daemon\'s verdict', async () => {
    const outcome = { verdict: { verdict: 'apply' }, model_id: 3, applied: true };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(outcome), { status: 200, headers: { 'content-type': 'application/json' } }),
    );

    await expect(applyTuneRun(12)).resolves.toEqual(outcome);

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/benchmark/tune/12/apply');
    expect(init.method).toBe('POST');
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer page-key');
  });

  it('rejects a refusal with the transport\'s coded error', async () => {
    fetchMock.mockResolvedValue(refusal(404, { error: 'no tune run has id 12', status: 404 }));

    const error = await applyTuneRun(12).catch((e: unknown) => e);

    expect(error).toBeInstanceOf(TransportError);
    expect(error).toMatchObject({ code: 'NOT_FOUND', message: 'no tune run has id 12' });
  });
});
