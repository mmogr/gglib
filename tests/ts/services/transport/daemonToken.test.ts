import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

/**
 * The page keeps the token the link `gglib web` prints carries, and sends it.
 *
 * Every `/api` route asks for the daemon's token, on loopback too, so a page
 * opened from that link must hold it, must not leave it in the address bar,
 * and must not answer the daemon asking for it by asking the person for an
 * API key, which no loopback daemon takes.
 */

const SENTENCE =
  "this route needs the daemon's token: run the command from `gglib` on this machine, " +
  'or open the page from the link `gglib web` prints';

/** Fresh modules each time: the client and the token are kept at module level. */
async function client() {
  vi.resetModules();
  return import('../../../../src/services/transport/api/client');
}

async function tokenModule() {
  vi.resetModules();
  return import('../../../../src/services/transport/api/daemonToken');
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

const refusal = () => json({ error: SENTENCE, status: 401, type: 'DAEMON_TOKEN_REQUIRED' }, 401);

/** The `Authorization` header of the `n`th request sent. */
function authOf(fetchMock: ReturnType<typeof vi.fn>, n = 0): string | undefined {
  const init = fetchMock.mock.calls[n][1] as RequestInit;
  return (init.headers as Record<string, string>)['Authorization'];
}

describe('the daemon token from the link', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    localStorage.clear();
    window.history.replaceState(null, '', '/');
    fetchMock = vi.fn(async () => json({ success: true, data: [] }));
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('is read from the fragment, kept, and stripped from the address', async () => {
    window.history.replaceState(null, '', '/models?view=grid#token=abc123');
    const { takeDaemonToken } = await tokenModule();

    expect(takeDaemonToken()).toBe('abc123');
    expect(localStorage.getItem('gglib_daemon_token')).toBe('abc123');
    expect(window.location.hash).toBe('');
    expect(window.location.pathname + window.location.search).toBe('/models?view=grid');
    expect(window.location.href).not.toContain('abc123');
  });

  it('is sent as the bearer on every /api call, and never in the URL', async () => {
    window.history.replaceState(null, '', '/#token=abc123');
    const api = await client();

    await api.get('/api/models');
    await api.post('/api/remote/invite');

    expect(fetchMock).toHaveBeenCalledTimes(2);
    for (const n of [0, 1]) {
      expect(authOf(fetchMock, n)).toBe('Bearer abc123');
      expect(String(fetchMock.mock.calls[n][0])).not.toContain('abc123');
    }
  });

  it('is still sent when storage refuses to keep it', async () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('blocked', 'SecurityError');
    });
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new DOMException('blocked', 'SecurityError');
    });
    window.history.replaceState(null, '', '/#token=abc123');
    const api = await client();
    const { takeDaemonToken } = await import('../../../../src/services/transport/api/daemonToken');

    takeDaemonToken(); // what main.tsx does before anything renders
    expect(window.location.hash).toBe('');
    await api.get('/api/models');

    expect(authOf(fetchMock)).toBe('Bearer abc123');
  });

  it('outlives a reload, and a bookmark, from localStorage', async () => {
    localStorage.setItem('gglib_daemon_token', 'abc123');
    const api = await client();

    await api.get('/api/models');

    expect(authOf(fetchMock)).toBe('Bearer abc123');
  });

  it('is not invented: a page opened without one sends no credential', async () => {
    const api = await client();

    await api.get('/api/models');

    expect(authOf(fetchMock)).toBeUndefined();
  });

  it("shows a loopback daemon's refusal in its own words, and asks for no API key", async () => {
    fetchMock.mockImplementation(async () => refusal());
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue('some-key');
    const api = await client();

    await expect(api.get('/api/models')).rejects.toThrow(SENTENCE);
    await expect(api.post('/api/remote/invite')).rejects.toThrow(SENTENCE);

    expect(prompt).not.toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  describe('in the desktop app, after its service restarted', () => {
    let invoke: ReturnType<typeof vi.fn>;

    beforeEach(() => {
      invoke = vi
        .fn()
        .mockResolvedValueOnce({ port: 9887, token: 'old-token' })
        .mockResolvedValue({ port: 9887, token: 'new-token' });
      Object.assign(window, { __TAURI_INTERNALS__: { invoke } });
    });

    afterEach(() => {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    });

    it('reads the token again and retries once with it', async () => {
      fetchMock.mockImplementationOnce(async () => refusal());
      const prompt = vi.spyOn(window, 'prompt');
      const api = await client();

      await api.get('/api/models');

      expect(invoke).toHaveBeenCalledTimes(2);
      expect(fetchMock).toHaveBeenCalledTimes(2);
      expect(authOf(fetchMock, 0)).toBe('Bearer old-token');
      expect(authOf(fetchMock, 1)).toBe('Bearer new-token');
      expect(String(fetchMock.mock.calls[1][0])).toBe('http://127.0.0.1:9887/api/models');
      await api.apiFetch('/api/events');
      expect(authOf(fetchMock, 2)).toBe('Bearer new-token');
      expect(prompt).not.toHaveBeenCalled();
    });

    it('retries only once, and never shows the link sentence', async () => {
      fetchMock.mockImplementation(async () => refusal());
      const api = await client();

      const error = await api.get('/api/models').catch((e: unknown) => e);

      expect(fetchMock).toHaveBeenCalledTimes(2);
      expect(String(error)).toContain('The gglib service restarted; reconnecting.');
      expect(String(error)).not.toContain('gglib web');
    });

    it('renews the event stream before it reconnects after a 401', async () => {
      const open = new Response(new ReadableStream({ start() {} }), { status: 200 });
      fetchMock.mockImplementationOnce(async () => refusal()).mockImplementation(async () => open);
      await client();
      const { SSEConnectionManager } = await import('../../../../src/services/transport/events/sse');

      const manager = new SSEConnectionManager('/api/events');
      const stop = manager.subscribe(() => {});
      await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2), { timeout: 3000 });
      stop();

      const auth = (n: number) => new Headers((fetchMock.mock.calls[n][1] as RequestInit).headers);
      expect(auth(0).get('Authorization')).toBe('Bearer old-token');
      expect(auth(1).get('Authorization')).toBe('Bearer new-token');
    });

    it('keeps trying to open the event stream when the desktop cannot yet say where the daemon is', async () => {
      invoke.mockReset().mockRejectedValueOnce(new Error('not ready')).mockResolvedValue({ port: 9887, token: 'new-token' });
      fetchMock.mockImplementation(async () => new Response(new ReadableStream({ start() {} }), { status: 200 }));
      await client();
      const { SSEConnectionManager } = await import('../../../../src/services/transport/events/sse');

      const manager = new SSEConnectionManager('/api/events');
      const opened = vi.fn();
      manager.opened.listen(opened);
      const stop = manager.subscribe(() => {});
      await vi.waitFor(() => expect(opened).toHaveBeenCalledTimes(1), { timeout: 3000 });
      stop();

      expect(invoke).toHaveBeenCalledTimes(2);
      expect(fetchMock).toHaveBeenCalledTimes(1);
      expect(String(fetchMock.mock.calls[0][0])).toBe('http://127.0.0.1:9887/api/events');
    });

    it('renews for a stream the daemon refused with a 401, and for nothing else', async () => {
      const api = await client();
      const { renewAfterRefusal } = await import('../../../../src/services/transport/api/renew');
      const { TransportError } = await import('../../../../src/services/transport/errors');
      const { SSEHttpError } = await import('../../../../src/utils/sse');
      await api.getClient();
      expect(invoke).toHaveBeenCalledTimes(1);

      await renewAfterRefusal(new TransportError('UNAUTHORIZED', 'forbidden', { status: 403 }));
      await renewAfterRefusal(new TransportError('INTERNAL', 'broken', { status: 500 }));
      await renewAfterRefusal(new TransportError('UNAUTHORIZED', 'no status'));
      await renewAfterRefusal(new TypeError('network error'));
      await renewAfterRefusal(new SSEHttpError(401, 'Unauthorized')); // a proxy's refusal, not the daemon's
      expect(invoke).toHaveBeenCalledTimes(1);

      await renewAfterRefusal(new TransportError('UNAUTHORIZED', SENTENCE, { status: 401, type: 'DAEMON_TOKEN_REQUIRED' }));
      expect(invoke).toHaveBeenCalledTimes(2);
      await api.apiFetch('/api/events');
      expect(authOf(fetchMock, fetchMock.mock.calls.length - 1)).toBe('Bearer new-token');
    });

    it('renews the server log stream before it reopens after a 401', async () => {
      const open = new Response(new ReadableStream({ start() {} }), { status: 200 });
      fetchMock.mockImplementationOnce(async () => refusal()).mockImplementation(async () => open);
      await client();
      const { listenToServerLogs } = await import('../../../../src/services/platform/serverLogs');

      const stop = await listenToServerLogs(9001, () => {});
      await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2), { timeout: 4000 });
      stop();

      const auth = (n: number) => new Headers((fetchMock.mock.calls[n][1] as RequestInit).headers);
      expect(auth(0).get('Authorization')).toBe('Bearer old-token');
      expect(auth(1).get('Authorization')).toBe('Bearer new-token');
      expect(String(fetchMock.mock.calls[1][0])).toBe('http://127.0.0.1:9887/api/servers/9001/logs/stream');
    });
  });

  it('is sent on the server log stream, which EventSource could not carry', async () => {
    const api = await client();
    api.setApiSession('', 'abc123');
    const { listenToServerLogs } = await import('../../../../src/services/platform/serverLogs');

    const stop = await listenToServerLogs(9001, () => {});
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
    stop();

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/servers/9001/logs/stream');
    expect(new Headers(init.headers).get('Authorization')).toBe('Bearer abc123');
  });

  it('still asks for the API key when a LAN-shared daemon wants one', async () => {
    fetchMock.mockImplementation(async () =>
      json({ error: 'Missing or invalid API key.', status: 401, type: 'INVALID_API_KEY' }, 401),
    );
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue(null);
    const api = await client();

    await expect(api.get('/api/models')).rejects.toThrow();

    expect(prompt).toHaveBeenCalledTimes(1);
  });
});
