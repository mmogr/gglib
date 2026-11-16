/**
 * State for one engine's card in System settings: what is installed, where
 * it stands against its pinned release, and the install, update, removal
 * and repair.
 *
 * Status is local and loads on mount. The install is the daemon's install
 * of the pinned download, the one `gglib config llama|sd install` takes, and
 * is the update too. It streams its progress, so the card can show it
 * instead of a spinner.
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { getEngineStatus, repairEngine, streamEngineInstall, uninstallEngine } from '../services/transport/api/setup';
import { ENGINE_PRODUCT } from '../types/setup';
import type { EngineStatus, LlamaProgressEvent, RepairReport, RuntimeKind } from '../types/setup';
import { allowServiceForeground, appLogger } from '../services/platform';
import { formatError } from '../utils/errors';

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

/**
 * A repair lives outside React too, one per engine, for the same reason:
 * the installer Windows runs for it outlives the tab. `round` counts the
 * repairs that have ended, on either engine: the Visual C++ runtime one
 * installs is both engines', so every engine's status is read again after
 * one, with the read the card already has.
 */
interface RepairState {
  repairing: boolean;
  /** What the last repair did, in the daemon's words. */
  said: string | null;
  error: string | null;
}

const NO_REPAIR: RepairState = { repairing: false, said: null, error: null };

const repairs: Record<RuntimeKind, RepairState> = { llama: NO_REPAIR, stable_diffusion: NO_REPAIR };
let repairRound = 0;

function setRepairState(kind: RuntimeKind, patch: Partial<RepairState>) {
  repairs[kind] = { ...repairs[kind], ...patch };
  installListeners.forEach((l) => l());
}

function endRepair(kind: RuntimeKind, patch: Partial<RepairState>) {
  repairRound += 1;
  setRepairState(kind, { ...patch, repairing: false });
}

/**
 * Post the daemon's repair of `kind`, the one place a page asks for one.
 * The service runs the installer, which asks Windows for permission, and
 * Windows leaves a prompt asked for from the background on the taskbar; so
 * first the app, whose window was just clicked, lets the service come to
 * the front (`allowServiceForeground`). That never fails the repair and
 * never holds it for long: the repair is posted once the grant answers or
 * after `GRANT_WAIT_MS`, whichever is first. Where it is not granted, in a
 * browser among them, the repair is posted as before and the daemon's
 * words say where to look. The prompt itself comes only after the service
 * has downloaded and checked the installer, and a click or key press in
 * that time ends the right.
 */
export async function postRepair(kind: RuntimeKind): Promise<RepairReport> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      allowServiceForeground(),
      new Promise<void>((resolve) => {
        timer = setTimeout(resolve, GRANT_WAIT_MS);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
  return repairEngine(kind);
}

/**
 * The longest a repair waits for the app's grant before it is posted
 * anyway. The app's command waits for nothing itself; this bounds the
 * bridge to it, so a grant that never answers cannot hold a repair.
 */
export const GRANT_WAIT_MS = 300;

/**
 * The release `kind`'s last finished install put there (`null` before one),
 * for a reader of the engine's status elsewhere to read it again when an
 * install anywhere completes.
 */
export function useInstalledVersion(kind: RuntimeKind): string | null {
  return useSyncExternalStore(subscribeInstall, () => installs[kind].installedVersion);
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

  repairing: boolean;
  /** What the last repair did, in the daemon's words. */
  repairResult: string | null;
  repairError: string | null;
  /**
   * Make the repair the status offers (`status.fix`): the install again for
   * a reinstall, the daemon's repair for the Visual C++ runtime. Every
   * engine's status is read again when it ends.
   */
  runRepair: () => void;
}

export function useEngine(kind: RuntimeKind): EngineState {
  const product = ENGINE_PRODUCT[kind];
  const [status, setStatus] = useState<EngineStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(true);

  const install = useSyncExternalStore(subscribeInstall, () => installs[kind]);
  const repair = useSyncExternalStore(subscribeInstall, () => repairs[kind]);
  const round = useSyncExternalStore(subscribeInstall, () => repairRound);

  const [uninstalling, setUninstalling] = useState(false);
  const [uninstallResult, setUninstallResult] = useState<string | null>(null);
  const [uninstallError, setUninstallError] = useState<string | null>(null);

  // The newest read wins: an older one that answers after it is dropped,
  // so two reads in flight (a repair's and an install's) cannot land out
  // of order and leave the row showing the state from before.
  const reads = useRef(0);
  const reloadStatus = useCallback(() => {
    const mine = ++reads.current;
    setLoadingStatus(true);
    void getEngineStatus(kind)
      .then((next) => {
        if (mine !== reads.current) return;
        setStatus(next);
        setStatusError(null);
      })
      .catch((err: unknown) => {
        if (mine === reads.current) setStatusError(formatError(err));
      })
      .finally(() => {
        if (mine === reads.current) setLoadingStatus(false);
      });
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

  // So is a repair, on every engine's card.
  useEffect(() => {
    if (round > 0) reloadStatus();
  }, [round, reloadStatus]);

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
          'The connection to the install ended before it reported a result. It may still be running: open Settings › Engines again to check.',
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

  const fix = status?.fix?.fix ?? null;
  // A fix with no action is a person's to make (Windows' repair of a
  // damaged runtime): nothing is sent for it.
  const actionable = status?.fix?.action != null;
  const runRepair = useCallback(() => {
    if (!actionable) return;
    if (fix === 'reinstall') {
      runInstall();
      return;
    }
    if (fix === null || repairs[kind].repairing) return;
    setRepairState(kind, { repairing: true, said: null, error: null });
    postRepair(kind).then(
      (report) => endRepair(kind, { said: report.says }),
      (err: unknown) => endRepair(kind, { error: formatError(err) }),
    );
  }, [actionable, fix, kind, runInstall]);

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
    repairing: repair.repairing,
    repairResult: repair.said,
    repairError: repair.error,
    runRepair,
  };
}
