/**
 * The service-not-running screen, and the gate that shows it.
 *
 * `ServiceGate` holds the window (the shell) while the daemon answers, and
 * puts this screen in its place while `useDaemonReachable` says it does
 * not: whatever stopped it, About's Stop gglib…, the tray, a terminal, a
 * crash. It goes the moment the daemon answers again, back at the address
 * the window was at. Before the first answer the window is drawn, never
 * this screen: a healthy daemon is not called gone for a moment.
 *
 * The screen is one centred sentence beside the sidebar, dimmed and out of
 * reach. On the desktop, Start gglib starts the daemon the way the tray
 * menu's Start gglib does (`start_gglib`), and a refusal is said in the
 * app's words, the port held by another program among them. A browser tab
 * has nothing to start a daemon with, so it says it lost gglib and asks
 * again until it is back (the poll is the retry).
 *
 * @module app/ServiceDown
 */

import { useState, type FC, type ReactNode } from 'react';
import { Button } from '../components/ui/Button';
import { StatusMark } from '../components/status/StatusMark';
import { useDaemonReachable } from '../hooks/useDaemonReachable';
import { isDesktop, startGglib } from '../services/platform';
import { Sidebar } from '../sidebar/Sidebar';
import { formatError } from '../utils/errors';

/** Start gglib, and what came of it. */
const StartGglib: FC = () => {
  const [starting, setStarting] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);

  const start = async () => {
    setStarting(true);
    setRefusal(null);
    try {
      await startGglib();
      // Held until the poll sees it answer and this screen goes.
    } catch (err) {
      setRefusal(formatError(err));
      setStarting(false);
    }
  };

  return (
    <>
      <Button variant="primary" disabled={starting} onClick={() => void start()}>
        {starting ? 'Starting gglib…' : 'Start gglib'}
      </Button>
      {refusal && (
        <p role="alert" className="m-0 max-w-[480px] text-sm text-text-secondary">
          {refusal}
        </p>
      )}
    </>
  );
};

export const ServiceDown: FC = () => (
  <div className="flex h-screen overflow-hidden bg-background text-text">
    <aside
      inert
      aria-hidden
      className="hidden wide:flex w-sidebar-width shrink-0 flex-col border-r border-border bg-background-secondary opacity-40"
    >
      <Sidebar />
    </aside>
    <main className="flex-1 min-w-0 flex flex-col items-center justify-center gap-lg px-lg text-center">
      {isDesktop() ? (
        <>
          <p className="m-0 text-lg text-text">gglib isn't running, so nothing can be served.</p>
          <StartGglib />
        </>
      ) : (
        <p role="status" className="m-0">
          <StatusMark kind="changing" word="Lost gglib. Reconnecting…" className="gap-sm text-lg text-text" />
        </p>
      )}
    </main>
  </div>
);

/** The window while the daemon answers; the screen above while it does not. */
export const ServiceGate: FC<{ children: ReactNode }> = ({ children }) => {
  const reachable = useDaemonReachable();
  return reachable === false ? <ServiceDown /> : <>{children}</>;
};
