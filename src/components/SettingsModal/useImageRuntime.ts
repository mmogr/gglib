/**
 * State for the image runtime section of System settings: what
 * stable-diffusion.cpp `sd-server` is installed, the install with its
 * progress, and the removal.
 *
 * The install and the removal are the daemon's (`/api/config/system/*-sd`),
 * as `gglib config sd install|uninstall` runs them in-process: one install,
 * one record, whichever surface asked.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { getSdStatus, streamSdInstall, uninstallSd } from '../../services/transport/api/setup';
import type { ImageRuntimeStatus, LlamaProgressEvent } from '../../types/setup';
import { appLogger } from '../../services/platform';
import { formatError } from '../../utils/errors';

export interface ImageRuntimeState {
  status: ImageRuntimeStatus | null;
  statusError: string | null;
  loadingStatus: boolean;

  installing: boolean;
  /** The install's latest event, for `InstallProgress`. */
  progress: LlamaProgressEvent | null;
  installError: string | null;
  /** The release a finished install put there. */
  installedVersion: string | null;
  runInstall: () => void;

  uninstalling: boolean;
  uninstallResult: string | null;
  uninstallError: string | null;
  runUninstall: () => Promise<void>;
}

export function useImageRuntime(): ImageRuntimeState {
  const [status, setStatus] = useState<ImageRuntimeStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(true);

  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<LlamaProgressEvent | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const [installedVersion, setInstalledVersion] = useState<string | null>(null);
  const stopReading = useRef<(() => void) | null>(null);

  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallResult, setUninstallResult] = useState<string | null>(null);
  const [uninstallError, setUninstallError] = useState<string | null>(null);

  const reloadStatus = useCallback(async () => {
    setLoadingStatus(true);
    try {
      setStatus(await getSdStatus());
      setStatusError(null);
    } catch (err) {
      setStatusError(formatError(err));
    } finally {
      setLoadingStatus(false);
    }
  }, []);

  useEffect(() => {
    void reloadStatus();
  }, [reloadStatus]);

  // Unmounting stops the reading, not the install: the daemon finishes it.
  useEffect(() => () => stopReading.current?.(), []);

  const runInstall = useCallback(() => {
    // A stream ends once: `completed`, `failed`, a transport error, or a close
    // that said neither. The first of them is the result.
    let settled = false;
    const fail = (message: string) => {
      if (settled) return;
      settled = true;
      appLogger.error('component.settings', 'image runtime install failed', { message });
      setInstalling(false);
      setInstallError(message);
    };

    setInstalling(true);
    setInstallError(null);
    setInstalledVersion(null);
    setUninstallResult(null);
    setProgress(null);

    stopReading.current = streamSdInstall(
      (event) => {
        if (settled) return;
        setProgress(event);
        if (event.type === 'completed') {
          settled = true;
          setInstalling(false);
          setInstalledVersion(event.version);
          void reloadStatus();
        } else if (event.type === 'failed') {
          fail(event.message);
        }
      },
      fail,
      () => fail('The connection to the install ended before it reported a result.'),
    );
  }, [reloadStatus]);

  const runUninstall = useCallback(async () => {
    setUninstalling(true);
    setUninstallError(null);
    setUninstallResult(null);
    setInstalledVersion(null);
    try {
      const outcome = await uninstallSd();
      setUninstallResult(
        outcome.wasInstalled
          ? `Removed ${outcome.removedPaths.join(', ')}.`
          : 'Nothing to remove: the image runtime was not installed.',
      );
    } catch (err) {
      setUninstallError(formatError(err));
    } finally {
      setUninstalling(false);
      void reloadStatus();
    }
  }, [reloadStatus]);

  return {
    status,
    statusError,
    loadingStatus,
    installing,
    progress,
    installError,
    installedVersion,
    runInstall,
    uninstalling,
    uninstallResult,
    uninstallError,
    runUninstall,
  };
}
