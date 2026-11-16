/**
 * The service's host kept in `hostRegistry`: read from `GET /api/version`
 * (`host`) now and again each time the event stream opens, and a read that
 * fails while the stream is open asked again (`keptCurrent`, the one way a
 * store reads the service). The main window (`AppContent`) and the tray
 * panel each keep it while they live.
 *
 * The host does not change while a service runs, but a service that went
 * can come back as another build, so each opening reads it afresh. While the
 * service is gone the last host it gave stays, so the words for the machine
 * do not flip to "this computer" and back on every outage. A service older
 * than the `host` field answers without it, and the host stays as it was.
 *
 * @module services/hostEvents
 */

import { keptCurrent } from './bridgeEvents';
import { applyHost } from './hostRegistry';
import { getTransport } from './transport';
import type { VersionDto } from '../types/generated/VersionDto';

const kept = keptCurrent<Partial<Pick<VersionDto, 'host'>>>({
  read: () => getTransport().getVersion(),
  apply: (version) => {
    if (version.host) applyHost(version.host);
  },
});

/** Read the service's host now and each time the event stream opens. Returns the way to stop. */
export function keepHostCurrent(): () => void {
  return kept.keep();
}
