/**
 * A second launch of the desktop app, heard by its main window.
 * TRANSPORT_EXCEPTION: a Tauri event, sent by the app itself, not the daemon.
 *
 * Launching gglib while it runs brings this window forward and sends it
 * `second-launch` with what that launch was started with
 * (`src-tauri/src/second_launch.rs`). Nothing acts on it yet: the latest is
 * kept here, for a later feature (deep links) to read. The name and the
 * fields are held to the Rust by `tests/ts/contracts/secondLaunch.test.ts`.
 */

import { isDesktop } from './detect';
import { appLogger } from './logging/appLogger';

/** The event's name: `EVENT` in `second_launch.rs`. */
export const SECOND_LAUNCH_EVENT = 'second-launch';

/** What a second launch carried: `SecondLaunchPayload` in `second_launch.rs`. */
export interface SecondLaunchPayload {
  /** The arguments, in order, without the program and gglib's own flags. */
  args: string[];
  /** The folder the second launch was started in. */
  cwd: string;
}

let last: SecondLaunchPayload | null = null;

/** The latest second launch this window heard of, or `null` for none yet. */
export function lastSecondLaunch(): SecondLaunchPayload | null {
  return last;
}

/**
 * Listen for second launches until the returned function is called.
 * A no-op in a browser, where there is no app to launch twice.
 */
export function listenToSecondLaunch(): () => void {
  if (!isDesktop()) {
    return () => {};
  }

  let stopped = false;
  let stopListening: (() => void) | null = null;
  void import('@tauri-apps/api/event')
    .then(({ listen }) =>
      listen<SecondLaunchPayload>(SECOND_LAUNCH_EVENT, ({ payload }) => {
        last = payload;
        // The count only: an argument may one day be a link carrying a code.
        appLogger.debug('service.platform', 'gglib was launched again', { arguments: payload.args.length });
      }),
    )
    .then((off) => {
      if (stopped) off();
      else stopListening = off;
    })
    .catch((error: unknown) => {
      appLogger.error('service.platform', 'Could not listen for a second launch', { error });
    });

  return () => {
    stopped = true;
    stopListening?.();
  };
}
