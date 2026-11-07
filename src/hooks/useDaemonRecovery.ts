/**
 * Whether the daemon answers, with the registries kept to what it says.
 *
 * `useDaemonReachable`'s answer, plus what a reader of the registries needs
 * when the daemon comes and goes: when it goes, what the proxy, remote and
 * server registries held is let go, so nothing the old daemon said is drawn
 * as now; when it comes back, the event stream is woken (it would otherwise
 * sleep out a reconnect wait that grew while the daemon was gone, and it has
 * no backlog) and the proxy status, the servers and the remote status are
 * read again. A first answer needs no re-read: the bridges' own reads are on
 * their way. The tray popover and the main window's `ServiceGate` both use
 * it, so the two recover the same way.
 *
 * @module hooks/useDaemonRecovery
 */

import { useEffect, useRef } from 'react';
import { refreshProxyStatus } from '../services/proxyEvents';
import { resetProxyState } from '../services/proxyRegistry';
import { refreshRemoteStatus } from '../services/remoteEvents';
import { resetRemoteState } from '../services/remoteRegistry';
import { forgetServerList, refreshServers } from '../services/serverEvents';
import { getTransport } from '../services/transport';
import { useDaemonReachable, type Reachability } from './useDaemonReachable';

export function useDaemonRecovery(): Reachability {
  const reachable = useDaemonReachable();
  const wasDown = useRef(false);
  useEffect(() => {
    if (reachable === false) {
      wasDown.current = true;
      resetProxyState();
      resetRemoteState();
      forgetServerList();
    } else if (reachable === true && wasDown.current) {
      wasDown.current = false;
      getTransport().reconnectEventStreamNow();
      refreshProxyStatus();
      refreshServers();
      void refreshRemoteStatus();
    }
  }, [reachable]);
  return reachable;
}
