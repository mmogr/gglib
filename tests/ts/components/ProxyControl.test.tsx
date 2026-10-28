/**
 * The proxy panel's Default context box, and what it says when nothing is set.
 *
 * It read `server default (from settings)`, which is wrong twice over: there
 * is no server default — `to_runtime_config` collapses the `BuiltInDefault`
 * rung to `None` precisely so the daemon sizes the launch — and it is not
 * from settings, because settings hold nothing. See ADR 0009.
 *
 * This surface is the reason the file exists. The claim shipped in #578 on
 * 2026-07-04 and survived until a phrase search found it seven weeks later;
 * a sweep scoped to the arc's own diff would have missed it. Correcting it
 * and leaving nothing to hold the correction would repeat exactly that.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ReactNode } from 'react';

import ProxyControl from '../../../src/components/ProxyControl';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { appSettings } from '../fixtures/settings';
import type { AppSettings } from '../../../src/types';

const settingsRef: { current: AppSettings | null } = { current: null };
const proxyStateRef = { current: { running: false, port: null as number | null } };
const { revealSecret, clearProxyCache, startProxy } = vi.hoisted(() => ({
  revealSecret: vi.fn(),
  clearProxyCache: vi.fn(),
  startProxy: vi.fn(),
}));

vi.mock('../../../src/hooks/useSettings', () => ({
  useSettings: () => ({ settings: settingsRef.current }),
}));
vi.mock('../../../src/services/proxyRegistry', () => ({
  useProxyState: () => proxyStateRef.current,
}));
vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({
    getProxyStatus: () => Promise.resolve({ pinned_model: null }),
    revealSecret,
    startProxy,
  }),
}));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  clearProxyCache,
}));
vi.mock('../../../src/components/ProxyDashboardModal', () => ({
  ProxyDashboardModal: () => null,
}));

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>{children}</ToastProvider>
);

/** Open the dropdown, then the connection options, and hand back the box. */
async function openContextBox(settings: AppSettings | null) {
  settingsRef.current = settings;
  render(<ProxyControl />, { wrapper });
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: /proxy/i }));
  await user.click(screen.getByRole('button', { name: /connection options/i }));
  return screen.getByLabelText(/Default context/i);
}

describe('ProxyControl — the Default context placeholder', () => {
  beforeEach(() => {
    settingsRef.current = null;
    proxyStateRef.current = { running: false, port: null };
  });

  it('does not claim a server default when nothing is stored', async () => {
    const box = await openContextBox(appSettings());
    expect(box).toHaveAttribute('placeholder', 'Sized per launch');
    // The exact string this replaced, named so a silent revert fails here.
    expect(box.getAttribute('placeholder')).not.toContain('from settings');
    expect(box.getAttribute('placeholder')).not.toContain('server default');
  });

  it('names the stored default, separated, when there is one', async () => {
    const box = await openContextBox(appSettings({ defaultContextSize: 32768 }));
    expect(box).toHaveAttribute('placeholder', '32,768 (from settings)');
  });
});

describe('ProxyControl — the port', () => {
  beforeEach(() => {
    settingsRef.current = null;
    proxyStateRef.current = { running: false, port: null };
    startProxy.mockReset();
    startProxy.mockResolvedValue({});
  });

  /**
   * Settings arrive after the first render, as they do in the app. The port
   * used to be read then, and showed and sent 8080 whatever was stored.
   */
  async function openWithLateSettings(settings: AppSettings) {
    const view = render(<ProxyControl />, { wrapper });
    settingsRef.current = settings;
    view.rerender(<ProxyControl />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /proxy/i }));
    await user.click(screen.getByRole('button', { name: /connection options/i }));
    return { user, box: screen.getByLabelText(/Proxy port/i) as HTMLInputElement };
  }

  it('shows the stored port and leaves it to the daemon', async () => {
    const { user, box } = await openWithLateSettings(appSettings({ proxyPort: 18080 }));

    expect(box).toHaveValue(null);
    expect(box).toHaveAttribute('placeholder', '18080 (from settings)');
    await user.click(screen.getByRole('button', { name: /start proxy/i }));

    await waitFor(() => expect(startProxy).toHaveBeenCalledTimes(1));
    expect(startProxy.mock.calls[0][0]).not.toHaveProperty('port');
  });

  /** Nothing stored: 8080 is the built-in default, not something settings say. */
  it('calls the port the default when none is stored', async () => {
    const { box } = await openWithLateSettings(appSettings({ proxyPort: null }));

    expect(box).toHaveAttribute('placeholder', '8080 (default)');
  });

  it('leaves the port to the daemon again once a typed one is cleared', async () => {
    const { user, box } = await openWithLateSettings(appSettings({ proxyPort: 18080 }));

    await user.type(box, '18181');
    await user.clear(box);
    await user.click(screen.getByRole('button', { name: /start proxy/i }));

    await waitFor(() => expect(startProxy).toHaveBeenCalledTimes(1));
    expect(startProxy.mock.calls[0][0]).not.toHaveProperty('port');
  });

  it('sends a port typed for this run', async () => {
    const { user, box } = await openWithLateSettings(appSettings({ proxyPort: 18080 }));

    await user.type(box, '18181');
    await user.click(screen.getByRole('button', { name: /start proxy/i }));

    await waitFor(() => expect(startProxy).toHaveBeenCalledTimes(1));
    expect(startProxy.mock.calls[0][0]).toMatchObject({ port: 18181 });
  });
});

describe('ProxyControl — View Dashboard', () => {
  beforeEach(() => {
    proxyStateRef.current = { running: false, port: null };
    revealSecret.mockResolvedValue(null);
  });

  it('closes the popover it opens over', async () => {
    // A click in a dialog no longer counts as outside the popover, so the
    // dashboard opening over it no longer closes it by accident. The button
    // closes it on purpose now, and this is what holds that.
    proxyStateRef.current = { running: true, port: 8080 };
    settingsRef.current = appSettings();
    render(<ProxyControl />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /proxy/i }));
    expect(screen.getByRole('heading', { name: 'OpenAI Proxy' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /view dashboard/i }));
    expect(screen.queryByRole('heading', { name: 'OpenAI Proxy' })).not.toBeInTheDocument();
  });
});

/** Settings carry no key, so Clear Cache presents the one revealed for it. */
describe('ProxyControl — Clear Cache', () => {
  it('sends the revealed proxy key', async () => {
    revealSecret.mockResolvedValue('k3y-5f3a');
    clearProxyCache.mockResolvedValue({ message: 'cleared' });
    proxyStateRef.current = { running: true, port: 8080 };
    settingsRef.current = appSettings({ hasProxyApiKey: true });
    render(<ProxyControl />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /proxy/i }));
    expect(revealSecret).toHaveBeenCalledWith({ kind: 'setting', secret: 'proxy_api_key' });
    const clear = screen.getByRole('button', { name: /clear cache/i });
    await waitFor(() => expect(clear).toBeEnabled());

    await user.click(clear);

    expect(clearProxyCache).toHaveBeenCalledWith('127.0.0.1', 8080, 'k3y-5f3a');
  });

  /** Sent before the reveal answers, it would carry no key and be refused. */
  it('waits for the reveal before it can be pressed', async () => {
    revealSecret.mockReturnValue(new Promise(() => {}));
    proxyStateRef.current = { running: true, port: 8080 };
    settingsRef.current = appSettings({ hasProxyApiKey: true });
    render(<ProxyControl />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /proxy/i }));

    expect(screen.getByRole('button', { name: /clear cache/i })).toBeDisabled();
  });

  /** A proxy with no key, or a reveal that failed, still clears: with no key. */
  it.each([
    ['no key is stored', () => revealSecret.mockResolvedValue(null)],
    ['the reveal fails', () => revealSecret.mockRejectedValue(new Error('daemon gone'))],
  ])('can be pressed when %s, and sends no key', async (_, answer) => {
    answer();
    clearProxyCache.mockResolvedValue({ message: 'cleared' });
    proxyStateRef.current = { running: true, port: 8080 };
    settingsRef.current = appSettings();
    render(<ProxyControl />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /proxy/i }));
    const clear = screen.getByRole('button', { name: /clear cache/i });
    await waitFor(() => expect(clear).toBeEnabled());

    await user.click(clear);

    expect(clearProxyCache).toHaveBeenCalledWith('127.0.0.1', 8080, null);
  });

  it('types the run’s own key into a masked input', async () => {
    revealSecret.mockResolvedValue(null);
    proxyStateRef.current = { running: false, port: null };
    settingsRef.current = appSettings();
    render(<ProxyControl />, { wrapper });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: /proxy/i }));
    await user.click(screen.getByRole('button', { name: /connection options/i }));

    expect(screen.getByLabelText('API key for this run')).toHaveAttribute('type', 'password');
  });
});
