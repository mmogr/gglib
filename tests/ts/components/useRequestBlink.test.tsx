/**
 * The endpoint dot's blink, at the hook: on for 150 ms when the request
 * count grows, at most once a second, never for the first frame (nor the
 * first after the stream was idle, whatever the count did meanwhile), nor
 * for a restarted proxy's lower count; its timer goes with it on unmount.
 * The drawn blink is pinned in `sidebar/endpointLine.test.tsx` and
 * `TrayPanel.test.tsx`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import { shouldBlink, useRequestBlink } from '../../../src/components/status/useRequestBlink';

describe('useRequestBlink', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
    vi.setSystemTime(10_000);
  });
  afterEach(() => vi.useRealTimers());

  const counted = (initial: number | null) =>
    renderHook(({ served }: { served: number | null }) => useRequestBlink(served), { initialProps: { served: initial } });

  it('the first frame after the stream was idle does not blink, even when the count rose meanwhile', () => {
    const hook = counted(5);
    vi.setSystemTime(20_000);
    hook.rerender({ served: null });
    hook.rerender({ served: 9 });
    expect(hook.result.current).toBe(false);

    vi.setSystemTime(30_000);
    hook.rerender({ served: 10 });
    expect(hook.result.current).toBe(true);
  });

  it('a blink ends after 150 ms, and leaves no timer behind when unmounted mid-blink', () => {
    const hook = counted(1);
    hook.rerender({ served: 2 });
    expect(hook.result.current).toBe(true);
    act(() => vi.advanceTimersByTime(150));
    expect(hook.result.current).toBe(false);

    vi.setSystemTime(20_000);
    hook.rerender({ served: 3 });
    expect(hook.result.current).toBe(true);
    hook.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('shouldBlink', () => {
  it("an unchanged or a restarted proxy's lower count does not blink, and its next request does", () => {
    expect(shouldBlink(null, 5, null, 0)).toBe(false);
    expect(shouldBlink(5, 5, null, 0)).toBe(false);
    expect(shouldBlink(40, 1, 1_000, 60_000)).toBe(false);
    expect(shouldBlink(1, 2, 1_000, 60_000)).toBe(true);
    expect(shouldBlink(1, 2, 1_000, 1_999)).toBe(false);
    expect(shouldBlink(1, 2, 1_000, 2_000)).toBe(true);
  });
});
