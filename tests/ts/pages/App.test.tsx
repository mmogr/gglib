/**
 * The app shell, past setup: that it refuses a file dropped where nothing
 * takes it, so the desktop window never opens the file in place of the app.
 * `useFileDropGuard` is tested on its own; this pins that the shell mounts
 * it. The pages, the services and the menu are stubbed: only the shell's own
 * wiring is under test.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/places/endpoint/EndpointPlace', () => ({ default: () => <main aria-label="Endpoint" /> }));
vi.mock('../../../src/app/BenchmarkRoute', () => ({ BenchmarkRoute: () => <main aria-label="Benchmarks" /> }));
vi.mock('../../../src/sidebar/Sidebar', () => ({ Sidebar: () => <nav aria-label="gglib" /> }));
vi.mock('../../../src/components/SettingsModal', () => ({ default: () => null }));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
  syncMenuStateSilent: vi.fn(),
  setSelectedModel: vi.fn(async () => {}),
  listenToMenuEvents: vi.fn(async () => () => {}),
  MENU_EVENTS: {},
}));
vi.mock('../../../src/services/serverEvents', () => ({ initServerEvents: vi.fn(), cleanupServerEvents: vi.fn() }));
vi.mock('../../../src/services/proxyEvents', () => ({ initProxyEvents: vi.fn(), cleanupProxyEvents: vi.fn() }));
vi.mock('../../../src/services/remoteEvents', () => ({ initRemoteEvents: vi.fn(), cleanupRemoteEvents: vi.fn() }));
vi.mock('../../../src/services/modelEvents', () => ({ initModelEvents: vi.fn(), cleanupModelEvents: vi.fn() }));
vi.mock('../../../src/services/downloadsEvents', () => ({
  initDownloadEvents: vi.fn(),
  cleanupDownloadEvents: vi.fn(),
  onDownloadEndings: vi.fn(() => () => {}),
}));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => ({ getSettings: async () => ({}) }) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: async () => ({ setupCompleted: true }),
}));
vi.mock('../../../src/services/tools', () => ({ syncBuiltinTools: async () => {} }));

import { StrictMode } from 'react';
import App from '../../../src/App';
import { appRouter, disposeAppRouter } from '../../../src/app/router';

describe('App', () => {
  // Each render makes the browser router over jsdom's address; neither outlives its test.
  afterEach(() => {
    disposeAppRouter();
    window.history.replaceState(null, '', '/');
  });

  it('under StrictMode, the router the menu drives is the one on screen', async () => {
    // StrictMode runs the state initializer that creates the router twice;
    // a second router would take the menu's navigation off screen.
    render(<StrictMode><App /></StrictMode>);
    await screen.findByRole('main', { name: 'Library' });
    // The place sits in the shell: the sidebar is beside it.
    expect(screen.getByRole('navigation', { name: 'gglib' })).toBeInTheDocument();

    await act(() => appRouter().navigate('/diagnostics/benchmarks'));

    expect(await screen.findByRole('main', { name: 'Benchmarks' })).toBeInTheDocument();
  });

  it('refuses a file dropped anywhere nothing takes it, and leaves any other drag alone', async () => {
    render(<App />);
    await screen.findByRole('main', { name: 'Library' });
    const files = { types: ['Files'], files: [new File(['png'], 'shot.png', { type: 'image/png' })] };

    expect(fireEvent.dragOver(document.body, { dataTransfer: { ...files, dropEffect: 'copy' } })).toBe(false);
    expect(fireEvent.drop(document.body, { dataTransfer: files })).toBe(false);
    expect(fireEvent.drop(document.body, { dataTransfer: { types: ['text/plain'], files: [] } })).toBe(true);
  });
});
