/**
 * The app shell learns whether llama.cpp is installed from the daemon.
 *
 * It used to ask a Tauri command, which re-derived the answer in the desktop
 * process and did not exist in a browser, where the hook reported "installed"
 * without looking. It reads the setup-status route now, on both surfaces. The
 * transport is the real one over a stubbed `fetch`, so the route is pinned.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../src/services/platform', async (original) => ({
  ...(await original<typeof import('../../../src/services/platform')>()),
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { useLlamaStatus } from '../../../src/hooks/useLlamaStatus';
import { resetClientCache, setApiSession } from '../../../src/services/transport/api/client';
import { mockInvoke } from '../setup';

/** A setup-status reply carrying the two fields the shell reads. */
const setupStatus = (llamaInstalled: boolean, llamaCanDownload: boolean) =>
  new Response(JSON.stringify({ setupCompleted: true, llamaInstalled, llamaCanDownload }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

describe.each([
  ['the desktop app', 'http://127.0.0.1:9887'],
  ['a browser tab', ''],
])('useLlamaStatus in %s', (_surface, daemon) => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let bridge: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', undefined);
    bridge = vi.fn().mockResolvedValue({ port: 9887, token: 'desk-token' });
    if (daemon) Object.assign(window, { __TAURI_INTERNALS__: { invoke: bridge } });
  });

  afterEach(() => {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    vi.unstubAllGlobals();
  });

  it('reads installed and downloadable from the daemon\'s setup-status route, and from no Tauri command', async () => {
    fetchMock.mockResolvedValue(setupStatus(false, true));

    const { result } = renderHook(() => useLlamaStatus());
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.status).toEqual({ installed: false, canDownload: true });
    expect(result.current.error).toBeNull();
    expect(fetchMock.mock.calls.map((call) => call[0])).toEqual([`${daemon}/api/config/system/setup-status`]);
    // The desktop app asks its shell where the daemon is, and nothing else.
    expect(bridge.mock.calls).toEqual(daemon ? [['get_embedded_api_info']] : []);
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('reports each field as the daemon sent it', async () => {
    fetchMock.mockResolvedValue(setupStatus(true, false));

    const { result } = renderHook(() => useLlamaStatus());
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.status).toEqual({ installed: true, canDownload: false });
  });

  it('assumes llama.cpp is installed, and keeps the daemon\'s reason, when the status cannot be read', async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify({ error: 'settings are locked', status: 500 }), {
        status: 500,
        headers: { 'content-type': 'application/json' },
      }),
    );

    const { result } = renderHook(() => useLlamaStatus());
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.status).toEqual({ installed: true, canDownload: false });
    expect(result.current.error).toBe('Failed to check llama status: settings are locked');
  });

  it('asks the daemon again when the status is checked again', async () => {
    fetchMock.mockResolvedValueOnce(setupStatus(false, true)).mockResolvedValueOnce(setupStatus(true, true));

    const { result } = renderHook(() => useLlamaStatus());
    await waitFor(() => expect(result.current.status).toEqual({ installed: false, canDownload: true }));

    await act(() => result.current.checkStatus());

    expect(result.current.status).toEqual({ installed: true, canDownload: true });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
