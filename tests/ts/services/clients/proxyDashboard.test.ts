/**
 * Proxy dashboard client — credential handling.
 *
 * The reason this module was moved off the native `EventSource` is that
 * `EventSource` cannot send an `Authorization` header, and the proxy requires
 * one on `/v1/*` once a key is configured. These tests pin that: the header
 * goes out when there is a key, and stays off when there is not — sending an
 * empty one would break every unauthenticated loopback setup.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { clearProxyCache, subscribeProxyDashboard } from '../../../../src/services/clients/proxyDashboard';

vi.mock('../../../../src/services/platform', () => ({
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

/** An SSE response body that ends immediately, so the reader completes. */
function emptyStreamResponse(): Response {
  return new Response(new ReadableStream({ start: (controller) => controller.close() }), {
    status: 200,
    headers: { 'Content-Type': 'text/event-stream' },
  });
}

describe('proxyDashboard credentials', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  /** Extract the outgoing headers from whichever call shape was used. */
  function headersOf(callIndex = 0): Headers {
    const [, init] = fetchMock.mock.calls[callIndex] as [string, RequestInit];
    return new Headers(init.headers);
  }

  it('sends a bearer token on the dashboard stream when one is configured', async () => {
    fetchMock.mockResolvedValue(emptyStreamResponse());

    const unsubscribe = subscribeProxyDashboard('127.0.0.1', 8080, 'secret123', () => {});
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
    unsubscribe();

    expect(fetchMock.mock.calls[0][0]).toBe('http://127.0.0.1:8080/v1/proxy/status/stream');
    expect(headersOf().get('Authorization')).toBe('Bearer secret123');
  });

  /**
   * The common case. A proxy with no key configured must not be sent an
   * `Authorization` header at all.
   */
  it('sends no Authorization header when the proxy is unauthenticated', async () => {
    fetchMock.mockResolvedValue(emptyStreamResponse());

    const unsubscribe = subscribeProxyDashboard('127.0.0.1', 8080, null, () => {});
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
    unsubscribe();

    expect(headersOf().has('Authorization')).toBe(false);
  });

  it('decodes snapshots off the stream', async () => {
    fetchMock.mockResolvedValue(
      new Response(
        new ReadableStream({
          start(controller) {
            controller.enqueue(
              new TextEncoder().encode('data: {"total_requests":7}\n\n')
            );
            controller.close();
          },
        }),
        { status: 200 }
      )
    );

    const onSnapshot = vi.fn();
    const unsubscribe = subscribeProxyDashboard('127.0.0.1', 8080, null, onSnapshot);
    await vi.waitFor(() => expect(onSnapshot).toHaveBeenCalled());
    unsubscribe();

    expect(onSnapshot).toHaveBeenCalledWith({ total_requests: 7 });
  });

  it('unsubscribing aborts the request', async () => {
    fetchMock.mockResolvedValue(emptyStreamResponse());

    const unsubscribe = subscribeProxyDashboard('127.0.0.1', 8080, null, () => {});
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(init.signal?.aborted).toBe(false);
    unsubscribe();
    expect(init.signal?.aborted).toBe(true);
  });

  it('authenticates the cache-clear request and keeps the session header', async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify({ status: 'ok', message: 'cleared' }), { status: 200 })
    );

    await clearProxyCache('127.0.0.1', 8080, 'secret123', 'session-42');

    const headers = headersOf();
    expect(headers.get('Authorization')).toBe('Bearer secret123');
    expect(headers.get('X-Gglib-Session-Id')).toBe('session-42');
  });

  it('surfaces a rejected cache clear rather than swallowing it', async () => {
    fetchMock.mockResolvedValue(new Response('', { status: 401, statusText: 'Unauthorized' }));

    await expect(clearProxyCache('127.0.0.1', 8080, 'wrong')).rejects.toThrow('401');
  });
});
