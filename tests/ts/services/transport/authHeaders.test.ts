import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook } from '@testing-library/react';

import { useServerMetrics } from '../../../../src/components/ConsoleInfoPanel/useServerMetrics';
import {
  applyTuneRun,
  startAgenticRun,
  startCompareRun,
  startPerfRun,
  startTuneRun,
} from '../../../../src/services/clients/benchmark';
import { clearProxyCache, subscribeProxyDashboard } from '../../../../src/services/clients/proxyDashboard';
import { getServerLogs, listenToServerLogs } from '../../../../src/services/platform/serverLogs';
import { fetchAttachmentBlob, uploadAttachment } from '../../../../src/services/transport/api/attachments';
import { generateChatTitle } from '../../../../src/services/transport/api/chat';
import {
  apiFetch,
  get,
  getAuthenticatedFetchConfig,
  resetClientCache,
  setApiSession,
} from '../../../../src/services/transport/api/client';
import { readFarRunEvents } from '../../../../src/services/transport/api/farChats';
import { readRunEvents } from '../../../../src/services/transport/api/runs';
import { streamLlamaInstall, streamLlamaUpdate } from '../../../../src/services/transport/api/setup';
import { SSEConnectionManager } from '../../../../src/services/transport/events/sse';
import { TransportError } from '../../../../src/services/transport/errors';

/**
 * Every request to the daemon presents the session's credential, and no
 * request to anything else does.
 *
 * There were three ways to build a raw `fetch` to the daemon, and they
 * disagreed: one read the token before the client had it, one asked Tauri for
 * the port on every call, and for a while one sent no credential at all, so
 * against a `--share-lan` daemon agent chat and the benchmark answered 401
 * once the key was entered while every other call succeeded. There is now
 * one, `apiFetch`, and this is the list of what goes through it.
 */

const noop = () => {};

/** Read a stream to its end, whatever it does. */
async function drain(events: AsyncGenerator<unknown>): Promise<void> {
  try {
    for await (const event of events) void event;
  } catch {
    // only the request's headers are under test
  }
}

const question = { id: 1, conversation_id: 1, role: 'user' as const, content: 'hi', created_at: '2026-10-06T00:00:00Z' };
const png = new Blob(['x'], { type: 'image/png' });

/**
 * Each raw fetch to the daemon: what starts it, and the path it asks for.
 * `start` may answer a function that stops a stream that reconnects.
 */
const DAEMON_FETCHES: [name: string, path: string, start: (signal: AbortSignal) => unknown][] = [
  ['a JSON request', '/api/models', () => get('/api/models').catch(noop)],
  ['apiFetch itself', '/api/anything', () => apiFetch('/api/anything').catch(noop)],
  ['the compare stream', '/api/benchmark/compare', (signal) => startCompareRun({} as never, noop, signal).catch(noop)],
  ['the perf stream', '/api/benchmark/perf', (signal) => startPerfRun({} as never, noop, signal).catch(noop)],
  ['the tune stream', '/api/benchmark/tune', (signal) => startTuneRun({} as never, noop, signal).catch(noop)],
  ['the agentic eval stream', '/api/benchmark/agentic', (signal) => startAgenticRun({} as never, noop, signal).catch(noop)],
  ['the gated tune apply', '/api/benchmark/tune/4/apply', () => applyTuneRun(4).catch(noop)],
  ['a run\'s events', '/api/runs/r1/events?after=0', (signal) => drain(readRunEvents('r1', 0, signal))],
  ['a far run\'s events', '/api/remote/runs/r1/events?after=0', (signal) => drain(readFarRunEvents('r1', 0, signal))],
  ['an image upload', '/api/attachments', () => uploadAttachment('this', png).catch(noop)],
  ['a far image upload', '/api/remote/attachments', () => uploadAttachment('far', png).catch(noop)],
  ['an image read', '/api/attachments/abc', () => fetchAttachmentBlob('this', 'abc').catch(noop)],
  ['a far image read', '/api/remote/attachments/abc', () => fetchAttachmentBlob('far', 'abc').catch(noop)],
  ['the llama install stream', '/api/config/system/install-llama', () => streamLlamaInstall(noop, noop)],
  ['the llama update stream', '/api/config/system/update-llama', () => streamLlamaUpdate(noop, noop)],
  ['the server log snapshot', '/api/servers/9001/logs', () => getServerLogs(9001).catch(noop)],
  ['the server log stream', '/api/servers/9001/logs/stream', () => listenToServerLogs(9001, noop)],
  ['the event bus', '/api/events', () => new SSEConnectionManager('/api/events').subscribe(noop)],
  ['the chat title request', '/api/chat', () => generateChatTitle({ serverPort: 9000, messages: [question] }).catch(noop)],
];

/** Each raw fetch that is not to the daemon: its URL, and what starts it. */
const OTHER_FETCHES: [name: string, url: string, start: () => unknown][] = [
  ['the proxy dashboard stream', 'http://127.0.0.1:8080/v1/proxy/status/stream', () => subscribeProxyDashboard('127.0.0.1', 8080, null, noop)],
  ['the proxy cache clear', 'http://127.0.0.1:8080/v1/proxy/cache/clear', () => clearProxyCache('127.0.0.1', 8080, null).catch(noop)],
  ['the llama-server metrics poll', 'http://127.0.0.1:9000/metrics', () => renderHook(() => useServerMetrics(9000, true)).unmount],
];

describe('API auth headers', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let controller: AbortController;
  let stops: unknown[];

  /** Start a fetch, wait for its request, stop whatever it left running, and answer the request. */
  async function requestOf(start: (signal: AbortSignal) => unknown): Promise<[string, RequestInit | undefined]> {
    stops.push(await start(controller.signal));
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
    return fetchMock.mock.calls[0] as [string, RequestInit | undefined];
  }

  const authOf = (init: RequestInit | undefined) => new Headers(init?.headers).get('Authorization');

  beforeEach(() => {
    // An accepted reply with nothing in it. A stream that reconnects is left
    // open, as a real one is; anything else ends at once.
    fetchMock = vi.fn(async (url: string) => {
      const staysOpen = url.endsWith('/api/events') || url.endsWith('/stream');
      return new Response(new ReadableStream({ start: (c) => (staysOpen ? undefined : c.close()) }), { status: 200 });
    });
    vi.stubGlobal('fetch', fetchMock);
    controller = new AbortController();
    stops = [];
    resetClientCache();
    setApiSession('', undefined);
  });

  afterEach(() => {
    controller.abort();
    for (const stop of stops) if (typeof stop === 'function') stop();
    vi.unstubAllGlobals();
    resetClientCache();
    setApiSession('', undefined);
  });

  describe.each(DAEMON_FETCHES)('%s', (_name, path, start) => {
    it('carries the session\'s token to the daemon', async () => {
      setApiSession('', 'lan-daemon-key');

      const [url, init] = await requestOf(start);

      expect(url).toBe(path);
      expect(authOf(init)).toBe('Bearer lan-daemon-key');
    });

    it('carries no Authorization header when the session has no token', async () => {
      const [url, init] = await requestOf(start);

      expect(url).toBe(path);
      expect(new Headers(init?.headers).has('Authorization')).toBe(false);
    });
  });

  describe.each(OTHER_FETCHES)('%s', (_name, target, start) => {
    it('is not the daemon\'s, and is never sent the session\'s token', async () => {
      setApiSession('', 'lan-daemon-key');

      const [url, init] = await requestOf(start);

      expect(url).toBe(target);
      expect(new Headers(init?.headers).has('Authorization')).toBe(false);
    });
  });

  it('sends the proxy its own key, not the session\'s', async () => {
    setApiSession('', 'lan-daemon-key');

    const [, init] = await requestOf(() => subscribeProxyDashboard('127.0.0.1', 8080, 'proxy-key', noop));

    expect(authOf(init)).toBe('Bearer proxy-key');
  });

  it('drops the header again when the session is cleared', async () => {
    setApiSession('', 'lan-daemon-key');
    setApiSession('', undefined);

    await apiFetch('/api/anything');

    expect(new Headers((fetchMock.mock.calls[0][1] as RequestInit).headers).has('Authorization')).toBe(false);
    expect((await getAuthenticatedFetchConfig()).headers).toEqual({});
  });

  it('adds the token over the caller\'s own headers, which it keeps', async () => {
    setApiSession('', 'lan-daemon-key');

    await apiFetch('/api/anything', {
      method: 'POST',
      headers: { 'Content-Type': 'image/png', Authorization: 'Bearer not-this' },
      body: png,
    });

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(init.method).toBe('POST');
    expect(init.body).toBe(png);
    expect(init.headers).toEqual({ 'Content-Type': 'image/png', Authorization: 'Bearer lan-daemon-key' });
  });

  it('hands back the response the daemon accepted, and throws its coded error for one it refused', async () => {
    const accepted = new Response('bytes', { status: 200 });
    fetchMock.mockResolvedValueOnce(accepted);
    await expect(apiFetch('/api/anything')).resolves.toBe(accepted);

    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ error: 'no such thing', status: 404, type: 'thing_not_found' }), {
        status: 404,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const refused = await apiFetch('/api/anything').catch((e: unknown) => e);
    expect(refused).toBeInstanceOf(TransportError);
    expect(refused).toMatchObject({ code: 'NOT_FOUND', message: 'no such thing', details: { status: 404, type: 'thing_not_found' } });
  });

  describe('in the desktop app', () => {
    let invoke: ReturnType<typeof vi.fn>;

    beforeEach(() => {
      invoke = vi.fn().mockResolvedValue({ port: 9887, token: 'desk-token' });
      Object.assign(window, { __TAURI_INTERNALS__: { invoke } });
    });

    afterEach(() => {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    });

    it('reaches the daemon with its token on the very first call, before any other has run', async () => {
      await apiFetch('/api/runs/r1/events?after=0');

      const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
      expect(url).toBe('http://127.0.0.1:9887/api/runs/r1/events?after=0');
      expect(authOf(init)).toBe('Bearer desk-token');
    });

    it('asks the desktop where the daemon is once, not on every call', async () => {
      await apiFetch('/api/a');
      await apiFetch('/api/b');
      await getAuthenticatedFetchConfig();

      expect(invoke).toHaveBeenCalledTimes(1);
      expect(invoke).toHaveBeenCalledWith('get_embedded_api_info');
      expect(fetchMock.mock.calls.map((call) => call[0])).toEqual([
        'http://127.0.0.1:9887/api/a',
        'http://127.0.0.1:9887/api/b',
      ]);
    });
  });
});
