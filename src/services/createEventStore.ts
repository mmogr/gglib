/**
 * Generic external store factory for event-driven state.
 *
 * Provides subscribe/getSnapshot/update/use pattern compatible with
 * React's useSyncExternalStore. Used by serverRegistry, proxyRegistry and
 * remoteRegistry to avoid duplicating store boilerplate. What feeds one from
 * the daemon's events is `bridgeEvents`.
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

  return { getState, setState, subscribe, useStore };
}
