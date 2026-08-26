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
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { ReactNode } from 'react';

import ProxyControl from '../../../src/components/ProxyControl';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { appSettings } from '../fixtures/settings';
import type { AppSettings } from '../../../src/types';

const settingsRef: { current: AppSettings | null } = { current: null };

vi.mock('../../../src/hooks/useSettings', () => ({
  useSettings: () => ({ settings: settingsRef.current }),
}));
vi.mock('../../../src/services/proxyRegistry', () => ({
  useProxyState: () => ({ running: false, port: null }),
}));
vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({}),
}));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  clearProxyCache: vi.fn(),
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
