/**
 * Whether the daemon answers, with the registries kept to what it says.
 *
 * `useDaemonReachable`'s answer, plus what a reader of the registries needs
 * when the daemon comes and goes: when it goes, what the proxy, remote and
 * server registries held is let go, so nothing the old daemon said is drawn
 * as now; when it comes back, the event stream is woken, since it would
 * otherwise sleep out a reconnect wait that grew while the daemon was gone
 * (or, still connected through a probe that failed, announces its opening
 * again). Nothing is read here: every store reads again when the stream
 * opens (`keptCurrent`), so the stream opening is what fills them, the first
 * time and after every return. A stream that has opened since the daemon
 * went is not woken: its opening has read them already. The tray popover
 * and the main window's `ServiceGate` both use it, so the two recover the
 * same way.
 *
 * @module hooks/useDaemonRecovery
 */

import { useEffect, useRef } from 'react';
import { resetProxyState } from '../services/proxyRegistry';
import { resetRemoteState } from '../services/remoteRegistry';
import { forgetServerList } from '../services/serverEvents';
import { getTransport } from '../services/transport';
import { useDaemonReachable, type Reachability } from './useDaemonReachable';

export function useDaemonRecovery(): Reachability {
  const reachable = useDaemonReachable();
  const wasDown = useRef(false);
  const openedSinceDown = useRef(false);
  useEffect(() => {
    return getTransport().onEventStreamOpen(() => {
      openedSinceDown.current = true;
    });
  }, []);
  useEffect(() => {
    if (reachable === false) {
      wasDown.current = true;
      openedSinceDown.current = false;
      resetProxyState();
      resetRemoteState();
      forgetServerList();
    } else if (reachable === true && wasDown.current) {
      wasDown.current = false;
      if (!openedSinceDown.current) getTransport().reconnectEventStreamNow();
    }
  }, [reachable]);
  return reachable;
}
