/**
 * Setup, as a gate above the router's outlet: while the daemon's settings
 * say setup has not been completed, the window is the setup place; once it
 * has, or once Open gglib writes it, the window is `children` (the shell).
 * Setup is not a route, and nothing opens it again.
 *
 * The gate reads `setupCompleted` once, from the setup status. A read that
 * fails opens the shell, as before: a daemon that cannot be reached is not
 * a reason to run setup, and the shell's places each say what they could
 * not read. Nothing is drawn while the read is out.
 *
 * @module app/SetupGate
 */

import { useCallback, useEffect, useState, type FC, type ReactNode } from 'react';
import SetupPlace from '../places/setup/SetupPlace';
import { getSetupStatus } from '../services/transport/api/setup';

export const SetupGate: FC<{ children: ReactNode }> = ({ children }) => {
  const [needed, setNeeded] = useState<boolean | null>(null);

  useEffect(() => {
    let live = true;
    getSetupStatus()
      .then((status) => live && setNeeded(!status.setupCompleted))
      .catch(() => live && setNeeded(false));
    return () => {
      live = false;
    };
  }, []);

  const done = useCallback(() => setNeeded(false), []);

  if (needed === null) return <div className="fixed inset-0 bg-background" />;
  if (needed) return <SetupPlace onDone={done} />;
  return <>{children}</>;
};
