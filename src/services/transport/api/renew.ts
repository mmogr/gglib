/**
 * Renewing the session's credential after a stream was refused with a 401.
 *
 * A request renews it itself (`client.ts`), but a stream is opened with
 * `apiFetch`, which sends the token the cached client holds. The daemon mints
 * a new token at every start, so after the desktop's service restarts every
 * reconnect would present the old one forever. Dropping the cached client and
 * building another asks the desktop app for the token again, and a page reads
 * the one it keeps, which a newer link may have replaced.
 */

import { TransportError } from '../errors';
import { getClient, resetClientCache } from './client';

/**
 * Renew the credential when `error` is a stream's 401; otherwise nothing.
 * Says whether it renewed: a 401 is the daemon answering, so the caller may
 * try again at once with the new credential.
 */
export async function renewAfterRefusal(error: unknown): Promise<boolean> {
  const status = TransportError.isTransportError(error)
    ? (error.details as { status?: number } | undefined)?.status
    : undefined;
  if (status !== 401) return false;
  resetClientCache();
  await getClient();
  return true;
}
