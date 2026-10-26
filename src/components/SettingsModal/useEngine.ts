/**
 * State for one engine's card in System settings: what is installed, where
 * it stands against its pinned release, and the install, update and removal.
 *
 * Status is local and loads on mount. The install is the daemon's install
 * of the pinned download, the one `gglib config llama|sd install` takes, and
 * is the update too. It streams its progress, so the card can show it
 * instead of a spinner.
 */

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { getEngineStatus, streamEngineInstall, uninstallEngine } from '../../services/transport/api/setup';
import { ENGINE_PRODUCT } from '../../types/setup';
import type { EngineStatus, LlamaProgressEvent, RuntimeKind } from '../../types/setup';
import { appLogger } from '../../services/platform';
import { formatError } from '../../utils/errors';

/**
 * Install state lives outside React, one per engine.
 *
 * An install keeps running on the daemon whatever the UI does. If this lived
 * in component state, switching tabs or closing Settings would discard it,
 * and the user would come back to a card claiming nothing is happening while
 * a download runs, and could start a second one. Module scope means a
 * remount re-attaches to the install in progress. Each engine has its own,
 * so one engine's install never shows on the other's card.
 */
interface InstallState {
  installing: boolean;
  /** The install's latest event, for `InstallProgress`. */
  progress: LlamaProgressEvent | null;
  error: string | null;
  /** The release a finished install put there. */
  installedVersion: string | null;
}

const IDLE: InstallState = { installing: false, progress: null, error: null, installedVersion: null };

const installs: Record<RuntimeKind, InstallState> = { llama: IDLE, stable_diffusion: IDLE };
const installListeners = new Set<() => void>();

function setInstallState(kind: RuntimeKind, patch: Partial<InstallState>) {
  installs[kind] = { ...installs[kind], ...patch };
  installListeners.forEach((l) => l());
}

function subscribeInstall(listener: () => void) {
  installListeners.add(listener);
  return () => {
    installListeners.delete(listener);
  };
}

export interface EngineState {
  status: EngineStatus | null;
  statusError: string | null;
  loadingStatus: boolean;

  installing: boolean;
  progress: LlamaProgressEvent | null;
  installError: string | null;
  installedVersion: string | null;
  /** Install the pinned release, over what is there if anything is. */
  runInstall: () => void;

  uninstalling: boolean;
  uninstallResult: string | null;
  uninstallError: string | null;
  runUninstall: () => Promise<void>;
}

export function useEngine(kind: RuntimeKind): EngineState {
  const product = ENGINE_PRODUCT[kind];
  const [status, setStatus] = useState<EngineStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(true);

  const install = useSyncExternalStore(subscribeInstall, () => installs[kind]);

  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallResult, setUninstallResult] = useState<string | null>(null);
  const [uninstallError, setUninstallError] = useState<string | null>(null);

  const reloadStatus = useCallback(() => {
    setLoadingStatus(true);
    void getEngineStatus(kind)
      .then((next) => {
        setStatus(next);
        setStatusError(null);
      })
      .catch((err: unknown) => {
        setStatusError(formatError(err));
      })
      .finally(() => setLoadingStatus(false));
  }, [kind]);

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
    if (installs[kind].installing) return;
    // A stream ends once: `completed`, `failed`, a transport error, or a
    // close that said neither. The first of them is the result.
    let settled = false;
    const fail = (message: string) => {
      if (settled) return;
      settled = true;
      appLogger.error('component.settings', `${product} install failed`, { message });
      setInstallState(kind, { installing: false, error: message });
    };

    setInstallState(kind, { installing: true, progress: null, error: null, installedVersion: null });
    setUninstallResult(null);
    setUninstallError(null);

    streamEngineInstall(
      kind,
      (event) => {
        if (settled) return;
        setInstallState(kind, { progress: event });
        if (event.type === 'completed') {
          settled = true;
          setInstallState(kind, { installing: false, installedVersion: event.version });
        } else if (event.type === 'failed') {
          fail(event.message);
        }
      },
      fail,
      // A stream that ends without `completed` or `failed` — daemon restart,
      // network drop — must not leave the card spinning forever.
      () =>
        fail(
          'The connection to the install ended before it reported a result. It may still be running — reopen this tab to check.',
        ),
    );
  }, [kind, product]);

  const runUninstall = useCallback(async () => {
    setUninstalling(true);
    setUninstallResult(null);
    setUninstallError(null);
    setInstallState(kind, { installedVersion: null, error: null });
    try {
      const outcome = await uninstallEngine(kind);
      setUninstallResult(
        outcome.wasInstalled
          ? `Removed ${outcome.removedPaths.join(', ')}.`
          : `Nothing to remove: ${product} was not installed.`,
      );
    } catch (err) {
      setUninstallError(formatError(err));
    } finally {
      setUninstalling(false);
      reloadStatus();
    }
  }, [kind, product, reloadStatus]);

  return {
    status,
    statusError,
    loadingStatus,
    installing: install.installing,
    progress: install.progress,
    installError: install.error,
    installedVersion: install.installedVersion,
    runInstall,
    uninstalling,
    uninstallResult,
    uninstallError,
    runUninstall,
  };
}
