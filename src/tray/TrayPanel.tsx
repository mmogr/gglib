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
 * answer, the panel says so in one sentence. In the desktop app Start gglib
 * starts it the way the tray menu does (`start_gglib`), a refusal said under
 * the sentence in the app's words; elsewhere it is drawn disabled. The poll in
 * `useDaemonReachable` notices it coming back, and Turn on becomes live.
 * Before the first answer, and until the registries have heard the
 * daemon, the panel claims neither; in the desktop app, while the app's
 * lookup of the daemon's address is still out (its launch of the daemon is
 * in flight), it says gglib is starting (`useAddressLookup`).
 *
 * The panel is its own window with none of the app's providers
 * (`tray-main.tsx`), so it starts its own bridges for the registries it
 * reads, the same `proxyRegistry`, `serverRegistry`, `remoteRegistry` and
 * dashboard stream as the main window. They stay up for the panel's life,
 * so there is one event stream however often the daemon comes and goes
 * (TX-1): when it goes, what the registries held is let go; when it comes
 * back, the stream connects at once rather than sleeping out a reconnect
 * wait that grew while the daemon was gone, and its opening reads each
 * registry again (`keptCurrent`). A list read that failed, or that an
 * event overtook, is followed by the next opening, the next server event
 * (`serverEvents`) or, while the stream is open, `keptCurrent`'s own
 * retry, since only a landed read tells "nothing running" from
 * "not known". Everything it does is HTTP except Open gglib,
 * its one window action (`show_main_window`, TM-9).
 *
 * @module tray/TrayPanel
 */

import { useEffect, useState, type FC } from 'react';
import { Button } from '../components/ui/Button';
import { STARTING_WORDS, useAddressLookup } from '../hooks/useAddressLookup';
import { useDaemonRecovery } from '../hooks/useDaemonRecovery';
import { useServers } from '../hooks/useServers';
import { useEndpointActions } from '../places/endpoint/useEndpointActions';
import { useEndpointDashboard } from '../places/library/useResidency';
import { appLogger, isDesktop, showMainWindow, startGglib } from '../services/platform';
import { formatError } from '../utils/errors';
import { cleanupProxyEvents, initProxyEvents } from '../services/proxyEvents';
import { useEndpointFacts, useProxyState } from '../services/proxyRegistry';
import { cleanupRemoteEvents, initRemoteEvents } from '../services/remoteEvents';
import { useRemoteState } from '../services/remoteRegistry';
import { cleanupServerEvents, initServerEvents, useServerListHeard } from '../services/serverEvents';
import { Devices } from './Devices';
import { Endpoint } from './Endpoint';
import { Loaded, trayLoadedRows } from './Loaded';

export const TrayPanel: FC = () => {
  // The daemon going lets go of what the registries held; its coming back
  // wakes the stream, whose opening reads them again (`useDaemonRecovery`).
  const reachable = useDaemonRecovery();
  const lookup = useAddressLookup();
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
  // Start gglib: held until the poll hears the daemon, a refusal said.
  const [starting, setStarting] = useState(false);
  const [startRefusal, setStartRefusal] = useState<string | null>(null);
  if (reachable !== false && (starting || startRefusal !== null)) {
    setStarting(false);
    setStartRefusal(null);
  }
  const startService = async () => {
    setStarting(true);
    setStartRefusal(null);
    try {
      await startGglib();
    } catch (err) {
      setStartRefusal(formatError(err));
      setStarting(false);
    }
  };
  const openGglib = () =>
    void showMainWindow().catch((err: unknown) => appLogger.error('component.tray', 'Could not show the main window', { error: err }));

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-background text-text tabular-nums">
      <span className="shrink-0 px-md pt-md text-sm font-semibold text-text">gglib</span>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {reachable === null && isDesktop() && lookup.state !== 'known' && (
          <p role="status" className="m-0 px-md py-md text-sm text-text-secondary">
            {STARTING_WORDS}
          </p>
        )}
        {reachable === false && (
          <div className="flex flex-col gap-sm px-md py-md">
            <p className="m-0 text-sm text-text-secondary">
              {isDesktop()
                ? "gglib isn't running, so nothing can be served."
                : "gglib isn't running, so nothing can be served. Start it from the tray menu."}
            </p>
            {startRefusal !== null && (
              <p role="alert" className="m-0 text-sm text-text-secondary">
                {startRefusal}
              </p>
            )}
          </div>
        )}
        {ready && (
          <>
            <Endpoint facts={facts} busy={actions.busy} error={actions.error} apiKey={dash.apiKey} served={dash.snapshot?.total_requests ?? null} />
            <Loaded
              rows={trayLoadedRows(dash.snapshot, servers, listHeard)}
              known={dash.snapshot !== null || (!up && listHeard)}
              off={!up}
            />
            <Devices status={remote} />
          </>
        )}
      </div>

      <footer className="flex shrink-0 items-center justify-between gap-sm px-md py-md">
        {reachable === false ? (
          isDesktop() ? (
            <Button variant="primary" disabled={starting} onClick={() => void startService()}>
              {starting ? 'Starting gglib…' : 'Start gglib'}
            </Button>
          ) : (
            // Disabled, not hidden (TM-8): nothing here can start the service.
            <Button disabled>Start gglib</Button>
          )
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
