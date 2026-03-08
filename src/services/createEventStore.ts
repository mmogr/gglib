/**
 * Generic external store factory for event-driven state.
 *
 * Provides subscribe/getSnapshot/update/use pattern compatible with
 * React's useSyncExternalStore. Used by serverRegistry and proxyRegistry
 * to avoid duplicating store boilerplate.
 */

import { useSyncExternalStore } from 'react';

export interface EventStore<S> {
  /** Get the current state snapshot. */
  getState: () => S;
  /** Replace the entire state. */
  setState: (next: S) => void;
  /** Subscribe to state changes. Returns an unsubscribe function. */
  subscribe: (listener: () => void) => () => void;
  /** React hook — subscribes to the full store state. */
  useStore: () => S;
  /** React hook — subscribes with a selector for derived values. */
  useSelector: <T>(selector: (state: S) => T) => T;
}

/**
 * Create an external store backed by a mutable value.
 *
 * @param initial - Initial state value.
 */
export function createEventStore<S>(initial: S): EventStore<S> {
  let state = initial;
  const listeners = new Set<() => void>();

  function notify(): void {
    listeners.forEach((fn) => fn());
  }

  function getState(): S {
    return state;
  }

  function setState(next: S): void {
    state = next;
    notify();
  }

  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }

  function useStore(): S {
    return useSyncExternalStore(subscribe, getState, getState);
  }

  function useSelector<T>(selector: (s: S) => T): T {
    return useSyncExternalStore(
      subscribe,
      () => selector(state),
      () => selector(state),
    );
  }

  return { getState, setState, subscribe, useStore, useSelector };
}
