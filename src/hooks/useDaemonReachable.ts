/**
 * Whether the gglib daemon is answering.
 *
 * Every other surface in the app can assume a daemon: the main window is only
 * reachable once one has been found, and the native tray menu reads the Rust
 * watcher's snapshot, which has "unreachable" as a first-class state. The tray
 * *popover* had neither. It rendered `useProxyState()`, which reports
 * `{running: false}` for a stopped proxy and for a daemon that is not there —
 * so a machine with no daemon showed "The proxy is stopped", and the Start
 * button fell into a connection error.
 *
 * Polling rather than reacting to a failed call: a reachability signal has to
 * recover on its own, and nothing else in the popover would ask again once its
 * one fetch had failed. Absolute each time, so it cannot drift.
 *
 * @module hooks/useDaemonReachable
 */

import { useEffect, useState } from 'react';
import { getTransport } from '../services/transport';

/** How often to ask. Matches the Rust watcher's interval for the same reason:
 *  under the threshold at which a tray surface reads as stale. */
const POLL_INTERVAL_MS = 2000;

/**
 * `null` until the first answer — the popover opens without claiming either
 * way, because "not running" flashed at a healthy daemon is worse than a
 * moment of nothing.
 */
export type Reachability = boolean | null;

/** Poll the daemon, reporting whether it answers. */
export function useDaemonReachable(): Reachability {
  const [reachable, setReachable] = useState<Reachability>(null);

  useEffect(() => {
    let cancelled = false;

    const probe = async () => {
      try {
        await getTransport().getProxyStatus();
        if (!cancelled) setReachable(true);
      } catch {
        // Any failure is the same answer: nothing is serving this app's API.
        // Distinguishing a refused connection from a 500 would be a claim
        // about the daemon's health that this poll cannot support.
        if (!cancelled) setReachable(false);
      }
    };

    void probe();
    const timer = setInterval(() => void probe(), POLL_INTERVAL_MS);

    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  return reachable;
}
