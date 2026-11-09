/**
 * The endpoint line says only what the daemon said: its state word, the
 * address a client reaches it at, whether a request needs the key, and how
 * many models are loaded. Between an event and the read that follows it,
 * the address is withheld rather than guessed. ⧉ copies the whole address
 * and goes nowhere; a click on the line opens the endpoint place. The dot
 * blinks once when the proxy's request count grows, at most once a second,
 * and never under reduced motion; the dashboard stream is played by the test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { ReactNode } from 'react';
import { MemoryRouter } from 'react-router';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ revealSecret: async () => null, getSettings: async () => ({}) }),
}));
const dashboard = vi.hoisted(() => ({ send: null as ((s: unknown) => void) | null }));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (_h: string, _p: number, _k: unknown, onSnapshot: (s: unknown) => void) => {
    dashboard.send = onSnapshot;
    return () => {
      dashboard.send = null;
    };
  },
}));

import { EndpointLine, secondLine, shownAddress } from '../../../src/sidebar/EndpointLine';
import { dashboardSnapshot } from '../fixtures/dashboard';
import { applyProxyStatus, ingestProxyEvent, resetProxyState } from '../../../src/services/proxyRegistry';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';
import { PlaceProviders, renderShell } from '../places/harness';

/** The line under the providers, in a router at the library. */
const Wrapper = ({ children }: { children: ReactNode }) => (
  <MemoryRouter initialEntries={['/library']}>
    <PlaceProviders>{children}</PlaceProviders>
  </MemoryRouter>
);

const UP: ProxyStatus = {
  running: true,
  state: 'on',
  port: 8080,
  base_url: 'http://192.168.1.20:8080/v1',
  key: 'none_needed',
  pinned_model: null,
  pinned_to: null,
};

function serve(modelId: number, type: 'running' | 'stopped' = 'running') {
  act(() =>
    ingestServerEvent({ type, modelId: String(modelId), port: 9000 + modelId, updatedAt: Date.now(), modelName: `m${modelId}` }),
  );
}

const line = () => screen.getByRole('button', { name: /^endpoint,/i });

describe('the endpoint line', () => {
  beforeEach(() => resetProxyState());
  afterEach(() => {
    for (const id of [1, 2]) serve(id, 'stopped');
  });

  it('off: says Off, shows no address, and says what a click does', () => {
    applyProxyStatus({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null, pinned_to: null });
    render(<EndpointLine />, { wrapper: Wrapper });
    expect(line()).toHaveAccessibleName('Endpoint, Off');
    expect(screen.getByText('Off · click to turn on')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /copy the endpoint address/i })).not.toBeInTheDocument();
  });

  it('on: says On, the address without its scheme, and how many models are loaded', () => {
    applyProxyStatus(UP);
    serve(1);
    serve(2);
    render(<EndpointLine />, { wrapper: Wrapper });
    expect(line()).toHaveAccessibleName('Endpoint, On');
    expect(screen.getByText('192.168.1.20:8080/v1')).toBeInTheDocument();
    expect(screen.getByText('2 loaded')).toBeInTheDocument();

    serve(2, 'stopped');
    expect(screen.getByText('1 loaded')).toBeInTheDocument();
    serve(1, 'stopped');
    expect(screen.getByText('nothing loaded yet')).toBeInTheDocument();
  });

  it('pinned: says Pinned and, under it, the daemon\'s line naming the one model it serves', () => {
    applyProxyStatus({ ...UP, state: 'pinned', pinned_model: 'qwen3-8b', pinned_to: 'Pinned to qwen3-8b' });
    render(<EndpointLine />, { wrapper: Wrapper });
    expect(line()).toHaveAccessibleName('Endpoint, Pinned');
    expect(screen.getByText('Pinned to qwen3-8b')).toBeInTheDocument();
    expect(screen.queryByText(/only/)).not.toBeInTheDocument();
  });

  it('shows the key glyph only when a request must carry the key', () => {
    applyProxyStatus({ ...UP, key: 'required' });
    const { rerender } = render(<EndpointLine />, { wrapper: Wrapper });
    expect(screen.getByRole('img', { name: 'Key required' })).toBeInTheDocument();

    act(() => applyProxyStatus(UP));
    rerender(<EndpointLine />);
    expect(screen.queryByRole('img', { name: 'Key required' })).not.toBeInTheDocument();
  });

  it('between a start event and the daemon\'s answer, says On and withholds the old address', () => {
    applyProxyStatus({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null, pinned_to: null });
    render(<EndpointLine />, { wrapper: Wrapper });
    act(() => ingestProxyEvent({ type: 'proxy_started', port: 8080 }));
    expect(line()).toHaveAccessibleName('Endpoint, On');
    expect(screen.queryByRole('button', { name: /copy the endpoint address/i })).not.toBeInTheDocument();

    act(() => applyProxyStatus(UP));
    expect(screen.getByText('192.168.1.20:8080/v1')).toBeInTheDocument();
  });

  it('⧉ copies the whole address, scheme included, and says so', async () => {
    const user = userEvent.setup();
    applyProxyStatus(UP);
    const app = renderShell('/library');

    await user.click(screen.getByRole('button', { name: /copy the endpoint address/i }));

    expect(await navigator.clipboard.readText()).toBe('http://192.168.1.20:8080/v1');
    // Said beside the control, never as a toast: a copy is not a background outcome.
    expect(await screen.findByText('Copied')).toBeInTheDocument();
    expect(screen.getByTestId('toasts')).not.toHaveTextContent('copied');
    // The copy went nowhere.
    expect(app.at()).toBe('/library');
  });

  it('a click on the line opens the endpoint place, and marks the line as the place open', async () => {
    applyProxyStatus(UP);
    const app = renderShell('/library');
    const user = userEvent.setup();
    expect(line()).not.toHaveAttribute('aria-current');

    await user.click(line());

    expect(app.at()).toBe('/endpoint');
    expect(line()).toHaveAttribute('aria-current', 'page');
  });
});

describe("the endpoint dot's request blink", () => {
  // The filled dot inside the mark: only it blinks.
  const dot = () => within(line()).getByRole('img', { name: 'On' }).firstElementChild;

  /** A frame of the dashboard stream saying `total` requests served, at `at` ms. */
  async function served(total: number, at: number) {
    vi.setSystemTime(at);
    for (let i = 0; i < 5 && dashboard.send === null; i += 1) await act(async () => {});
    expect(dashboard.send).not.toBeNull();
    act(() => dashboard.send?.(dashboardSnapshot({ total_requests: total })));
  }

  beforeEach(() => {
    resetProxyState();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
    applyProxyStatus(UP);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('blinks once, for 150 ms, when the count grows; not for the first frame', async () => {
    render(<EndpointLine />, { wrapper: Wrapper });
    await served(7, 10_000);
    expect(dot()).not.toHaveClass('animate-request-blink');

    await served(8, 20_000);
    expect(dot()).toHaveClass('animate-request-blink');
    act(() => vi.advanceTimersByTime(149));
    expect(dot()).toHaveClass('animate-request-blink');
    act(() => vi.advanceTimersByTime(1));
    expect(dot()).not.toHaveClass('animate-request-blink');

    // A frame with no new request does not blink.
    await served(8, 30_000);
    expect(dot()).not.toHaveClass('animate-request-blink');
  });

  it('two requests within 200 ms blink once; the next a second later blinks again', async () => {
    render(<EndpointLine />, { wrapper: Wrapper });
    await served(0, 10_000);
    await served(1, 11_000);
    expect(dot()).toHaveClass('animate-request-blink');
    act(() => vi.advanceTimersByTime(150));

    await served(2, 11_200);
    expect(dot()).not.toHaveClass('animate-request-blink');

    await served(3, 12_000);
    expect(dot()).toHaveClass('animate-request-blink');
  });

  it('never blinks under reduced motion', async () => {
    vi.stubGlobal('matchMedia', (query: string) => ({ matches: query === '(prefers-reduced-motion: reduce)' }));
    render(<EndpointLine />, { wrapper: Wrapper });
    await served(0, 10_000);
    await served(1, 20_000);
    expect(dot()).not.toHaveClass('animate-request-blink');
  });
});

describe('the words of the endpoint line', () => {
  it('strips only the scheme from the address', () => {
    expect(shownAddress('http://127.0.0.1:8080/v1')).toBe('127.0.0.1:8080/v1');
    expect(shownAddress('http://[fe80::1]:8080/v1')).toBe('[fe80::1]:8080/v1');
  });

  it('second line: off, starting, the pin, none, a count', () => {
    const facts = { state: 'on' as const, baseUrl: 'http://127.0.0.1:8080/v1', key: null, pinnedModel: null, pinnedTo: null };
    expect(secondLine({ ...facts, state: 'off' }, 3)).toBe('Off · click to turn on');
    expect(secondLine({ ...facts, state: 'starting' }, 3)).toBe('Starting');
    expect(secondLine({ ...facts, state: 'pinned', pinnedModel: 'qwen3-8b', pinnedTo: 'Pinned to qwen3-8b' }, 1)).toBe('Pinned to qwen3-8b');
    expect(secondLine(facts, 0)).toBe('nothing loaded yet');
    expect(secondLine(facts, 1)).toBe('1 loaded');
    expect(secondLine(facts, 2)).toBe('2 loaded');
  });
});
