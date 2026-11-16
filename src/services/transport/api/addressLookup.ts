/**
 * Where the desktop app's lookup of the daemon's address stands.
 *
 * In the desktop app `client.ts` asks the app where the daemon is
 * (`get_embedded_api_info`), and the app answers only once its launch of the
 * daemon has settled: a little over the launch's own budget (`LAUNCH_WAIT`
 * in `src-tauri/src/daemon`) after the window opens, at most. While the first
 * answer is out, the daemon cannot say anything, since nothing is listening
 * yet; so this is the one thing that knows gglib is starting. When the answer
 * is a failure, its words are the launch's own (the daemon did not come up in
 * time, no `gglib` binary to launch, the port held by another program), and
 * the service-down screen says them.
 *
 * `pending` only until the first answer. Asking again (every health probe
 * asks while the last answer was a failure) keeps the last answer until its
 * own replaces it, so a sentence on screen is not taken down and put back at
 * every probe. Nothing here asks, waits or times anything: it records what
 * `client.ts` heard.
 */

/** The lookup: not yet answered, answered, or answered with why the daemon is not there. */
export type AddressLookup =
  | { readonly state: 'pending' }
  | { readonly state: 'known' }
  | { readonly state: 'failed'; readonly reason: string };

const KNOWN: AddressLookup = { state: 'known' };

let current: AddressLookup = { state: 'pending' };
const listeners = new Set<() => void>();

function set(next: AddressLookup): void {
  current = next;
  for (const listener of listeners) listener();
}

/** The lookup as it stands. */
export function addressLookup(): AddressLookup {
  return current;
}

/** Call `listener` whenever the lookup changes; returns the unsubscribe. */
export function onAddressLookup(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The app answered with the daemon's address. */
export function lookupAnswered(): void {
  if (current !== KNOWN) set(KNOWN);
}

/** The app answered that the daemon is not there: the launch's own words. */
export function lookupRefused(reason: string): void {
  if (current.state === 'failed' && current.reason === reason) return;
  set({ state: 'failed', reason });
}
