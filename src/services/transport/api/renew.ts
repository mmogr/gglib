/**
 * Renewing the session's credential after a stream was refused with a 401.
 *
 * A request renews it itself (`client.ts`), but a stream reads the headers
 * `getAuthHeaders` returns, which only a new client sets. The daemon mints a
 * new token at every start, so after the desktop's service restarts every
 * reconnect would present the old one forever. Dropping the cached client and
 * building another asks the desktop app for the token again, and a page reads
 * the one it keeps, which a newer link may have replaced.
 */

import { SSEHttpError } from '../../../utils/sse';
import { getClient, resetClientCache } from './client';

/** Renew the credential when `error` is a stream's 401; otherwise nothing. */
export async function renewAfterRefusal(error: unknown): Promise<void> {
  if (error instanceof SSEHttpError && error.status === 401) {
    resetClientCache();
    await getClient();
  }
}
