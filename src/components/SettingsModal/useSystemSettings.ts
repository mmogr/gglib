/**
 * State for the System settings tab: what llama.cpp is installed, where it
 * stands against its pinned release, and the update and uninstall actions.
 *
 * Status is local and loads on mount. The update is the daemon's install of
 * the pinned release, the download `gglib config llama update` takes, and
 * streams its progress, so the tab can show it instead of a spinner.
 */

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { getLlamaStatus, streamLlamaInstall, uninstallLlama } from '../../services/transport/api/setup';
import type { LlamaProgressEvent, LlamaStatus } from '../../types/setup';
import { appLogger } from '../../services/platform';
import { formatError } from '../../utils/errors';

/**
 * Install state lives outside React.
 *
 * An install keeps running on the daemon whatever the UI does. If this lived
 * in component state, switching tabs or closing Settings would discard it,
 * and the user would come back to a panel claiming nothing is happening
 * while a download runs, and could start a second one. Module scope means a
 * remount re-attaches to the install in progress.
 */
interface InstallState {
  installing: boolean;
  /** The install's latest event, for `InstallProgress`. */
  progress: LlamaProgressEvent | null;
  error: string | null;
  /** The release a finished install put there. */
  installedVersion: string | null;
}

let installState: InstallState = { installing: false, progress: null, error: null, installedVersion: null };
const installListeners = new Set<() => void>();

function setInstallState(patch: Partial<InstallState>) {
  installState = { ...installState, ...patch };
  installListeners.forEach((l) => l());
}

function subscribeInstall(listener: () => void) {
  installListeners.add(listener);
  return () => {
    installListeners.delete(listener);
  };
}

export interface SystemSettingsState {
  status: LlamaStatus | null;
  statusError: string | null;
  loadingStatus: boolean;
  reloadStatus: () => void;

  installing: boolean;
  progress: LlamaProgressEvent | null;
  installError: string | null;
  installedVersion: string | null;
  /** Install the pinned release over what is there. */
  runInstall: () => void;

  uninstalling: boolean;
  uninstallResult: string | null;
  runUninstall: () => Promise<void>;
}

export function useSystemSettings(): SystemSettingsState {
  const [status, setStatus] = useState<LlamaStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(true);

  const install = useSyncExternalStore(subscribeInstall, () => installState);

  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallResult, setUninstallResult] = useState<string | null>(null);

  const reloadStatus = useCallback(() => {
    setLoadingStatus(true);
    setStatusError(null);
    void getLlamaStatus()
      .then(setStatus)
      .catch((err: unknown) => {
        setStatusError(formatError(err));
      })
      .finally(() => setLoadingStatus(false));
  }, []);

  useEffect(() => {
    reloadStatus();
    // Deliberately no teardown for the install stream: it is owned by the
    // module store, not by this component, so leaving the tab does not stop
    // us listening and coming back shows the install still in progress.
  }, [reloadStatus]);

  // An install that finishes is read back by whichever instance is mounted
  // then: the one that started it may have gone with the tab it was on.
  useEffect(() => {
    if (install.installedVersion) reloadStatus();
  }, [install.installedVersion, reloadStatus]);

  const runInstall = useCallback(() => {
    if (installState.installing) return;
    // A stream ends once: `completed`, `failed`, a transport error, or a
    // close that said neither. The first of them is the result.
    let settled = false;
    const fail = (message: string) => {
      if (settled) return;
      settled = true;
      appLogger.error('component.settings', 'llama.cpp install failed', { message });
      setInstallState({ installing: false, error: message });
    };

    setInstallState({ installing: true, progress: null, error: null, installedVersion: null });
    setUninstallResult(null);

    streamLlamaInstall(
      (event) => {
        if (settled) return;
        setInstallState({ progress: event });
        if (event.type === 'completed') {
          settled = true;
          setInstallState({ installing: false, installedVersion: event.version });
        } else if (event.type === 'failed') {
          fail(event.message);
        }
      },
      fail,
      // A stream that ends without `completed` or `failed` — daemon restart,
      // network drop — must not leave the panel spinning forever.
      () =>
        fail(
          'The connection to the install ended before it reported a result. It may still be running — reopen this tab to check.',
        ),
    );
  }, []);

  const runUninstall = useCallback(async () => {
    setUninstalling(true);
    setUninstallResult(null);
    setInstallState({ installedVersion: null, error: null });
    try {
      const outcome = await uninstallLlama();
      setUninstallResult(
        outcome.wasInstalled
          ? `Removed ${outcome.removedPaths.length} path(s)`
          : 'Nothing was installed',
      );
      reloadStatus();
    } catch (err) {
      setStatusError(formatError(err));
    } finally {
      setUninstalling(false);
    }
  }, [reloadStatus]);

  return {
    status,
    statusError,
    loadingStatus,
    reloadStatus,
    installing: install.installing,
    progress: install.progress,
    installError: install.error,
    installedVersion: install.installedVersion,
    runInstall,
    uninstalling,
    uninstallResult,
    runUninstall,
  };
}
