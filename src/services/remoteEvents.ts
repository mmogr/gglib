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
 * Same hydration-race care as the proxy side, from the same `bridgeEvents`:
 * subscribe first, then fetch, and drop a fetch that a live event overtook.
 */

import { bridgeEvents } from './bridgeEvents';
import { getTransport } from './transport';
import { applyRemoteStatus, ingestRemoteEvent, resetRemoteState } from './remoteRegistry';

const bridge = bridgeEvents({
  category: 'remote',
  onEvent: (evt) => {
    ingestRemoteEvent(evt);
    // The event overtook any read still out, so that answer is dropped in
    // favour of this one's.
    void bridge.refresh();
  },
  read: () => getTransport().getRemoteStatus(),
  apply: applyRemoteStatus,
  reset: resetRemoteState,
});

/**
 * Initialize remote event handling.
 * Safe to call multiple times — only initializes once.
 */
export function initRemoteEvents(): void {
  bridge.init();
}

/**
 * Ask the daemon again, for a panel that just opened or that just wrote.
 *
 * Resolves `true` once a read at least as new as the call has been applied,
 * and `false` when the read failed — or was overtaken twice running, which
 * says the state is still moving. A read an event overtook is retried once
 * rather than reported: the read itself worked, and one started now is at
 * least as new as both. So a caller that has just *changed* something can
 * wait for the state to catch up, and can tell whether it did.
 */
export async function refreshRemoteStatus(): Promise<boolean> {
  const first = await bridge.refresh();
  return (first === 'superseded' ? await bridge.refresh() : first) === 'applied';
}

/**
 * Cleanup remote event handling.
 * Should be called on app unmount or hot-reload.
 */
export function cleanupRemoteEvents(): void {
  bridge.cleanup();
}
