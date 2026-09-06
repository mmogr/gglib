/**
 * Remote Tunnel Events Initialization (ADR 0012)
 *
 * Bridges the `remote` SSE category into `remoteRegistry`, the way
 * `proxyEvents` does for the proxy, with one addition: every event also
 * triggers a status re-read. The tunnel's events are deliberately thin — a
 * fingerprint, a port, nothing a local GUI client should not see — so the
 * event moves the panel now and the re-read fills in paths, peers and
 * counters a moment later.
 *
 * Same hydration-race care as the proxy side: subscribe first, then fetch,
 * and drop a fetch that a live event overtook.
 */

import { subscribeSseEvent } from './transport/events/sse';
import { getTransport } from './transport';
import { applyRemoteStatus, ingestRemoteEvent, resetRemoteState } from './remoteRegistry';
import type { Unsubscribe } from './transport/types/common';
import type { RemoteEvent } from './transport/types/events';

let unsubscribe: Unsubscribe | null = null;
let eventVersion = 0;

/** Re-read the status; ignored if an event arrived while it was in flight. */
function refresh(): void {
  const versionBeforeFetch = eventVersion;
  getTransport()
    .getRemoteStatus()
    .then((status) => {
      if (eventVersion !== versionBeforeFetch) return;
      applyRemoteStatus(status);
    })
    .catch(() => {
      // Non-fatal: the next event, or the next open of the panel, tries again.
    });
}

/**
 * Initialize remote event handling.
 * Safe to call multiple times — only initializes once.
 */
export function initRemoteEvents(): void {
  if (unsubscribe) return;

  eventVersion = 0;

  // 1. Subscribe FIRST so no events are missed during hydration fetch
  unsubscribe = subscribeSseEvent('remote', (evt: RemoteEvent) => {
    eventVersion++;
    ingestRemoteEvent(evt);
    refresh();
  });

  // 2. Hydration fetch — seed initial state from the daemon
  refresh();
}

/** Ask the daemon again, for a panel that just opened. */
export function refreshRemoteStatus(): void {
  refresh();
}

/**
 * Cleanup remote event handling.
 * Should be called on app unmount or hot-reload.
 */
export function cleanupRemoteEvents(): void {
  if (unsubscribe) {
    unsubscribe();
    unsubscribe = null;
  }
  eventVersion = 0;
  resetRemoteState();
}
