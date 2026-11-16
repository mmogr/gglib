/**
 * A second launch of the desktop app, as the main window hears it.
 *
 * Rust shows the first window and sends it `second-launch` with what the
 * second launch was started with. Nothing acts on it yet; the listener keeps
 * the latest, typed, where a later feature (deep links) can read it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

type Handler = (event: { payload: unknown }) => void;

const shell = vi.hoisted(() => ({
  handlers: new Map<string, Handler>(),
  unlisten: vi.fn(),
  listen: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({ listen: shell.listen }));

/** A fresh copy of the module, so no test sees another's last launch. */
const load = () => import('../../../src/services/platform/secondLaunch');

describe('a second launch', () => {
  beforeEach(() => {
    vi.resetModules();
    shell.handlers.clear();
    shell.unlisten.mockReset();
    shell.listen.mockReset();
    shell.listen.mockImplementation(async (event: string, handler: Handler) => {
      shell.handlers.set(event, handler);
      return shell.unlisten;
    });
    window.__TAURI_INTERNALS__ = { invoke: vi.fn() as never };
  });

  afterEach(() => {
    delete window.__TAURI_INTERNALS__;
  });

  it('is heard on the desktop, and its arguments and folder are kept', async () => {
    const { listenToSecondLaunch, lastSecondLaunch, SECOND_LAUNCH_EVENT } = await load();

    listenToSecondLaunch();
    await vi.waitFor(() => expect(shell.handlers.has(SECOND_LAUNCH_EVENT)).toBe(true));
    expect(lastSecondLaunch()).toBeNull();

    shell.handlers.get(SECOND_LAUNCH_EVENT)!({ payload: { args: ['gglib://open', '--x'], cwd: '/home/someone' } });

    expect(lastSecondLaunch()).toEqual({ args: ['gglib://open', '--x'], cwd: '/home/someone' });
  });

  it('keeps the latest of several', async () => {
    const { listenToSecondLaunch, lastSecondLaunch, SECOND_LAUNCH_EVENT } = await load();

    listenToSecondLaunch();
    await vi.waitFor(() => expect(shell.handlers.has(SECOND_LAUNCH_EVENT)).toBe(true));
    const send = shell.handlers.get(SECOND_LAUNCH_EVENT)!;
    send({ payload: { args: ['first'], cwd: '/a' } });
    send({ payload: { args: [], cwd: '/b' } });

    expect(lastSecondLaunch()).toEqual({ args: [], cwd: '/b' });
  });

  it('stops being heard once stopped, even before the listener was registered', async () => {
    const { listenToSecondLaunch } = await load();

    const stop = listenToSecondLaunch();
    stop();

    await vi.waitFor(() => expect(shell.unlisten).toHaveBeenCalledTimes(1));
  });

  it('is not listened for in a browser, where there is no second launch', async () => {
    delete window.__TAURI_INTERNALS__;
    const { listenToSecondLaunch, lastSecondLaunch } = await load();

    listenToSecondLaunch()();
    await Promise.resolve();

    expect(shell.listen).not.toHaveBeenCalled();
    expect(lastSecondLaunch()).toBeNull();
  });
});
