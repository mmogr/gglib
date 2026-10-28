/**
 * `useProxyKey`: the proxy's key, revealed when a surface needs it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

import { useProxyKey } from '../../../src/hooks/useProxyKey';

const { revealSecret } = vi.hoisted(() => ({ revealSecret: vi.fn() }));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ revealSecret }),
}));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

const PROXY_KEY = { kind: 'setting', secret: 'proxy_api_key' };

describe('useProxyKey', () => {
  beforeEach(() => revealSecret.mockReset());

  it('asks for nothing while inactive', () => {
    const { result } = renderHook(() => useProxyKey(false));

    expect(result.current).toBeUndefined();
    expect(revealSecret).not.toHaveBeenCalled();
  });

  it('reveals the key when it turns active, and drops it when inactive', async () => {
    revealSecret.mockResolvedValue('k3y-5f3a');
    const { result, rerender } = renderHook(({ active }) => useProxyKey(active), {
      initialProps: { active: true },
    });

    await waitFor(() => expect(result.current).toBe('k3y-5f3a'));
    expect(revealSecret).toHaveBeenCalledWith(PROXY_KEY);

    rerender({ active: false });
    expect(result.current).toBeUndefined();
  });

  it('answers null when no key is stored, or it could not be read', async () => {
    revealSecret.mockResolvedValueOnce(null);
    const stored = renderHook(() => useProxyKey(true));
    await waitFor(() => expect(stored.result.current).toBeNull());

    revealSecret.mockRejectedValueOnce(new Error('daemon down'));
    const failed = renderHook(() => useProxyKey(true));
    await waitFor(() => expect(failed.result.current).toBeNull());
  });

  it('asks again when what it was told to follow changes, so a new key is presented', async () => {
    revealSecret.mockResolvedValueOnce('old').mockResolvedValueOnce('new');
    const { result, rerender } = renderHook(({ asOf }) => useProxyKey(true, asOf), {
      initialProps: { asOf: 1 },
    });
    await waitFor(() => expect(result.current).toBe('old'));

    rerender({ asOf: 2 });

    await waitFor(() => expect(result.current).toBe('new'));
    expect(revealSecret).toHaveBeenCalledTimes(2);
  });
});
