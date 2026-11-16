/**
 * The tray popover while the desktop app's launch of gglib is in flight,
 * over the real transport: the app's lookup of the daemon's address
 * (`get_embedded_api_info`) is out until the launch settles, and nothing
 * listens on the daemon's port. The popover says gglib is starting, never
 * that it is not running; a launch that failed is then said as not running.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

const STARTING = 'gglib is starting…';
const DOWN = "gglib isn't running, so nothing can be served.";

/** The lookup, out until `settle`; nothing answers on the port meanwhile. */
function launchInFlight() {
  let failed = false;
  const waiting: Array<() => void> = [];
  (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
    invoke: vi.fn((cmd: string) => {
      if (cmd !== 'get_embedded_api_info') return Promise.resolve(null);
      if (failed) return Promise.reject('the gglib daemon did not come up within 15s');
      return new Promise((_resolve, reject) =>
        waiting.push(() => reject('the gglib daemon did not come up within 15s')),
      );
    }),
  };
  return async () => {
    failed = true;
    await act(async () => {
      for (const settle of waiting.splice(0)) settle();
    });
  };
}

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      throw new TypeError('Failed to fetch');
    }),
  );
});

afterEach(() => {
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('the tray popover while the launch is in flight', () => {
  it("says gglib is starting, then that it isn't running when the launch failed", async () => {
    const fail = launchInFlight();
    const { TrayPanel } = await import('../../../src/tray/TrayPanel');
    render(<TrayPanel />);

    expect(await screen.findByText(STARTING)).toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(4000));
    expect(screen.getByText(STARTING)).toBeInTheDocument();
    expect(screen.queryByText(DOWN)).not.toBeInTheDocument();

    await fail();
    expect(await screen.findByText(DOWN)).toBeInTheDocument();
    expect(screen.queryByText(STARTING)).not.toBeInTheDocument();
  });
});
