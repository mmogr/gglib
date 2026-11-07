/**
 * Setup, as a gate above the router's outlet: while the daemon's settings
 * say setup has not been completed, the window is the setup place; once it
 * has, or once Open gglib writes it, the window is `children` (the shell).
 * Setup is not a route, and nothing opens it again.
 *
 * The gate reads the setup status once, and hands it to the place, which
 * does not read it again (each read runs the daemon's Python check). A read that
 * fails opens the shell, as before: a daemon that cannot be reached is not
 * a reason to run setup, and the shell's places each say what they could
 * not read. Nothing is drawn while the read is out.
 *
 * @module app/SetupGate
 */

import { useCallback, useEffect, useState, type FC, type ReactNode } from 'react';
import SetupPlace from '../places/setup/SetupPlace';
import { getSetupStatus } from '../services/transport/api/setup';
import type { SetupStatus } from '../types/setup';

export const SetupGate: FC<{ children: ReactNode }> = ({ children }) => {
  // The status while setup is needed; `false` once it is not; `null` while asked.
  const [needed, setNeeded] = useState<SetupStatus | false | null>(null);

  useEffect(() => {
    let live = true;
    getSetupStatus()
      .then((status) => live && setNeeded(status.setupCompleted ? false : status))
      .catch(() => live && setNeeded(false));
    return () => {
      live = false;
    };
  }, []);

  const done = useCallback(() => setNeeded(false), []);

  if (needed === null) return <div className="fixed inset-0 bg-background" />;
  if (needed) return <SetupPlace status={needed} onDone={done} />;
  return <>{children}</>;
};
