/**
 * Whether this is the desktop app is asked in one place, `isDesktop()`.
 *
 * It used to be asked four ways: a constant worked out when its module
 * loaded, a function that returned the constant, a private check inside the
 * transport's client, and another inside the log transport. They agreed in
 * the three places the app runs (a window of the desktop app, its tray panel,
 * a browser tab) and that agreement is what is pinned here, with the two
 * consumers that had checks of their own.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { isDesktop } from '../../../src/services/platform/detect';
import { isDesktop as isDesktopFromPlatform } from '../../../src/services/platform';
import { TauriTracingTransport } from '../../../src/services/platform/logging/transports';
import type { LogEntry } from '../../../src/services/platform/logging/types';

type TauriWindow = { __TAURI_INTERNALS__?: unknown; __TAURI__?: unknown };

/** The bridge Tauri v2 puts on each of the desktop app's windows. */
function enterDesktop(invoke = vi.fn().mockResolvedValue(undefined)) {
  Object.assign(window, { __TAURI_INTERNALS__: { invoke } });
  return invoke;
}

function leaveDesktop() {
  delete (window as TauriWindow).__TAURI_INTERNALS__;
  delete (window as TauriWindow).__TAURI__;
}

describe('desktop detection', () => {
  beforeEach(leaveDesktop);
  afterEach(leaveDesktop);

  it('is false in a browser tab, which has no Tauri bridge', () => {
    expect(isDesktop()).toBe(false);
  });

  it.each(['the main window', 'the tray panel'])(
    'is true in %s of the desktop app, each of which has the bridge',
    () => {
      enterDesktop();
      expect(isDesktop()).toBe(true);
    },
  );

  it('is asked on each call, so the answer does not depend on when a module loaded', () => {
    expect(isDesktop()).toBe(false);
    enterDesktop();
    expect(isDesktop()).toBe(true);
    leaveDesktop();
    expect(isDesktop()).toBe(false);
  });

  it('is the same function for UI code and for the transport', () => {
    expect(isDesktopFromPlatform).toBe(isDesktop);
  });

  it('does not take Tauri v1\'s global, which this app never sets, for the desktop app', () => {
    Object.assign(window, { __TAURI__: {} });
    expect(isDesktop()).toBe(false);
  });
});

describe('the transport client', () => {
  beforeEach(() => {
    leaveDesktop();
    vi.resetModules();
    vi.stubGlobal('fetch', vi.fn(async () => new Response('[]', { status: 200 })));
  });

  afterEach(() => {
    leaveDesktop();
    vi.unstubAllGlobals();
  });

  it('asks Tauri where the daemon is in the desktop app', async () => {
    const invoke = enterDesktop(vi.fn().mockResolvedValue({ port: 9887, token: 'desk-token' }));
    const { get } = await import('../../../src/services/transport/api/client');

    await get('/api/models');

    expect(invoke).toHaveBeenCalledWith('get_embedded_api_info');
    expect(vi.mocked(fetch).mock.calls[0][0]).toBe('http://127.0.0.1:9887/api/models');
  });

  it('asks its own origin in a browser tab', async () => {
    const { get } = await import('../../../src/services/transport/api/client');

    await get('/api/models');

    expect(vi.mocked(fetch).mock.calls[0][0]).toBe('/api/models');
  });
});

describe('the Tauri log transport', () => {
  const entry: LogEntry = { timestamp: '2026-10-06T00:00:00.000Z', level: 'info', category: 'platform.system', message: 'hello', data: { a: 1 } };

  beforeEach(leaveDesktop);
  afterEach(leaveDesktop);

  it('hands an entry to the desktop process in the desktop app', async () => {
    const invoke = enterDesktop();

    new TauriTracingTransport().write(entry);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalled());

    expect(invoke).toHaveBeenCalledWith('log_from_frontend', {
      entry: { timestamp: '2026-10-06T00:00:00.000Z', level: 'info', category: 'platform.system', message: 'hello', data: '{"a":1}' },
    });
  });

  it('writes nothing in a browser tab, and does not throw', () => {
    expect(() => new TauriTracingTransport().write(entry)).not.toThrow();
  });

  it('writes nothing when it is switched off, desktop or not', async () => {
    const invoke = enterDesktop();

    new TauriTracingTransport(false).write(entry);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(invoke).not.toHaveBeenCalled();
  });
});
