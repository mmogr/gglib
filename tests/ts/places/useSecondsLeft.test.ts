/**
 * `places/devices/useSecondsLeft`: the seconds left read from the deadline
 * at once when it changes, `null` once there is no code, and no timer left
 * behind when the reader goes.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useSecondsLeft } from '../../../src/places/devices/useSecondsLeft';

describe('useSecondsLeft', () => {
  afterEach(() => vi.useRealTimers());

  it('reads a new deadline at once, not at the next tick', () => {
    vi.useFakeTimers();
    const now = Date.now();
    const { result, rerender } = renderHook(({ at }: { at: number | null }) => useSecondsLeft(at), {
      initialProps: { at: now + 30_000 },
    });
    expect(result.current).toBe(30);
    rerender({ at: now + 90_000 });
    expect(result.current).toBe(90);
  });

  it('is null once there is no code', () => {
    vi.useFakeTimers();
    const { result, rerender } = renderHook(({ at }: { at: number | null }) => useSecondsLeft(at), {
      initialProps: { at: Date.now() + 30_000 } as { at: number | null },
    });
    rerender({ at: null });
    expect(result.current).toBeNull();
  });

  it('leaves no timer behind when its reader goes', () => {
    vi.useFakeTimers();
    const { unmount } = renderHook(() => useSecondsLeft(Date.now() + 30_000));
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
