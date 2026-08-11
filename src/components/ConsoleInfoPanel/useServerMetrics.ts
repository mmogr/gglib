import { useEffect, useState } from 'react';

export interface ServerMetrics {
  kvCacheUsageRatio: number | null;
  kvCacheTokens: number | null;
  nTokensMax: number;
  promptTokensTotal: number;
  predictedTokensTotal: number;
  requestsProcessing: number;
}

/** Parse Prometheus-format metrics text into structured data. */
export function parsePrometheusMetrics(text: string): ServerMetrics {
  const getMetricValue = (name: string): number | null => {
    const regex = new RegExp(`^${name}\\s+([\\d.]+)`, 'm');
    const match = text.match(regex);
    return match ? parseFloat(match[1]) : null;
  };

  return {
    kvCacheUsageRatio: getMetricValue('llamacpp:kv_cache_usage_ratio'),
    kvCacheTokens: getMetricValue('llamacpp:kv_cache_tokens'),
    nTokensMax: getMetricValue('llamacpp:n_tokens_max') ?? 0,
    promptTokensTotal: getMetricValue('llamacpp:prompt_tokens_total') ?? 0,
    predictedTokensTotal: getMetricValue('llamacpp:tokens_predicted_total') ?? 0,
    requestsProcessing: getMetricValue('llamacpp:requests_processing') ?? 0,
  };
}

/**
 * Poll llama-server's /metrics every 2s while the server is running.
 *
 * Uses setTimeout recursion + AbortController. On fetch failure the local loop
 * stops and the metrics clear without touching global state; polling resumes
 * automatically when `isRunning` flips back via the server:started event.
 *
 * Each poll stores a freshly parsed object, so the returned value's identity
 * changes once per poll — consumers use it directly as the metric-history tick.
 */
export function useServerMetrics(serverPort: number, isRunning: boolean): ServerMetrics | null {
  const [sample, setSample] = useState<ServerMetrics | null>(null);

  useEffect(() => {
    if (!isRunning) {
      setSample(null);
      return;
    }

    let cancelled = false;
    const controller = new AbortController();

    const fetchMetrics = async (): Promise<void> => {
      try {
        const response = await fetch(`http://127.0.0.1:${serverPort}/metrics`, {
          signal: controller.signal,
        });
        if (response.ok && !cancelled) {
          const text = await response.text();
          setSample(parsePrometheusMetrics(text));
        }
      } catch {
        if (!cancelled) {
          setSample(null);
          return;
        }
      }

      if (!cancelled && isRunning) {
        setTimeout(fetchMetrics, 2000);
      }
    };

    fetchMetrics();

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [serverPort, isRunning]);

  return sample;
}
