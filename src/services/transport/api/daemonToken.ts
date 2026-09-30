/**
 * The daemon's token, as the link `gglib web` prints hands it to the page.
 *
 * The routes that change who is trusted (pairing a device, forgetting one,
 * joining another machine) answer only this token, on loopback too, and every
 * other route takes it as well. The link carries it as a fragment,
 * `#token=…`, which a browser never sends in a request. The page keeps it in
 * `sessionStorage`, never `localStorage`, so it goes when the tab does, and
 * takes it out of the address bar so it is not bookmarked or shared.
 */

const STORAGE_KEY = 'gglib_daemon_token';

/** The `type` of the daemon's 401 on a route that wants its token. */
const TOKEN_REQUIRED = 'DAEMON_TOKEN_REQUIRED';

function stored(): string | undefined {
  try {
    return sessionStorage.getItem(STORAGE_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

/**
 * Take the token out of the address bar when the link put one there, keep
 * it for this tab, and return whichever token this tab holds.
 */
export function takeDaemonToken(): string | undefined {
  const fragment = new URLSearchParams(window.location.hash.slice(1));
  const token = fragment.get('token')?.trim();
  if (!token) {
    return stored();
  }
  try {
    sessionStorage.setItem(STORAGE_KEY, token);
  } catch {
    // Storage unavailable: the token still serves this page until it reloads.
  }
  fragment.delete('token');
  const rest = fragment.toString();
  const { pathname, search } = window.location;
  window.history.replaceState(window.history.state, '', `${pathname}${search}${rest ? `#${rest}` : ''}`);
  return token;
}

/**
 * Whether `response` is the daemon refusing a route that wants its token,
 * which no API key opens, so asking for one would not help. Reads a clone,
 * leaving the body for the error the caller shows.
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
