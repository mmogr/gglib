/**
 * The window's gate over the real transport, a browser tab's: what the
 * daemon answers at `/health` and `/api` is a stubbed `fetch`, so what is
 * pinned is what each answer makes the window say. A daemon that refuses
 * the tab's token (it started again since the tab took its link) is there,
 * and the window says the daemon's own sentence, not that gglib was lost; a
 * 401 that is not the token refusal is a daemon there too, however many
 * polls; no answer at `/health` is gglib gone, and the
 * window comes back when it answers; one slow answer is asked again at
 * once, and only two in a row say gone; another program answering on the port
 * is no daemon, as Rust's probes say. The probe goes to `/health` with no
 * credential, and counts only an answer that says it is gglib's. In the
 * desktop app it asks the daemon's own address and never the tab's token.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

vi.mock('../../../src/sidebar/Sidebar', () => ({ Sidebar: () => <nav aria-label="gglib">sidebar</nav> }));

const SENTENCE =
  "this route needs the daemon's token: run the command from `gglib` on this machine, " +
  'or open the page from the link `gglib web` prints';
const LOST = 'Lost gglib. Reconnecting…';

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

/** What the daemon does: answer `/health` or not, and how it answers `/api`. */
const daemon = {
  up: true,
  /** What answers `/health`: gglib's daemon, or another program on its port. */
  health: (): Response => json({ service: 'gglib-daemon', status: 'ok' }),
  api: (): Response => json({ semver: '0.31.0', display: '0.31.0', sha: 'a', fingerprint: 'a' }),
};

let fetchMock: ReturnType<typeof vi.fn>;

const calls = (path: string) =>
  (fetchMock.mock.calls as [string, RequestInit | undefined][]).filter(([url]) => String(url).endsWith(path));

/** The gate around a stand-in for the window, from fresh modules: the client and its token are module state. */
async function renderGate() {
  const { ServiceGate } = await import('../../../src/app/ServiceDown');
  render(
    <ServiceGate>
      <p>the window</p>
    </ServiceGate>,
  );
}

const nextProbe = () => act(() => vi.advanceTimersByTimeAsync(2000));

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  localStorage.clear();
  window.history.replaceState(null, '', '/settings/about');
  daemon.up = true;
  daemon.health = () => json({ service: 'gglib-daemon', status: 'ok' });
  daemon.api = () => json({ semver: '0.31.0', display: '0.31.0', sha: 'a', fingerprint: 'a' });
  fetchMock = vi.fn(async (url: string) => {
    if (String(url).endsWith('/health')) {
      if (!daemon.up) throw new TypeError('Failed to fetch');
      return daemon.health();
    }
    if (!daemon.up) throw new TypeError('Failed to fetch');
    return daemon.api();
  });
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the window's gate in a browser", () => {
  it("says the daemon's sentence when it refuses the tab's token, not that gglib was lost", async () => {
    localStorage.setItem('gglib_daemon_token', 'from-before-the-restart');
    daemon.api = () => json({ error: SENTENCE, status: 401, type: 'DAEMON_TOKEN_REQUIRED' }, 401);
    await renderGate();

    expect(await screen.findByRole('alert')).toHaveTextContent(SENTENCE);
    expect(screen.queryByText('the window')).not.toBeInTheDocument();
    await nextProbe();
    await nextProbe();
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent(SENTENCE);
  });

  it('shows the window for a 401 that is not the token refusal', async () => {
    daemon.api = () => json({ error: 'Unauthorized.', status: 401, type: 'SOMETHING_ELSE' }, 401);
    await renderGate();

    expect(await screen.findByText('the window')).toBeInTheDocument();
    for (let i = 0; i < 5; i += 1) await nextProbe();

    expect(screen.getByText('the window')).toBeInTheDocument();
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
    expect(calls('/health').length).toBeGreaterThanOrEqual(6);
    // The token is asked about once, when the daemon is first heard, never per poll.
    expect(calls('/api/version')).toHaveLength(1);
  });

  it('says gglib was lost while /health has no answer, and gives the window back when it answers', async () => {
    daemon.up = false;
    await renderGate();

    expect(await screen.findByText(LOST)).toBeInTheDocument();
    expect(screen.queryByText('the window')).not.toBeInTheDocument();

    daemon.up = true;
    await nextProbe();

    expect(await screen.findByText('the window')).toBeInTheDocument();
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
  });

  it('asks again at once when /health is slow, and says lost only after two in a row', async () => {
    await renderGate();
    expect(await screen.findByText('the window')).toBeInTheDocument();

    // One probe gets no answer inside its limit; the one asked right after it does.
    const slow = (signal?: AbortSignal | null) =>
      new Promise<Response>((_, reject) => signal?.addEventListener('abort', () => reject(signal.reason)));
    let unanswered = 1;
    fetchMock.mockImplementation(async (url: string, init?: RequestInit) => {
      if (String(url).endsWith('/health') && unanswered > 0) {
        unanswered -= 1;
        return slow(init?.signal);
      }
      return String(url).endsWith('/health') ? daemon.health() : daemon.api();
    });
    await nextProbe();
    // Past the probe's 1.5 s limit: asked again, answered, and never called lost.
    await act(() => vi.advanceTimersByTimeAsync(1600));
    expect(screen.getByText('the window')).toBeInTheDocument();
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(400));

    // Two in a row: gglib is gone.
    unanswered = 2;
    await nextProbe();
    await act(() => vi.advanceTimersByTimeAsync(1600));
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(1600));
    expect(screen.getByText(LOST)).toBeInTheDocument();
  });

  it('asks /health with no credential, and not from the cache', async () => {
    localStorage.setItem('gglib_daemon_token', 'held');
    await renderGate();
    expect(await screen.findByText('the window')).toBeInTheDocument();

    const [url, init] = calls('/health')[0];
    expect(url).toBe('/health');
    expect(init).toEqual({ cache: 'no-store', signal: expect.any(AbortSignal) });
  });

  it('takes another program answering on the port for no daemon, as Rust does', async () => {
    daemon.health = () => new Response('<html>not gglib</html>', { status: 200, headers: { 'content-type': 'text/html' } });
    await renderGate();
    expect(await screen.findByText(LOST)).toBeInTheDocument();
    expect(screen.queryByText('the window')).not.toBeInTheDocument();

    daemon.health = () => json({ service: 'something-else', status: 'ok' });
    await nextProbe();
    expect(screen.getByText(LOST)).toBeInTheDocument();
  });
});

describe("the window while the desktop app's launch of gglib is in flight", () => {
  const STARTING = 'gglib is starting…';
  const DOWN = "gglib isn't running, so nothing can be served.";
  const WHY = 'the gglib daemon did not come up within 15s';

  /** The app's lookup of the daemon's address: out until `settle`, as the launch is. */
  function launchInFlight() {
    let outcome: { ok: true } | { ok: false } | null = null;
    const waiting: Array<() => void> = [];
    const answer = () =>
      outcome!.ok ? Promise.resolve({ port: 9887, token: 'minted' }) : Promise.reject(WHY);
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: vi.fn((cmd: string) => {
        if (cmd !== 'get_embedded_api_info') return Promise.resolve(null);
        if (outcome) return answer();
        return new Promise((resolve, reject) => waiting.push(() => void answer().then(resolve, reject)));
      }),
    };
    return async (ok: boolean) => {
      outcome = ok ? { ok: true } : { ok: false };
      await act(async () => {
        for (const settle of waiting.splice(0)) settle();
      });
    };
  }

  it('says gglib is starting while the answer is out, never that it is not running', async () => {
    launchInFlight();
    daemon.up = false;
    await renderGate();
    expect(await screen.findByText(STARTING)).toBeInTheDocument();
    await nextProbe();
    await nextProbe();
    expect(screen.getByText(STARTING)).toBeInTheDocument();
    expect(screen.queryByText(DOWN)).not.toBeInTheDocument();
    expect(screen.queryByText('the window')).not.toBeInTheDocument();
  });

  it('draws the window once the launch answers', async () => {
    const settle = launchInFlight();
    await renderGate();
    expect(await screen.findByText(STARTING)).toBeInTheDocument();

    await settle(true);
    expect(await screen.findByText('the window')).toBeInTheDocument();
    expect(screen.queryByText(STARTING)).not.toBeInTheDocument();
  });

  it("says gglib isn't running, in the launch's own words, when the launch failed", async () => {
    const settle = launchInFlight();
    daemon.up = false;
    await renderGate();
    expect(await screen.findByText(STARTING)).toBeInTheDocument();

    await settle(false);
    expect(await screen.findByText(DOWN)).toBeInTheDocument();
    expect(screen.getByText(WHY)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Start gglib' })).toBeInTheDocument();
    expect(screen.queryByText(STARTING)).not.toBeInTheDocument();
  });

  it('keeps the failed launch\'s sentence up, the same node, while every probe asks again', async () => {
    const settle = launchInFlight();
    daemon.up = false;
    await renderGate();
    // The same module instance the gate reads (both imported after the reset).
    const lookup = await import('../../../src/services/transport/api/addressLookup');
    const seen: string[] = [];
    lookup.onAddressLookup(() => seen.push(lookup.addressLookup().state));

    await settle(false);
    const sentence = await screen.findByRole('alert');
    expect(sentence).toHaveTextContent(WHY);

    for (let tick = 0; tick < 4; tick += 1) {
      await nextProbe();
      expect(screen.getByRole('alert')).toBe(sentence);
    }
    expect(seen).not.toContain('pending');
    expect(seen.filter((state) => state === 'failed')).toHaveLength(1);

    // A later answer (a daemon heard since) replaces it.
    daemon.up = true;
    await settle(true);
    await nextProbe();
    expect(await screen.findByText('the window')).toBeInTheDocument();
    expect(lookup.addressLookup().state).toBe('known');
    expect(screen.queryByText(WHY)).not.toBeInTheDocument();
  });

  it('draws the window once the address is known, before the probe has answered', async () => {
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: vi.fn(async (cmd: string) => (cmd === 'get_embedded_api_info' ? { port: 9887, token: 'desk' } : null)),
    };
    daemon.health = () => new Promise<Response>(() => {}) as unknown as Response;
    await renderGate();
    expect(await screen.findByText('the window')).toBeInTheDocument();
    expect(screen.queryByText(STARTING)).not.toBeInTheDocument();
  });

  it('leaves a browser tab as it was before its first answer: the window, never starting', async () => {
    daemon.health = () => new Promise<Response>(() => {}) as unknown as Response;
    await renderGate();
    expect(await screen.findByText('the window')).toBeInTheDocument();
    expect(screen.queryByText(STARTING)).not.toBeInTheDocument();
  });
});

describe("the window's gate in the desktop app", () => {
  it("asks the daemon's own address, and never asks about a tab's token", async () => {
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      invoke: vi.fn(async (cmd: string) => (cmd === 'get_embedded_api_info' ? { port: 9887, token: 'desk' } : null)),
    };
    await renderGate();
    expect(await screen.findByText('the window')).toBeInTheDocument();
    await nextProbe();

    expect(calls('/health')[0][0]).toBe('http://127.0.0.1:9887/health');
    expect(calls('/api/version')).toHaveLength(0);
  });
});
