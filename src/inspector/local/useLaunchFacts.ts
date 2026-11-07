/**
 * How a loaded model was launched: its `ServerInfo.launch` from the
 * daemon's server list, read when the server registry says it is running
 * and again when its state or port changes (not on a health tick). `null` while it is not
 * loaded, before the list answers, and for a resident that recorded no
 * launch.
 *
 * @module inspector/local/useLaunchFacts
 */

import { useEffect, useState } from 'react';
import { useServerState } from '../../services/serverRegistry';
import { getTransport } from '../../services/transport';
import type { LaunchFacts } from '../../types/generated/LaunchFacts';

export function useLaunchFacts(modelId: number, loaded: boolean): LaunchFacts | null {
  const [facts, setFacts] = useState<LaunchFacts | null>(null);
  const server = useServerState(modelId);
  const status = server?.status;
  const port = server?.port;

  useEffect(() => {
    setFacts(null);
    if (!loaded) return;
    let live = true;
    Promise.resolve()
      .then(() => getTransport().listServers())
      .then((servers) => live && setFacts(servers.find((s) => s.model_id === modelId)?.launch ?? null))
      .catch(() => live && setFacts(null));
    return () => {
      live = false;
    };
  }, [modelId, loaded, status, port]);

  return facts;
}
