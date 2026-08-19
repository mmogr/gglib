/**
 * The subscription that lets one client see another client's edits.
 *
 * Before this existed the library refreshed only when the tab in front of you
 * made the change, so a second window or browser tab editing the same daemon
 * left the list confidently stale.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';

import { useModelLibraryEvents } from '../../../src/hooks/useModelLibraryEvents';
import type { ModelEvent } from '../../../src/services/transport/types/events';

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

/** The handler the hook registered, so a test can play events into it. */
let emit: (event: ModelEvent) => void;
const unsubscribe = vi.fn();

const WINDOW_MS = 50;

const added: ModelEvent = {
  type: 'model_added',
  model: { id: 7, name: 'llama-7b', filePath: '/models/llama-7b.gguf' },
};

beforeEach(() => {
  vi.useFakeTimers();
  unsubscribe.mockClear();
  transport.subscribe.mockImplementation((category: string, handler: (e: ModelEvent) => void) => {
    expect(category).toBe('model');
    emit = handler;
    return unsubscribe;
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useModelLibraryEvents', () => {
  it('reloads once a model event arrives', () => {
    const onChange = vi.fn();
    renderHook(() => useModelLibraryEvents(onChange, WINDOW_MS));

    emit(added);
    expect(onChange).not.toHaveBeenCalled();

    vi.advanceTimersByTime(WINDOW_MS);
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  /**
   * Importing a directory emits one event per model. Each is a reason to
   * refetch the list once, not a reason to refetch it per model.
   */
  it('collapses a burst into a single reload', () => {
    const onChange = vi.fn();
    renderHook(() => useModelLibraryEvents(onChange, WINDOW_MS));

    for (let i = 0; i < 10; i++) {
      emit({ ...added, model: { ...added.model, id: i } } as ModelEvent);
    }

    vi.advanceTimersByTime(WINDOW_MS);
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it('reloads again for a later, separate burst', () => {
    const onChange = vi.fn();
    renderHook(() => useModelLibraryEvents(onChange, WINDOW_MS));

    emit(added);
    vi.advanceTimersByTime(WINDOW_MS);

    emit({ type: 'model_removed', modelId: 7 });
    vi.advanceTimersByTime(WINDOW_MS);

    expect(onChange).toHaveBeenCalledTimes(2);
  });

  /**
   * The callback is read through a ref, so a caller passing an inline closure
   * does not tear the subscription down and rebuild it on every render — and
   * the latest callback is still the one that runs.
   */
  it('keeps one subscription across renders and calls the latest callback', () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(({ cb }) => useModelLibraryEvents(cb, WINDOW_MS), {
      initialProps: { cb: first },
    });

    rerender({ cb: second });
    expect(transport.subscribe).toHaveBeenCalledTimes(1);

    emit(added);
    vi.advanceTimersByTime(WINDOW_MS);

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('unsubscribes and drops a pending batch on unmount', () => {
    const onChange = vi.fn();
    const { unmount } = renderHook(() => useModelLibraryEvents(onChange, WINDOW_MS));

    emit(added);
    unmount();
    vi.advanceTimersByTime(WINDOW_MS);

    expect(unsubscribe).toHaveBeenCalledTimes(1);
    expect(onChange).not.toHaveBeenCalled();
  });
});
