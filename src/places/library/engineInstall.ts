/**
 * What the llama.cpp banner is doing, kept for as long as the window is
 * open: whether "Not now" put it away, and the install it started, with its
 * progress and how it ended. Held here and not in the banner, which unmounts
 * whenever the library does (a chat, the settings), so neither a dismissal
 * nor a running install is lost by going elsewhere and coming back.
 *
 * @module places/library/engineInstall
 */

import { createEventStore } from '../../services/createEventStore';
import { streamEngineInstall } from '../../services/transport/api/setup';
import type { LlamaProgressEvent } from '../../types/setup';

export interface EngineInstall {
  dismissed: boolean;
  installing: boolean;
  /** The install's last event, while it runs. */
  progress: LlamaProgressEvent | null;
  /** Why the last install did not finish: the daemon's words, or the stream's end. */
  refusal: string | null;
  /** How many installs have completed, so the banner reads the status again. */
  completed: number;
}

const INITIAL: EngineInstall = { dismissed: false, installing: false, progress: null, refusal: null, completed: 0 };

const store = createEventStore<EngineInstall>(INITIAL);

const update = (change: Partial<EngineInstall>) => store.setState({ ...store.getState(), ...change });

export function useEngineInstall(): EngineInstall {
  return store.useStore();
}

/** "Not now": put the banner away for as long as the window is open. */
export function dismissEngineBanner(): void {
  update({ dismissed: true });
}

/**
 * Run the daemon's llama.cpp install. The stream ends once: completed,
 * failed, a transport error (a refusal among them), or closed having said
 * neither; whichever comes first is the result.
 */
export function installLlama(): void {
  if (store.getState().installing) return;
  let settled = false;
  const fail = (message: string) => {
    if (settled) return;
    settled = true;
    update({ installing: false, progress: null, refusal: message });
  };
  update({ installing: true, progress: null, refusal: null });
  streamEngineInstall(
    'llama',
    (event) => {
      if (settled) return;
      if (event.type === 'completed') {
        settled = true;
        update({ installing: false, progress: null, completed: store.getState().completed + 1 });
      } else if (event.type === 'failed') {
        fail(event.message);
      } else {
        update({ progress: event });
      }
    },
    fail,
    () => fail('The connection to the install ended before it reported a result.'),
  );
}

/** Forget everything, for a test. */
export function resetEngineInstall(): void {
  store.setState(INITIAL);
}
