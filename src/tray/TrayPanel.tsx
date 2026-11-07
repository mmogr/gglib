/**
 * TrayPanel: the popover behind the system tray icon, 360×480.
 *
 * "gglib", then three blocks: the Endpoint (its state, the address with ⧉,
 * the pin, the key rule), Loaded (each model in memory with its word and
 * ×) and Devices (the joined machine's reach, sharing and how many are
 * paired); the footer turns the endpoint on or off (the panel's one ink
 * button) and opens gglib. The blocks scroll inside; the footer stays.
 *
 * No daemon is not a stopped endpoint (TM-8): while the daemon does not
 * answer, the panel says so in one sentence and Start gglib is drawn
 * disabled, since only the tray menu can start the service; the poll in
 * `useDaemonReachable` notices it coming back, and Turn on becomes live.
 * Before the first answer, and until the registries have heard the
 * daemon, the panel claims neither.
 *
 * The panel is its own window with none of the app's providers
 * (`tray-main.tsx`), so it starts its own bridges for the registries it
 * reads, the same `proxyRegistry`, `serverRegistry`, `remoteRegistry` and
 * dashboard stream as the main window. They stay up for the panel's life,
 * so there is one event stream however often the daemon comes and goes
 * (TX-1): when it goes, what the registries held is let go; when it comes
 * back, each is read again, and the stream connects at once rather than
 * sleeping out a reconnect wait that grew while the daemon was gone. A list
 * read that failed, or that an event overtook, is tried again each poll
 * until one lands, since only a landed read tells "nothing running" from
 * "not known". Everything it does is HTTP except Open gglib,
 * its one window action (`show_main_window`, TM-9).
 *
 * @module tray/TrayPanel
 */

import { useEffect, useState, type FC } from 'react';
import { Button } from '../components/ui/Button';
import { useDaemonRecovery } from '../hooks/useDaemonRecovery';
import { useServers } from '../hooks/useServers';
import { useEndpointActions } from '../places/endpoint/useEndpointActions';
import { useEndpointDashboard } from '../places/library/useResidency';
import { appLogger, isDesktop, showMainWindow } from '../services/platform';
import { cleanupProxyEvents, initProxyEvents } from '../services/proxyEvents';
import { useEndpointFacts, useProxyState } from '../services/proxyRegistry';
import { cleanupRemoteEvents, initRemoteEvents } from '../services/remoteEvents';
import { useRemoteState } from '../services/remoteRegistry';
import { cleanupServerEvents, initServerEvents, refreshServers, useServerListHeard } from '../services/serverEvents';
import { Devices } from './Devices';
import { Endpoint } from './Endpoint';
import { Loaded, trayLoadedRows } from './Loaded';

/** How often an unheard server list is read again: the probe's own poll. */
const LIST_RETRY_MS = 2000;

export const TrayPanel: FC = () => {
  // The daemon going lets go of what the registries held; its coming back
  // reads them again and wakes the stream (`useDaemonRecovery`).
  const reachable = useDaemonRecovery();
  const facts = useEndpointFacts();
  const dash = useEndpointDashboard();
  const actions = useEndpointActions();
  const { servers } = useServers();
  const { status: remote } = useRemoteState();
  const listHeard = useServerListHeard();

  // Up for the panel's life: taking them down with the daemon and up again
  // could leave the old stream's reconnect loop running beside the new one.
  useEffect(() => {
    initProxyEvents();
    initServerEvents();
    initRemoteEvents();
    return () => {
      cleanupProxyEvents();
      cleanupServerEvents();
      cleanupRemoteEvents();
    };
  }, []);

  const live = reachable === true;

  // Until a list read lands, ask again each poll: a read that failed or that
  // a server event overtook is dropped, and would leave Loaded unknown.
  useEffect(() => {
    if (!live || listHeard) return;
    const timer = setInterval(refreshServers, LIST_RETRY_MS);
    return () => clearInterval(timer);
  }, [live, listHeard]);

  // The probe answering is not the registries having heard: until the
  // bridge's own read lands they hold nothing, which would draw a running
  // endpoint as Off. Held once heard, since every proxy event drops the
  // status until its re-read, and let go when the daemon goes.
  const { status } = useProxyState();
  const [heard, setHeard] = useState(false);
  if (!live && heard) setHeard(false);
  if (live && !heard && status !== null) setHeard(true);
  const ready = live && heard;

  const up = facts.state !== 'off';
  const openGglib = () =>
    void showMainWindow().catch((err: unknown) => appLogger.error('component.tray', 'Could not show the main window', { error: err }));

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background text-text tabular-nums">
      <span className="shrink-0 px-md pt-md text-sm font-semibold text-text">gglib</span>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {reachable === false && (
          <p className="m-0 px-md py-md text-sm text-text-secondary">
            gglib isn&apos;t running, so nothing can be served. Start it from the tray menu.
          </p>
        )}
        {ready && (
          <>
            <Endpoint facts={facts} busy={actions.busy} error={actions.error} apiKey={dash.apiKey} />
            <Loaded
              rows={trayLoadedRows(dash.snapshot, listHeard ? servers : [])}
              known={dash.snapshot !== null || (!up && listHeard)}
              off={!up}
            />
            <Devices status={remote} />
          </>
        )}
      </div>

      <footer className="flex shrink-0 items-center justify-between gap-sm px-md py-md">
        {reachable === false ? (
          // Disabled, not hidden (TM-8): only the tray menu can start the service.
          <Button disabled>Start gglib</Button>
        ) : !ready ? (
          // Neither Turn on nor Turn off until the panel knows which is true.
          <span />
        ) : up ? (
          <Button isLoading={actions.busy === 'off'} onClick={() => void actions.turnOff()}>
            Turn off
          </Button>
        ) : (
          <Button isLoading={actions.busy === 'on'} onClick={() => void actions.turnOn()}>
            Turn on
          </Button>
        )}
        {isDesktop() && (
          <Button variant="ghost" onClick={openGglib}>
            Open gglib
          </Button>
        )}
      </footer>
    </div>
  );
};
