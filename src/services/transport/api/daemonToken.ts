/**
 * The daemon's token, as the link `gglib web` prints hands it to the page.
 *
 * Every `/api` route asks for this token, on loopback too. The link carries
 * it as a fragment, `#token=…`, which a browser never sends in a request. The
 * page takes it out of the address bar, so it is not shared with a copied URL,
 * and keeps it in `localStorage`, which is this origin's alone, so a bookmark
 * of the page keeps working until the daemon next starts and mints a new one.
 */

const STORAGE_KEY = 'gglib_daemon_token';

/** The `type` of the daemon's 401 when it wanted its token. */
const TOKEN_REQUIRED = 'DAEMON_TOKEN_REQUIRED';

/**
 * The token this page load took from its link. Held here as well as in
 * storage, because storage can refuse to keep it (a private window, blocked
 * site data), and the fragment is gone once it is taken.
 */
let taken: string | undefined;

function stored(): string | undefined {
  try {
    return localStorage.getItem(STORAGE_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

/**
 * Take the token out of the address bar when the link put one there, keep
 * it, and return whichever token this page holds.
 */
export function takeDaemonToken(): string | undefined {
  const fragment = new URLSearchParams(window.location.hash.slice(1));
  const token = fragment.get('token')?.trim();
  if (!token) {
    return taken ?? stored();
  }
  taken = token;
  try {
    localStorage.setItem(STORAGE_KEY, token);
  } catch {
    // Storage refused it: `taken` still serves this page load.
  }
  fragment.delete('token');
  const rest = fragment.toString();
  const { pathname, search } = window.location;
  window.history.replaceState(window.history.state, '', `${pathname}${search}${rest ? `#${rest}` : ''}`);
  return token;
}

/**
 * Whether `response` is the daemon asking for its token, which no API key
 * opens, so asking the person for a key would not help. Reads a clone,
 * leaving the body, which says how to get the token, for the error shown.
 */
export async function isDaemonTokenRefusal(response: Response): Promise<boolean> {
  if (response.status !== 401) {
    return false;
  }
  try {
    const body = (await response.clone().json()) as { type?: unknown };
    return body.type === TOKEN_REQUIRED;
  } catch {
    return false;
  }
}
