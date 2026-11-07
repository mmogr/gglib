/**
 * Setup's step 2, as state the place holds across its steps: both engines'
 * status and installs, and the download accelerator.
 *
 * Started once, on the step's first arrival (`start`): the engines are read,
 * then llama.cpp installs if it is missing and its install is a download,
 * then stable-diffusion.cpp the same way, then each is read again. The
 * installs are the daemon's stream in both builds (SU-1), the one the
 * Settings engine card and the Library banner read, and a stream that
 * closes without a result ends with a reason. Where the install is a source
 * build, llama.cpp is read again every few seconds until a terminal has
 * built it (`recheck` reads it at once). The accelerator is set up at the
 * same time where Python 3 is there; downloads work without it, so its
 * failure, like stable-diffusion.cpp's, is only a warning (SU-2).
 *
 * Held by the place, not by the step, so going Back and forward again
 * starts nothing twice.
 *
 * @module places/setup/useEngines
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { enableFastDownloads, getEngineStatus, streamEngineInstall } from '../../services/transport/api/setup';
import type { EngineStatus, LlamaProgressEvent, RuntimeKind } from '../../types/setup';
import { formatError } from '../../utils/errors';

/** How often a source build is looked for while setup waits on a terminal. */
export const BUILD_POLL_MS = 5000;

/** Why an install ended with no result: its stream closed having said neither. */
export const CLOSED_WITHOUT_RESULT = 'The connection to the install ended before it reported a result.';

export interface EngineView {
  status: EngineStatus | null;
  /** The install's last event while it runs; `null` when none runs. */
  installing: LlamaProgressEvent | 'starting' | null;
  /** Why the last install did not finish, in the daemon's words or the stream's end. */
  error: string | null;
}

export type FastState = 'ready' | 'no-python' | 'setting-up' | { failed: string };

/**
 * Whether `status`'s engine is missing and the daemon's install is a
 * download; one the command line would build is left to a terminal.
 */
export const installable = (status: EngineStatus | null): boolean =>
  status !== null && !status.installed && status.pin.updateDownloads;

/** Whether `status`'s engine is missing and only a terminal can build it. */
export const needsBuild = (status: EngineStatus | null): boolean =>
  status !== null && !status.installed && !status.pin.updateDownloads;

/** The command that builds it: again where it was built before, else from scratch. */
export const buildCommand = (status: EngineStatus): string =>
  status.download !== null ? status.updateCommand : status.installCommand;

const IDLE: EngineView = { status: null, installing: null, error: null };

export interface SetupEngines {
  llama: EngineView;
  sd: EngineView;
  fast: FastState;
  /** Why the engines could not be read. */
  readError: string | null;
  /** Begin step 2: read, then install what is missing. Once only. */
  start: () => void;
  retry: (kind: RuntimeKind) => void;
  retryFast: () => void;
  /** Read the engines again now. */
  recheck: () => void;
}

/** `fastReady` and `python` are the setup status's: whether it is set up, and whether it can be. */
export function useEngines(fastReady: boolean, python: boolean): SetupEngines {
  const [views, setViews] = useState<Record<RuntimeKind, EngineView>>({ llama: IDLE, stable_diffusion: IDLE });
  const [readError, setReadError] = useState<string | null>(null);
  const [fast, setFast] = useState<FastState>(fastReady ? 'ready' : python ? 'setting-up' : 'no-python');
  const [started, setStarted] = useState(false);
  // A ref as well as the state: StrictMode runs the arrival's effect twice before either sees the state.
  const once = useRef(false);
  const live = useRef(true);
  useEffect(() => () => void (live.current = false), []);

  const patch = useCallback((kind: RuntimeKind, change: Partial<EngineView>) => {
    if (live.current) setViews((prev) => ({ ...prev, [kind]: { ...prev[kind], ...change } }));
  }, []);

  /** Read `kinds`; answers the statuses read, or null when a read failed. */
  const read = useCallback(
    async (kinds: RuntimeKind[]): Promise<Partial<Record<RuntimeKind, EngineStatus>> | null> => {
      try {
        const found = await Promise.all(kinds.map((kind) => getEngineStatus(kind)));
        const byKind = Object.fromEntries(kinds.map((kind, i) => [kind, found[i]]));
        for (const kind of kinds) patch(kind, { status: byKind[kind] });
        setReadError(null);
        return byKind;
      } catch (err) {
        if (live.current) setReadError(formatError(err));
        return null;
      }
    },
    [patch],
  );

  /** Install `kind` and read it again; settles once, with the stream's first ending. */
  const install = useCallback(
    (kind: RuntimeKind) =>
      new Promise<void>((resolve) => {
        let settled = false;
        const settle = (error: string | null) => {
          if (settled) return;
          settled = true;
          patch(kind, { installing: null, error });
          void read([kind]).then(() => resolve());
        };
        patch(kind, { installing: 'starting', error: null });
        streamEngineInstall(
          kind,
          (event) => {
            if (settled) return;
            if (event.type === 'completed') settle(null);
            else if (event.type === 'failed') settle(event.message);
            else patch(kind, { installing: event });
          },
          settle,
          () => settle(CLOSED_WITHOUT_RESULT),
        );
      }),
    [patch, read],
  );

  const setUpFast = useCallback(() => {
    setFast('setting-up');
    enableFastDownloads()
      .then(() => live.current && setFast('ready'))
      .catch((err: unknown) => live.current && setFast({ failed: formatError(err) }));
  }, []);

  const begin = useCallback(async () => {
    const found = await read(['llama', 'stable_diffusion']);
    if (found === null) return;
    if (installable(found.llama ?? null)) await install('llama');
    if (installable(found.stable_diffusion ?? null)) await install('stable_diffusion');
  }, [install, read]);

  const start = useCallback(() => {
    if (once.current) return;
    once.current = true;
    setStarted(true);
    if (!fastReady && python) setUpFast();
    void begin();
  }, [begin, fastReady, python, setUpFast]);

  const retry = useCallback(
    (kind: RuntimeKind) => {
      if (views[kind].installing === null) void install(kind);
    },
    [install, views],
  );

  const recheck = useCallback(() => {
    // A read that failed before anything was read is the start's to repeat.
    if (views.llama.status === null && views.stable_diffusion.status === null) void begin();
    else void read(['llama', 'stable_diffusion']);
  }, [begin, read, views]);

  // While only a terminal can build llama.cpp, look for it every few seconds.
  const waiting = started && needsBuild(views.llama.status);
  useEffect(() => {
    if (!waiting) return undefined;
    const timer = setInterval(() => void read(['llama']), BUILD_POLL_MS);
    return () => clearInterval(timer);
  }, [waiting, read]);

  return { llama: views.llama, sd: views.stable_diffusion, fast, readError, start, retry, retryFast: setUpFast, recheck };
}
