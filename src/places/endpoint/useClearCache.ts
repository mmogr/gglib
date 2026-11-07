/**
 * Clear prompt cache, as the endpoint place's Activity and ⋯ and
 * Diagnostics › Live's Prompt cache section offer it: one handler for all
 * three. It presents the proxy's key, so it can act only once the key has
 * been revealed (`null` when there is none) and while the endpoint runs.
 * The outcome is a toast: the proxy's message, or the failure in its words.
 *
 * @module places/endpoint/useClearCache
 */

import { useState } from 'react';
import { useToastContext } from '../../contexts/ToastContext';
import { clearProxyCache } from '../../services/clients/proxyDashboard';
import { formatError } from '../../utils/errors';
import type { EndpointDashboard } from '../library/useResidency';

export interface ClearCache {
  /** Whether a clear can be sent now: the endpoint running and its key revealed. */
  canClear: boolean;
  /** A clear is on its way. */
  clearing: boolean;
  clear: () => Promise<void>;
}

export function useClearCache(dash: Pick<EndpointDashboard, 'host' | 'port' | 'apiKey'>): ClearCache {
  const { showToast } = useToastContext();
  const [clearing, setClearing] = useState(false);
  const canClear = dash.port !== null && dash.apiKey !== undefined;
  const clear = async () => {
    if (dash.port === null || dash.apiKey === undefined) return;
    setClearing(true);
    try {
      const result = await clearProxyCache(dash.host, dash.port, dash.apiKey);
      showToast(result.message || 'Prompt cache cleared', 'success');
    } catch (err) {
      showToast(formatError(err), 'error');
    } finally {
      setClearing(false);
    }
  };
  return { canClear, clearing, clear };
}
