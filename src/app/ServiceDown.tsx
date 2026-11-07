/**
 * The service-not-running screen, and the gate that shows it.
 *
 * `ServiceGate` holds the window (the shell) while the daemon answers its
 * health probe, and puts this screen in its place while it does not
 * (`useDaemonRecovery`): whatever stopped it, About's Stop gglib…, the tray,
 * a terminal, a crash. While it is gone the registries let go of what it
 * said; it goes the moment the daemon answers again, back at the address the
 * window was at, with the proxy, the servers and the remote read again and
 * the event stream woken, as the tray does. Before the first answer the
 * window is drawn, never this screen: a healthy daemon is not called gone
 * for a moment.
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

import { useEffect, useState, type FC, type ReactNode } from 'react';
import { Button } from '../components/ui/Button';
import { StatusMark } from '../components/status/StatusMark';
import { useDaemonRecovery } from '../hooks/useDaemonRecovery';
import { isDesktop, startGglib } from '../services/platform';
import { getTransport } from '../services/transport';
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

/** The dimmed sidebar beside one centred message. */
const Frame: FC<{ children: ReactNode }> = ({ children }) => (
  <div className="flex h-screen overflow-hidden bg-background text-text">
    <aside
      inert
      aria-hidden
      className="hidden wide:flex w-sidebar-width shrink-0 flex-col border-r border-border bg-background-secondary opacity-40"
    >
      <Sidebar />
    </aside>
    <main className="flex-1 min-w-0 flex flex-col items-center justify-center gap-lg px-lg text-center">{children}</main>
  </div>
);

export const ServiceDown: FC = () => (
  <Frame>
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
  </Frame>
);

/**
 * The window while the daemon answers; the screen above while it does not.
 * A browser tab whose token the daemon refuses (it started again since the
 * tab took its link) is told so in the daemon's own sentence instead: the
 * remedy is a new link, and nothing here can mend it. That is asked once
 * each time the daemon is heard, never polled.
 */
export const ServiceGate: FC<{ children: ReactNode }> = ({ children }) => {
  const reachable = useDaemonRecovery();
  const [refused, setRefused] = useState<string | null>(null);

  useEffect(() => {
    if (reachable !== true || isDesktop()) return;
    let current = true;
    void getTransport()
      .tokenRefusal()
      .then((sentence) => {
        if (current) setRefused(sentence);
      });
    return () => {
      current = false;
    };
  }, [reachable]);

  if (reachable === false) return <ServiceDown />;
  if (refused !== null) {
    return (
      <Frame>
        <p role="alert" className="m-0 max-w-[560px] text-lg text-text">
          {refused}
        </p>
      </Frame>
    );
  }
  return <>{children}</>;
};
