/**
 * Tests for the tray popover panel.
 *
 * No Tauri mocking anywhere below: the panel deliberately uses no IPC, so it
 * renders under jsdom exactly as it does in the popover window.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { TrayPanel } from '../../../src/pages/TrayPanel';
import type { DashboardSnapshot } from '../../../src/services/transport/types/dashboard';

const transport = vi.hoisted(() => ({
  startProxy: vi.fn().mockResolvedValue(undefined),
  stopProxy: vi.fn().mockResolvedValue(undefined),
  getProxyStatus: vi.fn().mockResolvedValue({ running: false, port: null }),
  // The panel reads the proxy's API key on mount so its dashboard stream can
  // authenticate. Unset is the loopback default.
  getSettings: vi.fn().mockResolvedValue({ proxyApiKey: null }),
}));

const proxyState = vi.hoisted(() => ({
  current: { running: false, port: null as number | null },
}));

const dashboard = vi.hoisted(() => ({
  current: { snapshot: null as DashboardSnapshot | null, connected: false },
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

vi.mock('../../../src/services/proxyRegistry', () => ({
  useProxyState: () => proxyState.current,
}));

vi.mock('../../../src/services/proxyEvents', () => ({
  initProxyEvents: vi.fn(),
  cleanupProxyEvents: vi.fn(),
}));

vi.mock('../../../src/hooks/useProxyDashboard', () => ({
  useProxyDashboard: () => dashboard.current,
}));

function snapshot(overrides: Partial<DashboardSnapshot> = {}): DashboardSnapshot {
  return {
    active_connections: [],
    slots: [],
    slots_available: false,
    slots_status: 'Slot metrics unavailable.',
    total_requests: 0,
    ...overrides,
  } as DashboardSnapshot;
}

describe('TrayPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    proxyState.current = { running: false, port: null };
    dashboard.current = { snapshot: null, connected: false };
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it('explains what the proxy is for while it is stopped', () => {
    render(<TrayPanel />);

    expect(screen.getByText('Stopped')).toBeInTheDocument();
    expect(screen.getByText(/openai-compatible endpoint/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /start proxy/i })).toBeInTheDocument();
  });

  it('starts the proxy through the shared transport', async () => {
    render(<TrayPanel />);

    await userEvent.click(screen.getByRole('button', { name: /start proxy/i }));

    expect(transport.startProxy).toHaveBeenCalledOnce();
  });

  it('shows the endpoint and live metrics once running', () => {
    proxyState.current = { running: true, port: 8080 };
    dashboard.current = { snapshot: snapshot(), connected: true };

    render(<TrayPanel />);

    expect(screen.getByText('Running')).toBeInTheDocument();
    expect(screen.getByText('http://127.0.0.1:8080/v1')).toBeInTheDocument();
    expect(screen.getByText('Live')).toBeInTheDocument();
    expect(screen.getByText(/no active connections/i)).toBeInTheDocument();
  });

  it('stops the proxy when running', async () => {
    proxyState.current = { running: true, port: 8080 };
    render(<TrayPanel />);

    await userEvent.click(screen.getByRole('button', { name: /stop proxy/i }));

    expect(transport.stopProxy).toHaveBeenCalledOnce();
  });

  /**
   * The popover has no toast host, so a failure that only logged would leave
   * the button silently doing nothing.
   */
  it('reports a failed start in place', async () => {
    transport.startProxy.mockRejectedValueOnce(new Error('port in use'));
    render(<TrayPanel />);

    await userEvent.click(screen.getByRole('button', { name: /start proxy/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/could not start the proxy/i);
  });

  /**
   * The cause has to survive to the screen. Collapsing every failure into one
   * sentence is what made a refused connection — the app having torn down its
   * own API server — indistinguishable from a port conflict.
   */
  it('shows why the start failed, not just that it did', async () => {
    transport.startProxy.mockRejectedValueOnce(new Error('Failed to fetch'));
    render(<TrayPanel />);

    await userEvent.click(screen.getByRole('button', { name: /start proxy/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/failed to fetch/i);
  });

  /** A thrown non-Error must not render as "[object Object]". */
  it('still says something useful when the failure has no message', async () => {
    transport.startProxy.mockRejectedValueOnce(new Error(''));
    render(<TrayPanel />);

    await userEvent.click(screen.getByRole('button', { name: /start proxy/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/no further detail/i);
  });

  /** A long backend error must not blow the 360px popover open. */
  it('trims a very long failure message', async () => {
    transport.startProxy.mockRejectedValueOnce(new Error('x'.repeat(500)));
    render(<TrayPanel />);

    await userEvent.click(screen.getByRole('button', { name: /start proxy/i }));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent?.length ?? 0).toBeLessThan(240);
    expect(alert).toHaveTextContent(/…$/);
  });

  it('confirms a copied endpoint inline', async () => {
    proxyState.current = { running: true, port: 9000 };
    dashboard.current = { snapshot: snapshot(), connected: true };

    render(<TrayPanel />);
    await userEvent.click(screen.getByTitle('Copy URL'));

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('http://127.0.0.1:9000/v1');
    await waitFor(() => expect(screen.getByText('Copied to clipboard')).toBeInTheDocument());
  });

  /**
   * Opening the main window and quitting are tray-menu actions handled in
   * Rust. Buttons here would need Tauri IPC, which this panel does not use.
   */
  it('offers no window-level actions', () => {
    proxyState.current = { running: true, port: 8080 };
    render(<TrayPanel />);

    expect(screen.queryByRole('button', { name: /open gglib/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /quit/i })).not.toBeInTheDocument();
    expect(screen.getByText(/right-click the tray icon/i)).toBeInTheDocument();
  });
});
