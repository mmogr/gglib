/**
 * useProxyKey hook.
 *
 * The proxy's API key, for a surface that must present it to the proxy: the
 * dashboard stream and Clear Cache. Settings say only whether a key is
 * stored (ADR 0027), so this asks the daemon's one reveal, each time
 * `active` turns on and again whenever `asOf` changes, so a key saved
 * meanwhile is the one presented. The key is dropped while inactive.
 *
 * Reads through `getTransport()` and no context, so the tray panel, which
 * has none of the app's providers, can use it.
 *
 * @module hooks/useProxyKey
 */

import { useEffect, useState } from 'react';
import { PROXY_API_KEY } from '../constants/secrets';
import { appLogger } from '../services/platform';
import { getTransport } from '../services/transport';

/**
 * The proxy's key while `active`. `null` when none is stored, which the proxy
 * then takes as no key, and when it could not be read, which leaves a keyed
 * proxy's dashboard blank and nothing else; `undefined` while inactive or
 * while the answer is on its way, so a stream can wait for it rather than
 * open once without it.
 *
 * @param active Whether the caller needs the key now.
 * @param asOf   Anything whose change means the key may have changed, such
 *               as the settings the caller renders.
 */
export function useProxyKey(active: boolean, asOf?: unknown): string | null | undefined {
  const [key, setKey] = useState<string | null | undefined>(undefined);

  useEffect(() => {
    if (!active) {
      setKey(undefined);
      return undefined;
    }
    let cancelled = false;
    getTransport()
      .revealSecret(PROXY_API_KEY)
      .then((value) => {
        if (!cancelled) setKey(value);
      })
      .catch((err) => {
        appLogger.error('hook.server', 'Could not read the proxy API key', { err });
        if (!cancelled) setKey(null);
      });
    return () => {
      cancelled = true;
    };
  }, [active, asOf]);

  return key;
}
