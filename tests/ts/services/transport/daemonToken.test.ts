import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

/**
 * The page keeps the token the link `gglib web` prints carries, and sends it.
 *
 * The routes that change who is trusted answer only the daemon's token, so a
 * page opened from that link must hold it, must not leave it in the address
 * bar, and must not answer a refusal on one of those routes by asking for an
 * API key no route of that kind takes.
 */

const SENTENCE =
  "this route needs the daemon's token: run the command from `gglib` on this machine, " +
  'or open the page from the link `gglib web` prints';

/** A fresh client module each time: it caches its client at module level. */
async function client() {
  vi.resetModules();
  return import('../../../../src/services/transport/api/client');
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/** The `Authorization` header of the `n`th request sent. */
function authOf(fetchMock: ReturnType<typeof vi.fn>, n = 0): string | undefined {
  const init = fetchMock.mock.calls[n][1] as RequestInit;
  return (init.headers as Record<string, string>)['Authorization'];
}

describe('the daemon token from the link', () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    sessionStorage.clear();
    localStorage.clear();
    window.history.replaceState(null, '', '/');
    fetchMock = vi.fn(async () => json({ success: true, data: [] }));
    vi.stubGlobal('fetch', fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('is read from the fragment, kept for the tab, and stripped from the address', async () => {
    window.history.replaceState(null, '', '/models?view=grid#token=abc123');
    const { takeDaemonToken } = await import('../../../../src/services/transport/api/daemonToken');

    expect(takeDaemonToken()).toBe('abc123');
    expect(sessionStorage.getItem('gglib_daemon_token')).toBe('abc123');
    expect(window.location.hash).toBe('');
    expect(window.location.pathname + window.location.search).toBe('/models?view=grid');
    expect(window.location.href).not.toContain('abc123');
    expect(localStorage.length).toBe(0);
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

  it('outlives a reload of the tab, from sessionStorage', async () => {
    sessionStorage.setItem('gglib_daemon_token', 'abc123');
    const api = await client();

    await api.get('/api/models');

    expect(authOf(fetchMock)).toBe('Bearer abc123');
  });

  it('is not invented: a page opened without one sends no credential', async () => {
    const api = await client();

    await api.get('/api/models');

    expect(authOf(fetchMock)).toBeUndefined();
  });

  it("shows a trust route's refusal in the daemon's words, and asks for no API key", async () => {
    fetchMock.mockImplementation(async () =>
      json({ error: SENTENCE, status: 401, type: 'DAEMON_TOKEN_REQUIRED' }, 401),
    );
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue('some-key');
    const api = await client();

    await expect(api.post('/api/remote/invite')).rejects.toThrow(SENTENCE);

    expect(prompt).not.toHaveBeenCalled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('comes from the desktop app, and is asked for again after a trust route refused it', async () => {
    const invoke = vi
      .fn()
      .mockResolvedValueOnce({ port: 9887, token: null })
      .mockResolvedValue({ port: 9887, token: 'desk-token' });
    Object.assign(window, { __TAURI_INTERNALS__: { invoke } });
    fetchMock.mockImplementationOnce(async () =>
      json({ error: SENTENCE, status: 401, type: 'DAEMON_TOKEN_REQUIRED' }, 401),
    );
    const api = await client();

    try {
      await expect(api.post('/api/remote/invite')).rejects.toThrow(SENTENCE);
      await api.post('/api/remote/invite');
    } finally {
      delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    }

    expect(authOf(fetchMock, 0)).toBeUndefined();
    expect(authOf(fetchMock, 1)).toBe('Bearer desk-token');
    expect(String(fetchMock.mock.calls[1][0])).toBe('http://127.0.0.1:9887/api/remote/invite');
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
