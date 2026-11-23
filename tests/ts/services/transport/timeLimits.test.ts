/**
 * A request to the daemon that is never answered ends.
 *
 * Nothing limited how long a request could wait. A daemon stuck inside a
 * handler (a probe that never returns, B7 in #1959) left the page waiting on
 * a read for ever, and the health probe that decides whether the service is
 * up never answered either way, so a hung daemon was drawn as neither down
 * nor up. The limits, all set in one place under every request:
 *
 * - a read (GET) ends after 30 s with a timeout the page can say, except a
 *   read the daemon answers from the Hugging Face Hub or the paired machine,
 *   which can legitimately take longer and is given its own limit;
 * - the health probe gives up after 1.5 s, and the daemon reads as down;
 * - a stream is limited until its response headers arrive, and never after,
 *   however long it runs;
 * - a mutation (POST, PUT, PATCH, DELETE) has none, since one that took
 *   effect and answered late would be reported as failed, unless the caller
 *   sets one;
 * - a caller's own signal still stops a request, and is told apart from a
 *   timeout.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { startPerfRun } from '../../../../src/services/clients/benchmark';
import { apiFetch, del, get, patch, post, put, resetClientCache, setApiSession } from '../../../../src/services/transport/api/client';
import { probeDaemon } from '../../../../src/services/transport/api/health';
import { fetchAttachmentBlob } from '../../../../src/services/transport/api/attachments';
import { readFarRunEvents } from '../../../../src/services/transport/api/farChats';
import { checkModelUpgrade } from '../../../../src/services/transport/api/models/local';
import { getHfModelSummary, getHfPick, getHfQuantizations } from '../../../../src/services/transport/api/models/hf';
import { checkModelUpdates } from '../../../../src/services/transport/api/verification';
import { readRunEvents } from '../../../../src/services/transport/api/runs';
import { streamEngineInstall } from '../../../../src/services/transport/api/setup';
import { TransportError } from '../../../../src/services/transport/errors';
import { formatError, isAbortError } from '../../../../src/utils/errors';

const MINUTE = 60_000;
const HUB_MODEL = 'unsloth/some-model-GGUF' as never;

/** Read a stream to its end, throwing what it throws. */
async function drain(items: AsyncGenerator<unknown>): Promise<void> {
  for await (const item of items) void item;
}

/** What a promise has done so far, read without awaiting it. */
function watch<T>(promise: Promise<T>): { state: 'pending' | 'resolved' | 'rejected'; value?: T; error?: unknown } {
  const seen: { state: 'pending' | 'resolved' | 'rejected'; value?: T; error?: unknown } = { state: 'pending' };
  promise.then(
    (value) => Object.assign(seen, { state: 'resolved', value }),
    (error: unknown) => Object.assign(seen, { state: 'rejected', error }),
  );
  return seen;
}

/** Rejects as `fetch` does when `signal` fires: with the signal's reason. */
function onAbort(signal: AbortSignal | null | undefined, reject: (reason: unknown) => void): void {
  if (!signal) return;
  if (signal.aborted) reject(signal.reason);
  signal.addEventListener('abort', () => reject(signal.reason), { once: true });
}

/** A `fetch` that never answers, and stops only when its signal fires. */
function neverAnswers(_url: string, init?: RequestInit): Promise<Response> {
  return new Promise((_resolve, reject) => onAbort(init?.signal, reject));
}

/**
 * An accepted reply at once whose body never ends, and errors when the
 * request's signal fires, as a real one does: a limit left running on it
 * would cut it.
 */
function bodyNeverEnds(_url: string, init?: RequestInit): Promise<Response> {
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      onAbort(init?.signal, (reason) => controller.error(reason));
    },
  });
  return Promise.resolve(new Response(body, { status: 200, headers: { 'Content-Type': 'application/json' } }));
}

/**
 * An accepted event stream at once that sends `frames`, one every `gapMs`,
 * then ends; it errors when the request's signal fires.
 */
function slowStream(frames: string[], gapMs: number) {
  return (_url: string, init?: RequestInit): Promise<Response> => {
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        onAbort(init?.signal, (reason) => controller.error(reason));
        frames.forEach((frame, i) => {
          setTimeout(() => {
            if (init?.signal?.aborted) return;
            controller.enqueue(encoder.encode(frame));
            if (i === frames.length - 1) controller.close();
          }, gapMs * (i + 1));
        });
      },
    });
    return Promise.resolve(new Response(body, { status: 200, headers: { 'Content-Type': 'text/event-stream' } }));
  };
}

function expectTimeout(error: unknown): void {
  expect(TransportError.hasCode(error, 'TIMEOUT')).toBe(true);
  // Not an abort: a reader that drops its own aborts silently must still say this one.
  expect(isAbortError(error)).toBe(false);
}

describe('time limits on requests to the daemon', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    fetchMock = vi.fn(neverAnswers);
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    resetClientCache();
  });

  describe('a read', () => {
    it('that is never answered ends after 30 s with a timeout', async () => {
      const read = watch(get('/api/models'));

      await vi.advanceTimersByTimeAsync(29_999);
      expect(read.state).toBe('pending');

      await vi.advanceTimersByTimeAsync(1);
      expect(read.state).toBe('rejected');
      expectTimeout(read.error);
    });

    it('says so in words the page can show', async () => {
      const read = watch(get('/api/config/system/engines/llama'));
      await vi.advanceTimersByTimeAsync(30_000);

      expect(read.state).toBe('rejected');
      expect(formatError(read.error)).toBe('gglib did not answer in 30 seconds.');
    });

    it('whose body never ends is a timeout too, not a body that would not decode', async () => {
      fetchMock.mockImplementation(bodyNeverEnds);
      const read = watch(get('/api/models'));

      await vi.advanceTimersByTimeAsync(30_000);
      expect(read.state).toBe('rejected');
      expectTimeout(read.error);
    });

    it('through apiFetch ends after 30 s as well', async () => {
      const read = watch(apiFetch('/api/attachments/abc'));

      await vi.advanceTimersByTimeAsync(30_000);
      expect(read.state).toBe('rejected');
      expectTimeout(read.error);
    });

    it('that is answered in time is not cut off later', async () => {
      fetchMock.mockImplementation(async () => new Response('[]', { status: 200 }));
      const read = watch(get('/api/models'));

      await vi.advanceTimersByTimeAsync(10 * MINUTE);
      expect(read).toEqual({ state: 'resolved', value: [] });
    });
  });

  describe.each([
    ['a model\'s summary from the Hub', () => getHfModelSummary(HUB_MODEL), 5 * MINUTE, '5 minutes'],
    ['a model\'s quantizations from the Hub', () => getHfQuantizations(HUB_MODEL), 5 * MINUTE, '5 minutes'],
    ['the pick from the Hub', () => getHfPick(HUB_MODEL), 5 * MINUTE, '5 minutes'],
    ['a model\'s update check against the Hub', () => checkModelUpdates(1 as never), 5 * MINUTE, '5 minutes'],
    ['a model\'s upgrade check against the Hub', () => checkModelUpgrade(1), 5 * MINUTE, '5 minutes'],
    ['an image from the paired machine', () => fetchAttachmentBlob('far', 'abc'), 90_000, '90 seconds'],
    ['a far run\'s events', () => drain(readFarRunEvents('r1', 0, new AbortController().signal)), 90_000, '90 seconds'],
  ] as const)('%s, which can legitimately take longer than 30 s', (_name, read, limit, said) => {
    it('is given its own limit', async () => {
      const asked = watch(read() as Promise<unknown>);

      await vi.advanceTimersByTimeAsync(limit - 1);
      expect(asked.state).toBe('pending');

      await vi.advanceTimersByTimeAsync(1);
      expect(asked.state).toBe('rejected');
      expect(formatError(asked.error)).toBe(`gglib did not answer in ${said}.`);
    });
  });

  describe('the health probe', () => {
    it('gives up after 1.5 s and reads the daemon as not there', async () => {
      const probe = watch(probeDaemon());

      await vi.advanceTimersByTimeAsync(1_499);
      expect(probe.state).toBe('pending');

      await vi.advanceTimersByTimeAsync(1);
      expect(probe).toEqual({ state: 'resolved', value: false });
    });
  });

  describe('a stream', () => {
    it('whose headers never come ends after 30 s with a timeout', async () => {
      const run = watch(startPerfRun({} as never, () => {}));

      await vi.advanceTimersByTimeAsync(29_999);
      expect(run.state).toBe('pending');

      await vi.advanceTimersByTimeAsync(1);
      expect(run.state).toBe('rejected');
      expectTimeout(run.error);
    });

    it('whose headers never come is said to the install as a timeout, not dropped as an abort', async () => {
      const onError = vi.fn();
      streamEngineInstall('llama', () => {}, onError);

      await vi.advanceTimersByTimeAsync(30_000);
      expect(onError).toHaveBeenCalledWith('gglib did not answer in 30 seconds.');
    });

    it('that a POST opened is not cut off once its headers came, however long it runs', async () => {
      fetchMock.mockImplementation(slowStream(['data: {"n":1}\n\n', 'data: {"n":2}\n\n'], 5 * MINUTE));
      const events: unknown[] = [];
      const run = watch(startPerfRun({} as never, (event) => events.push(event)));

      await vi.advanceTimersByTimeAsync(10 * MINUTE);
      expect(run.state).toBe('resolved');
      expect(events).toEqual([{ n: 1 }, { n: 2 }]);
    });

    it('that a GET opened is not cut off once its headers came, however long it runs', async () => {
      fetchMock.mockImplementation(slowStream(['event: run\ndata: {"id":"r1"}\n\n'], 10 * MINUTE));
      const controller = new AbortController();
      const items = (async () => {
        const seen: unknown[] = [];
        for await (const item of readRunEvents('r1', 0, controller.signal)) seen.push(item);
        return seen;
      })();
      const read = watch(items);

      await vi.advanceTimersByTimeAsync(11 * MINUTE);
      expect(read.state).toBe('resolved');
      expect(read.value).toEqual([{ type: 'end', info: { id: 'r1' } }]);
    });
  });

  describe.each([
    ['POST', () => post('/api/servers/start', {})],
    ['PUT', () => put('/api/config/settings', {})],
    ['PATCH', () => patch('/api/conversations/1', {})],
    ['DELETE', () => del('/api/models/1')],
    ['an apiFetch POST', () => apiFetch('/api/attachments', { method: 'POST', body: 'x' })],
  ] as const)('a mutation (%s)', (_name, send) => {
    it('has no limit of its own', async () => {
      const sent = watch(send() as Promise<unknown>);

      await vi.advanceTimersByTimeAsync(30 * MINUTE);
      expect(sent.state).toBe('pending');
    });
  });

  it('a mutation is limited when its caller sets a limit', async () => {
    const sent = watch(post('/api/chat', {}, { timeoutMs: 5_000 }));

    await vi.advanceTimersByTimeAsync(4_999);
    expect(sent.state).toBe('pending');

    await vi.advanceTimersByTimeAsync(1);
    expect(sent.state).toBe('rejected');
    expectTimeout(sent.error);
  });

  describe('a caller\'s own signal', () => {
    it('still stops a read, as an abort and not a timeout', async () => {
      const controller = new AbortController();
      const read = watch(apiFetch('/api/attachments/abc', { signal: controller.signal }));

      await vi.advanceTimersByTimeAsync(1_000);
      controller.abort();
      await vi.advanceTimersByTimeAsync(0);

      expect(read.state).toBe('rejected');
      expect(isAbortError(read.error)).toBe(true);
      expect(TransportError.hasCode(read.error, 'TIMEOUT')).toBe(false);
    });

    it('still stops a stream after its headers came, as an abort', async () => {
      fetchMock.mockImplementation(bodyNeverEnds);
      const controller = new AbortController();
      const run = watch(startPerfRun({} as never, () => {}, controller.signal));

      await vi.advanceTimersByTimeAsync(5 * MINUTE);
      expect(run.state).toBe('pending');
      controller.abort();
      await vi.advanceTimersByTimeAsync(0);

      expect(run.state).toBe('rejected');
      expect(isAbortError(run.error)).toBe(true);
    });
  });
});
