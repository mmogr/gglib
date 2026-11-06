/**
 * The proxy panel, opened from the sidebar's endpoint line: Turn on names
 * nothing, so the daemon starts the endpoint as the settings say; View
 * Dashboard closes the popover it opens over; Clear Cache presents the
 * revealed key.
 *
 * The per-run connection options this file once pinned are gone: where the
 * endpoint binds, its hosts and its prompt cache are settings now.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ReactNode } from 'react';

import { EndpointLine } from '../../../src/sidebar/EndpointLine';
import { applyProxyStatus, resetProxyState } from '../../../src/services/proxyRegistry';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { appSettings } from '../fixtures/settings';
import type { AppSettings } from '../../../src/types';

const settingsRef: { current: AppSettings | null } = { current: null };

/** The registry as the daemon's status leaves it: up on `port`, or down. */
function proxyIs(running: boolean, port: number | null) {
  resetProxyState();
  applyProxyStatus({
    running,
    state: running ? 'on' : 'off',
    port,
    base_url: running && port !== null ? `http://127.0.0.1:${port}/v1` : null,
    key: running ? 'none_needed' : null,
    pinned_model: null,
  });
}
const { revealSecret, clearProxyCache, startProxy } = vi.hoisted(() => ({
  revealSecret: vi.fn(),
  clearProxyCache: vi.fn(),
  startProxy: vi.fn(),
}));

vi.mock('../../../src/hooks/useSettings', () => ({
  useSettings: () => ({ settings: settingsRef.current }),
}));
vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({
    revealSecret,
    startProxy,
  }),
}));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  clearProxyCache,
}));
vi.mock('../../../src/components/ProxyDashboardModal', () => ({
  ProxyDashboardModal: ({ isOpen }: { isOpen: boolean }) =>
    isOpen ? <div role="dialog" aria-label="Live numbers" /> : null,
}));

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>{children}</ToastProvider>
);

describe('The proxy panel — Turn on', () => {
  beforeEach(() => {
    proxyIs(false, null);
    revealSecret.mockResolvedValue(null);
    startProxy.mockReset();
    startProxy.mockResolvedValue({ running: true, port: 8080 });
  });

  // Nothing is named, not even a port: the daemon resolves every field
  // from the settings, the same start the tray and `gglib proxy` get.
  it('names nothing, so the daemon starts the endpoint as the settings say', async () => {
    settingsRef.current = appSettings({ proxyHost: '0.0.0.0', proxyCacheOnDisk: true });
    render(<EndpointLine />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^endpoint,/i }));
    await user.click(screen.getByRole('button', { name: /start proxy/i }));
    await waitFor(() => expect(startProxy).toHaveBeenCalledTimes(1));
    expect(startProxy).toHaveBeenCalledWith({});
  });
});

describe('The proxy panel — View Dashboard', () => {
  beforeEach(() => {
    proxyIs(false, null);
    revealSecret.mockResolvedValue(null);
  });

  it('closes the popover it opens over', async () => {
    // A click in a dialog no longer counts as outside the popover, so the
    // dashboard opening over it no longer closes it by accident. The button
    // closes it on purpose now, and this is what holds that.
    proxyIs(true, 8080);
    settingsRef.current = appSettings();
    render(<EndpointLine />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^endpoint,/i }));
    expect(screen.getByRole('heading', { name: 'OpenAI Proxy' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /view dashboard/i }));
    expect(screen.queryByRole('heading', { name: 'OpenAI Proxy' })).not.toBeInTheDocument();
    // The dashboard it opened outlives the panel: the line owns the panel's
    // open state now, and the dashboard must not go with it.
    expect(screen.getByRole('dialog', { name: 'Live numbers' })).toBeInTheDocument();
  });
});

/** Settings carry no key, so Clear Cache presents the one revealed for it. */
describe('The proxy panel — Clear Cache', () => {
  it('sends the revealed proxy key', async () => {
    revealSecret.mockResolvedValue('k3y-5f3a');
    clearProxyCache.mockResolvedValue({ message: 'cleared' });
    proxyIs(true, 8080);
    settingsRef.current = appSettings({ hasProxyApiKey: true });
    render(<EndpointLine />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^endpoint,/i }));
    expect(revealSecret).toHaveBeenCalledWith({ kind: 'setting', secret: 'proxy_api_key' });
    const clear = screen.getByRole('button', { name: /clear cache/i });
    await waitFor(() => expect(clear).toBeEnabled());

    await user.click(clear);

    expect(clearProxyCache).toHaveBeenCalledWith('127.0.0.1', 8080, 'k3y-5f3a');
  });

  /** Bound to one interface's address, the endpoint listens nowhere else. */
  it('reaches an endpoint bound to one address on that address, and a wildcard on loopback', async () => {
    revealSecret.mockResolvedValue(null);
    clearProxyCache.mockResolvedValue({ message: 'cleared' });
    proxyIs(true, 8080);
    settingsRef.current = appSettings({ proxyHost: '192.168.1.20' });
    const { unmount } = render(<EndpointLine />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^endpoint,/i }));
    expect(screen.getByText('http://192.168.1.20:8080/v1')).toBeInTheDocument();
    const clear = screen.getByRole('button', { name: /clear cache/i });
    await waitFor(() => expect(clear).toBeEnabled());
    await user.click(clear);
    expect(clearProxyCache).toHaveBeenCalledWith('192.168.1.20', 8080, null);
    unmount();

    settingsRef.current = appSettings({ proxyHost: '0.0.0.0' });
    render(<EndpointLine />, { wrapper });
    await user.click(screen.getByRole('button', { name: /^endpoint,/i }));
    expect(screen.getByText('http://127.0.0.1:8080/v1')).toBeInTheDocument();
  });

  /** Sent before the reveal answers, it would carry no key and be refused. */
  it('waits for the reveal before it can be pressed', async () => {
    revealSecret.mockReturnValue(new Promise(() => {}));
    proxyIs(true, 8080);
    settingsRef.current = appSettings({ hasProxyApiKey: true });
    render(<EndpointLine />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^endpoint,/i }));

    expect(screen.getByRole('button', { name: /clear cache/i })).toBeDisabled();
  });

  /** A proxy with no key, or a reveal that failed, still clears: with no key. */
  it.each([
    ['no key is stored', () => revealSecret.mockResolvedValue(null)],
    ['the reveal fails', () => revealSecret.mockRejectedValue(new Error('daemon gone'))],
  ])('can be pressed when %s, and sends no key', async (_, answer) => {
    answer();
    clearProxyCache.mockResolvedValue({ message: 'cleared' });
    proxyIs(true, 8080);
    settingsRef.current = appSettings();
    render(<EndpointLine />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /^endpoint,/i }));
    const clear = screen.getByRole('button', { name: /clear cache/i });
    await waitFor(() => expect(clear).toBeEnabled());

    await user.click(clear);

    expect(clearProxyCache).toHaveBeenCalledWith('127.0.0.1', 8080, null);
  });
});
