/**
 * The paired machine's rows, against what the status says of it: read while
 * it is reached, kept and marked while it is away, a read fails or a new
 * connection names no machine yet, cleared on a disconnection, and never
 * shown for a machine that is no longer the one answering; and never read
 * for a caller that does not need them.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { FAR_FINGERPRINT, FakeFarDaemon, farEntry } from '../fixtures/fakeFarDaemon';
import { usePairedModels } from '../../../src/hooks/usePairedModels';
import {
  IDLE_STATUS,
  applyRemoteStatus,
  ingestRemoteEvent,
  resetRemoteState,
} from '../../../src/services/remoteRegistry';

const connected = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: FAR_FINGERPRINT,
  path: 'direct',
  away_for_s: null,
};
const status = { ...IDLE_STATUS, connected, paired_name: 'desk' };

let daemons: FakeFarDaemon;

beforeEach(() => {
  daemons = new FakeFarDaemon();
  vi.stubGlobal('fetch', vi.fn(daemons.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

/** The ids of the rows the hook shows. */
const ids = (hook: { result: { current: ReturnType<typeof usePairedModels> } }) =>
  hook.result.current.group?.models.map((m) => m.id) ?? null;

describe('usePairedModels', () => {
  it('reads the rows while connected, base entries only, under the machine’s name', async () => {
    act(() => applyRemoteStatus(status));
    const hook = renderHook(() => usePairedModels());

    await waitFor(() => expect(ids(hook)).toEqual(['qwen3']));
    expect(hook.result.current.name).toBe('desk');
    expect(hook.result.current.reach).toBe('reached');
    expect(hook.result.current.group?.actions).toEqual(['list', 'detail', 'chat', 'load']);
  });

  it('asks nothing with nothing connected', async () => {
    const hook = renderHook(() => usePairedModels());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(hook.result.current.group).toBeNull();
    expect(daemons.farCount('GET', '/api/remote/models')).toBe(0);
  });

  it('reads nothing for a caller that does not need them, not on connect, on focus or when asked, and gives it no rows', async () => {
    act(() => applyRemoteStatus(status));
    const unneeded = renderHook(({ enabled }) => usePairedModels(enabled), { initialProps: { enabled: false } });
    // A caller that needs them, beside it, shows each of those happened.
    const needed = renderHook(() => usePairedModels());
    await waitFor(() => expect(ids(needed)).toEqual(['qwen3']));
    act(() => {
      window.dispatchEvent(new Event('focus'));
    });
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(2));
    act(() => unneeded.result.current.refetch());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });

    expect(daemons.farCount('GET', '/api/remote/models')).toBe(2);
    expect(unneeded.result.current.group).toBeNull();

    unneeded.rerender({ enabled: true });
    await waitFor(() => expect(ids(unneeded)).toEqual(['qwen3']));
    expect(daemons.farCount('GET', '/api/remote/models')).toBe(3);

    // Rows read while it needed them are not given once it does not.
    unneeded.rerender({ enabled: false });
    expect(unneeded.result.current.group).toBeNull();
  });

  it('keeps the rows, marked away, while the machine is away, and reads again when it is back', async () => {
    act(() => applyRemoteStatus(status));
    const hook = renderHook(() => usePairedModels());
    await waitFor(() => expect(ids(hook)).toEqual(['qwen3']));

    act(() => ingestRemoteEvent({ type: 'remote_away', port: 41234 }));
    expect(hook.result.current.reach).toBe('away');
    expect(ids(hook)).toEqual(['qwen3']);
    expect(daemons.farCount('GET', '/api/remote/models')).toBe(1);

    daemons.entries = [farEntry('qwen3', 3), farEntry('llama', 1000)];
    act(() => ingestRemoteEvent({ type: 'remote_back', port: 41234 }));
    await waitFor(() => expect(ids(hook)).toEqual(['qwen3', 'llama']));
    expect(hook.result.current.reach).toBe('reached');
  });

  it('keeps the last rows, marked stale, when a read fails', async () => {
    act(() => applyRemoteStatus(status));
    const hook = renderHook(() => usePairedModels());
    await waitFor(() => expect(ids(hook)).toEqual(['qwen3']));

    daemons.modelsFail = 1;
    act(() => hook.result.current.refetch());
    await waitFor(() => expect(hook.result.current.reach).toBe('stale'));
    expect(ids(hook)).toEqual(['qwen3']);

    act(() => hook.result.current.refetch());
    await waitFor(() => expect(hook.result.current.reach).toBe('reached'));
  });

  it('reads again when the window regains focus', async () => {
    act(() => applyRemoteStatus(status));
    renderHook(() => usePairedModels());
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(1));

    act(() => {
      window.dispatchEvent(new Event('focus'));
    });
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(2));
  });

  it('clears the rows on a disconnection', async () => {
    act(() => applyRemoteStatus(status));
    const hook = renderHook(() => usePairedModels());
    await waitFor(() => expect(ids(hook)).toEqual(['qwen3']));

    act(() => ingestRemoteEvent({ type: 'remote_disconnected' }));
    expect(hook.result.current.group).toBeNull();
  });

  it('marks the rows stale while a new connection names no machine, until the status does', async () => {
    act(() => applyRemoteStatus(status));
    const hook = renderHook(() => usePairedModels());
    await waitFor(() => expect(ids(hook)).toEqual(['qwen3']));

    // The daemon acts on whoever answered, where these ids may be other models.
    act(() => ingestRemoteEvent({ type: 'remote_joined', port: 41235 }));
    expect(hook.result.current.reach).toBe('stale');
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(2));
    expect(hook.result.current.reach).toBe('stale');

    act(() => applyRemoteStatus(status));
    expect(hook.result.current.reach).toBe('reached');
    expect(ids(hook)).toEqual(['qwen3']);
  });

  it('never shows one machine’s rows once another answers, even if its read fails', async () => {
    act(() => applyRemoteStatus(status));
    const hook = renderHook(() => usePairedModels());
    await waitFor(() => expect(ids(hook)).toEqual(['qwen3']));

    daemons.modelsFail = 1;
    act(() =>
      applyRemoteStatus({
        ...status,
        connected: { ...connected, ticket_fingerprint: 'ffee11223344' },
        paired_name: 'laptop',
      }),
    );
    expect(hook.result.current.group).toBeNull();
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(2));
    expect(hook.result.current.group).toBeNull();
  });
});
