/**
 * The window's gate over the real transport, a browser tab's: what the
 * daemon answers at `/health` and `/api` is a stubbed `fetch`, so what is
 * pinned is what each answer makes the window say. A daemon that refuses
 * the tab's token (it started again since the tab took its link) is there,
 * and the window says the daemon's own sentence, not that gglib was lost; a
 * daemon that wants an API key is there too, and the probe never asks for
 * the key, however many polls; no answer at `/health` is gglib gone, and the
 * window comes back when it answers. The probe goes to `/health` with no
 * credential, and only no answer is gone.
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
  api: (): Response => json({ semver: '0.31.0', display: '0.31.0', sha: 'a', fingerprint: 'a' }),
};

let fetchMock: ReturnType<typeof vi.fn>;
let prompt: ReturnType<typeof vi.spyOn>;

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
  daemon.api = () => json({ semver: '0.31.0', display: '0.31.0', sha: 'a', fingerprint: 'a' });
  fetchMock = vi.fn(async (url: string) => {
    if (String(url).endsWith('/health')) {
      if (!daemon.up) throw new TypeError('Failed to fetch');
      return json({ service: 'gglib-daemon', status: 'ok' });
    }
    if (!daemon.up) throw new TypeError('Failed to fetch');
    return daemon.api();
  });
  vi.stubGlobal('fetch', fetchMock);
  prompt = vi.spyOn(window, 'prompt').mockReturnValue(null);
});

afterEach(() => {
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
    expect(prompt).not.toHaveBeenCalled();
  });

  it('shows the window for a daemon that wants an API key, and never asks for the key itself', async () => {
    daemon.api = () =>
      json({ error: "Missing or invalid API key. Send it as 'Authorization: Bearer <key>'.", status: 401, type: 'INVALID_API_KEY' }, 401);
    await renderGate();

    expect(await screen.findByText('the window')).toBeInTheDocument();
    for (let i = 0; i < 5; i += 1) await nextProbe();

    expect(screen.getByText('the window')).toBeInTheDocument();
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
    expect(prompt).not.toHaveBeenCalled();
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

  it('asks /health with no credential, and outside CORS', async () => {
    localStorage.setItem('gglib_daemon_token', 'held');
    await renderGate();
    expect(await screen.findByText('the window')).toBeInTheDocument();

    const [url, init] = calls('/health')[0];
    expect(url).toBe('/health');
    expect(init).toEqual({ mode: 'no-cors', cache: 'no-store' });
  });
});
