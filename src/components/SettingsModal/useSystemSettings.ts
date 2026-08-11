/**
 * State for the System settings tab: what llama.cpp is installed, whether
 * upstream has moved, and the update/uninstall actions.
 *
 * Split by cost. Status is local and loads on mount; the update check runs
 * `git fetch` and only ever runs when asked. The update itself streams build
 * events, so the tab can show progress instead of an indefinite spinner.
 */

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import {
  checkLlamaUpdates,
  getLlamaStatus,
  streamLlamaUpdate,
  uninstallLlama,
} from '../../services/transport/api/setup';
import type { BuildEvent, LlamaStatus, LlamaUpdateCheck } from '../../types/setup';
import { appLogger } from '../../services/platform';

/** Human-readable names for the build phases, in the order they run. */
const PHASE_LABELS: Record<string, string> = {
  dependency_check: 'Checking dependencies',
  clone_or_update_repo: 'Updating repository',
  configure: 'Configuring',
  compile: 'Compiling',
  install_binaries: 'Installing binaries',
};

/**
 * Build state lives outside React.
 *
 * A llama.cpp rebuild takes minutes and keeps running on the daemon whatever
 * the UI does. If this lived in component state, switching tabs or closing
 * Settings would discard it and the user would come back to a panel claiming
 * nothing is happening while a compile is running — and could start a second
 * one. Module scope means a remount re-attaches to the build in progress.
 */
interface BuildState {
  updating: boolean;
  progress: string | null;
  error: string | null;
  result: string | null;
}

let buildState: BuildState = { updating: false, progress: null, error: null, result: null };
const buildListeners = new Set<() => void>();

function setBuildState(patch: Partial<BuildState>) {
  buildState = { ...buildState, ...patch };
  buildListeners.forEach((l) => l());
}

function subscribeBuild(listener: () => void) {
  buildListeners.add(listener);
  return () => {
    buildListeners.delete(listener);
  };
}

export interface SystemSettingsState {
  status: LlamaStatus | null;
  statusError: string | null;
  loadingStatus: boolean;
  reloadStatus: () => void;

  updateCheck: LlamaUpdateCheck | null;
  checkingUpdates: boolean;
  checkError: string | null;
  runUpdateCheck: () => void;

  updating: boolean;
  /** Current build phase, or the last log line — whatever is most recent. */
  updateProgress: string | null;
  updateError: string | null;
  updateResult: string | null;
  runUpdate: () => void;

  uninstalling: boolean;
  uninstallResult: string | null;
  runUninstall: () => Promise<void>;
}

export function useSystemSettings(): SystemSettingsState {
  const [status, setStatus] = useState<LlamaStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(true);

  const [updateCheck, setUpdateCheck] = useState<LlamaUpdateCheck | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [checkError, setCheckError] = useState<string | null>(null);

  const build = useSyncExternalStore(subscribeBuild, () => buildState);

  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallResult, setUninstallResult] = useState<string | null>(null);

  const reloadStatus = useCallback(() => {
    setLoadingStatus(true);
    setStatusError(null);
    void getLlamaStatus()
      .then(setStatus)
      .catch((err: unknown) => {
        setStatusError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => setLoadingStatus(false));
  }, []);

  useEffect(() => {
    reloadStatus();
    // Deliberately no teardown for the build stream: it is owned by the module
    // store, not by this component, so leaving the tab does not stop us
    // listening and coming back shows the build still in progress.
  }, [reloadStatus]);

  const runUpdateCheck = useCallback(() => {
    setCheckingUpdates(true);
    setCheckError(null);
    void checkLlamaUpdates()
      .then(setUpdateCheck)
      .catch((err: unknown) => {
        setCheckError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => setCheckingUpdates(false));
  }, []);

  const runUpdate = useCallback(() => {
    if (buildState.updating) return;
    setBuildState({ updating: true, error: null, result: null, progress: 'Starting…' });

    streamLlamaUpdate(
      (event: BuildEvent) => {
        switch (event.type) {
          case 'phase_started':
            setBuildState({ progress: PHASE_LABELS[event.phase] ?? event.phase });
            break;
          case 'log':
            setBuildState({ progress: event.message });
            break;
          case 'completed':
            setBuildState({
              updating: false,
              progress: null,
              result: `Rebuilt at ${event.version} (${event.acceleration})`,
            });
            setUpdateCheck(null);
            reloadStatus();
            break;
          case 'failed':
            setBuildState({ updating: false, progress: null, error: event.message });
            break;
          default:
            break;
        }
      },
      (message) => {
        appLogger.error('component', 'llama update stream failed', { message });
        setBuildState({ updating: false, progress: null, error: message });
      },
      // A stream that ends without `completed` or `failed` — daemon restart,
      // network drop — must not leave the panel spinning forever.
      () => {
        if (buildState.updating) {
          setBuildState({
            updating: false,
            progress: null,
            error: 'The connection to the build ended before it reported a result. It may still be running — reopen this tab to check.',
          });
        }
      },
    );
  }, [reloadStatus]);

  const runUninstall = useCallback(async () => {
    setUninstalling(true);
    setUninstallResult(null);
    try {
      const outcome = await uninstallLlama();
      setUninstallResult(
        outcome.wasInstalled
          ? `Removed ${outcome.removedPaths.length} path(s)`
          : 'Nothing was installed',
      );
      setUpdateCheck(null);
      reloadStatus();
    } catch (err) {
      setStatusError(err instanceof Error ? err.message : String(err));
    } finally {
      setUninstalling(false);
    }
  }, [reloadStatus]);

  return {
    status,
    statusError,
    loadingStatus,
    reloadStatus,
    updateCheck,
    checkingUpdates,
    checkError,
    runUpdateCheck,
    updating: build.updating,
    updateProgress: build.progress,
    updateError: build.error,
    updateResult: build.result,
    runUpdate,
    uninstalling,
    uninstallResult,
    runUninstall,
  };
}
