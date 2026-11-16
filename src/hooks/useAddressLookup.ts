/**
 * Where the desktop app's lookup of the daemon's address stands
 * (`services/transport/api/addressLookup`), for the window's `ServiceGate`
 * and the tray popover: while it is out and the daemon has not answered,
 * both say gglib is starting; when it failed, the service-down screen says
 * why in the launch's own words.
 *
 * @module hooks/useAddressLookup
 */

import { useSyncExternalStore } from 'react';
import { addressLookup, onAddressLookup, type AddressLookup } from '../services/transport/api/addressLookup';

/**
 * What the window and the popover say while the app's launch of the daemon
 * is in flight: the word the tray's status line says ("gglib · starting",
 * `src-tauri/src/tray/icon.rs`). Written here, not read from the daemon,
 * because nothing is listening to say it until the launch has an answer.
 */
export const STARTING_WORDS = 'gglib is starting…';

/** The lookup as it stands, re-rendering when it changes. */
export function useAddressLookup(): AddressLookup {
  return useSyncExternalStore(onAddressLookup, addressLookup);
}
