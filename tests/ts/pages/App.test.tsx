/**
 * The app shell, past setup: that it refuses a file dropped where nothing
 * takes it, so the desktop window never opens the file in place of the app.
 * `useFileDropGuard` is tested on its own; this pins that the shell mounts
 * it. The pages, the services and the menu are stubbed: only the shell's own
 * wiring is under test.
 */

import { describe, it, expect, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

vi.mock('../../../src/pages/ModelControlCenterPage', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/components/Header', () => ({ default: () => null }));
vi.mock('../../../src/components/SettingsModal', () => ({ default: () => null }));
vi.mock('../../../src/components/LlamaInstallModal', () => ({ default: () => null }));
vi.mock('../../../src/components/SetupWizard', () => ({ default: () => null }));
vi.mock('../../../src/hooks/useServers', () => {
  const servers = { servers: [], stopServer: vi.fn() };
  return { useServers: () => servers };
});
vi.mock('../../../src/hooks/useLlamaStatus', () => {
  const llama = {
    status: { installed: true, canDownload: false },
    loading: false,
    error: null,
    checkStatus: vi.fn(),
  };
  return { useLlamaStatus: () => llama };
});
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
  syncMenuStateSilent: vi.fn(),
  listenToMenuEvents: vi.fn(async () => () => {}),
  MENU_EVENTS: {},
}));
vi.mock('../../../src/services/serverEvents', () => ({ initServerEvents: vi.fn(), cleanupServerEvents: vi.fn() }));
vi.mock('../../../src/services/proxyEvents', () => ({ initProxyEvents: vi.fn(), cleanupProxyEvents: vi.fn() }));
vi.mock('../../../src/services/remoteEvents', () => ({ initRemoteEvents: vi.fn(), cleanupRemoteEvents: vi.fn() }));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => ({ getSettings: async () => ({}) }) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: async () => ({ setupCompleted: true }),
}));
vi.mock('../../../src/services/tools', () => ({ syncBuiltinTools: async () => {} }));

import App from '../../../src/App';

describe('App', () => {
  it('refuses a file dropped anywhere nothing takes it, and leaves any other drag alone', async () => {
    render(<App />);
    await screen.findByRole('main', { name: 'Library' });
    const files = { types: ['Files'], files: [new File(['png'], 'shot.png', { type: 'image/png' })] };

    expect(fireEvent.dragOver(document.body, { dataTransfer: { ...files, dropEffect: 'copy' } })).toBe(false);
    expect(fireEvent.drop(document.body, { dataTransfer: files })).toBe(false);
    expect(fireEvent.drop(document.body, { dataTransfer: { types: ['text/plain'], files: [] } })).toBe(true);
  });
});
