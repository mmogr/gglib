/**
 * Whether a daemon is there at all, and whether it takes this page's token.
 *
 * `probeDaemon` asks `/health`, the route the desktop app's own probe and
 * the CLI ask, which sits outside the token guard: a refused token is a
 * daemon that is there. As those probes do, it
 * counts only an answer that says it is gglib's (`service: "gglib-daemon"`):
 * another program on the port is no daemon. The daemon lets the pages it
 * lets read `/api` read `/health` too (CORS), the desktop window included.
 * It is a plain `fetch` outside the client's 401 handling (the token
 * re-read), through the same time limit as every request
 * (`timeLimit.ts`): 1.5 s, the reply included, so a daemon that does not
 * answer reads as not there instead of leaving the question open.
 *
 * `tokenRefusal` is the one `/api` read (`/api/version`) a browser tab makes
 * to learn whether the daemon takes the token it holds. A daemon that
 * started since the tab took it refuses it, and the daemon's own sentence
 * (open the page from the link `gglib web` prints) is the answer. It goes
 * through `apiFetch`, which retries nothing; a refusal of any other kind is
 * not this one, and reads as `null`.
 */

import { TransportError } from '../errors';
import { apiFetch, healthUrl } from './client';
import { TOKEN_REQUIRED } from './daemonToken';
import { fetchWithin, HEALTH_LIMIT_MS, limitFor } from './timeLimit';

/** Whether a daemon answers at this session's address. */
export async function probeDaemon(): Promise<boolean> {
  try {
    return await fetchWithin(
      await healthUrl(),
      { cache: 'no-store' },
      limitFor('GET', { timeoutMs: HEALTH_LIMIT_MS }),
      async (response) => {
        if (!response.ok) return false;
        const body = (await response.json()) as { service?: unknown } | null;
        return body?.service === GGLIB_DAEMON;
      },
    );
  } catch {
    return false;
  }
}

/** What `/health` says it is, on a gglib daemon (`gglib-axum`'s `health_check`). */
const GGLIB_DAEMON = 'gglib-daemon';

/** The daemon's sentence when it refuses this session's token, else `null`. */
export async function tokenRefusal(): Promise<string | null> {
  try {
    await apiFetch('/api/version');
    return null;
  } catch (err) {
    const type = TransportError.isTransportError(err) ? (err.details as { type?: unknown } | undefined)?.type : undefined;
    return type === TOKEN_REQUIRED ? (err as TransportError).message : null;
  }
}
