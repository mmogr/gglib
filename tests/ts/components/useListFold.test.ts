/**
 * The conversation list's fold: its default, the remembered choice, and
 * folding by itself in a window too narrow for list and notebook.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import {
  LIST_FITS_QUERY,
  LIST_STORAGE_KEY,
  useListFold,
} from '../../../src/components/ConversationListPanel/useListFold';

/** A window `fits` wide, which the test can resize. */
function windowThatFits(fits: boolean) {
  const listeners = new Set<() => void>();
  const query = {
    media: LIST_FITS_QUERY,
    matches: fits,
    addEventListener: (_: string, f: () => void) => listeners.add(f),
    removeEventListener: (_: string, f: () => void) => listeners.delete(f),
  };
  window.matchMedia = ((q: string) => (q === LIST_FITS_QUERY ? query : { ...query, matches: false })) as never;
  return {
    resize(next: boolean) {
      query.matches = next;
      listeners.forEach((f) => f());
    },
  };
}

const original = window.matchMedia;

beforeEach(() => {
  window.localStorage.clear();
  windowThatFits(true);
});

afterEach(() => {
  window.matchMedia = original;
});

describe('useListFold', () => {
  it('shows the list by default, with a control to fold it', () => {
    const { result } = renderHook(() => useListFold());
    expect(result.current.open).toBe(true);
    expect(result.current.canFold).toBe(true);
  });

  it('remembers a fold', () => {
    const first = renderHook(() => useListFold());
    act(() => first.result.current.toggle());
    expect(first.result.current.open).toBe(false);
    expect(window.localStorage.getItem(LIST_STORAGE_KEY)).toBe('folded');
    first.unmount();

    const again = renderHook(() => useListFold());
    expect(again.result.current.open).toBe(false);
    act(() => again.result.current.toggle());
    expect(renderHook(() => useListFold()).result.current.open).toBe(true);
  });

  it('folds by itself in a narrow window, and does not remember unfolding there', () => {
    const screen = windowThatFits(true);
    const { result } = renderHook(() => useListFold());
    expect(result.current.open).toBe(true);

    act(() => screen.resize(false));
    expect(result.current.open).toBe(false);

    act(() => result.current.unfold());
    expect(result.current.open).toBe(true);
    expect(window.localStorage.getItem(LIST_STORAGE_KEY)).toBeNull();

    act(() => screen.resize(true));
    expect(result.current.open).toBe(true);
  });

  it('starts folded under the rail-only policy, and never folds under always', () => {
    expect(renderHook(() => useListFold('folded')).result.current.open).toBe(false);

    windowThatFits(false);
    const always = renderHook(() => useListFold('always')).result.current;
    expect(always).toMatchObject({ open: true, canFold: false });
  });
});
