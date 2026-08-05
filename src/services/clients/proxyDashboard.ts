/**
 * Proxy dashboard client.
 *
 * Unlike every other client in `services/clients/`, this one talks directly
 * to an already-running proxy's own HTTP port — it does **not** go through
 * `getTransport()`/Tauri IPC. The proxy is always a plain HTTP/axum server
 * (see `services/proxyEvents.ts`'s doc comment: "Proxy always uses HTTP/axum
 * (no Tauri commands)"), and its dashboard SSE endpoint lives on the proxy's
 * own port, not the app's own backend API port — the same relationship the
 * CLI's `gglib proxy dashboard` command has to it
 * (`crates/gglib-cli/src/handlers/proxy_dashboard.rs`).
 *
 * Because the port is the proxy's, so is the credential: the `apiKey`
 * parameter is the `proxyApiKey` setting, passed down from callers exactly
 * as `host`/`port` already are. That is deliberately not read from the
 * transport layer's session — that token belongs to the app's own backend,
 * which is a different server on a different port.
 *
 * @module services/clients/proxyDashboard
 */

import { appLogger } from '../platform';
import type { DashboardSnapshot } from '../transport/types/dashboard';
import { createSSEStream } from '../../utils/sse';

/** Delay before re-opening a dropped dashboard stream. */
const RECONNECT_DELAY_MS = 2000;

/**
 * Authorization header for the proxy's own port, if it requires one.
 *
 * A proxy bound to loopback with no key configured is the common case and
 * needs no header at all — sending an empty one would turn a working
 * unauthenticated setup into a 401.
 */
function proxyAuthHeaders(apiKey?: string | null): Record<string, string> {
  return apiKey ? { Authorization: `Bearer ${apiKey}` } : {};
}

/**
 * Subscribe to a running proxy's live dashboard stream, connected to
 * `GET /v1/proxy/status/stream`.
 *
 * Uses `createSSEStream` rather than the native `EventSource`, which cannot
 * send an `Authorization` header at all — the proxy requires one on `/v1/*`
 * whenever a key is configured. The cost is that reconnection is ours to do
 * rather than the browser's, which is what the retry loop below is for.
 *
 * The server hydrates this stream itself (the first event is always the
 * current full snapshot, followed by live ticks — see
 * `gglib_sse::Broadcaster::subscribe_with_hydration` on the Rust side), so
 * there is no separate initial-fetch step here: the very first `onSnapshot`
 * call already contains complete state. That also means a reconnect
 * re-hydrates on its own.
 *
 * @param host   Proxy host (typically `127.0.0.1`).
 * @param port   Proxy port.
 * @param apiKey The proxy's API key (the `proxyApiKey` setting), or null/undefined
 *               when it runs unauthenticated.
 * @param onSnapshot Called with each decoded snapshot (hydration + every tick).
 * @param onError    Called when the stream drops or is refused. Reconnection
 *                   continues regardless; this is informational for the UI.
 * @returns An unsubscribe function that aborts the stream. Callers
 *          (see `hooks/useProxyDashboard.ts`) must invoke this on cleanup —
 *          the retry loop runs until it is aborted.
 */
export function subscribeProxyDashboard(
  host: string,
  port: number,
  apiKey: string | null | undefined,
  onSnapshot: (snapshot: DashboardSnapshot) => void,
  onError?: (error: unknown) => void
): () => void {
  const url = `http://${host}:${port}/v1/proxy/status/stream`;
  const controller = new AbortController();

  void (async () => {
    while (!controller.signal.aborted) {
      try {
        for await (const message of createSSEStream(url, {
          headers: proxyAuthHeaders(apiKey),
          signal: controller.signal,
        })) {
          if (!message.data) continue;
          try {
            onSnapshot(JSON.parse(message.data) as DashboardSnapshot);
          } catch (error) {
            appLogger.error('service.server', 'Failed to parse proxy dashboard snapshot', {
              error,
              data: message.data,
            });
          }
        }
      } catch (error) {
        // An abort is the caller unsubscribing, not a failure.
        if (controller.signal.aborted) return;
        appLogger.error('service.server', 'Proxy dashboard stream error (will retry)', {
          url,
          error,
        });
        onError?.(error);
      }

      if (controller.signal.aborted) return;
      await new Promise((resolve) => setTimeout(resolve, RECONNECT_DELAY_MS));
    }
  })();

  return () => controller.abort();
}

/**
 * Clear KV cache via `POST /v1/proxy/cache/clear`.
 *
 * @param host   Proxy host (typically `127.0.0.1`).
 * @param port   Proxy port.
 * @param apiKey The proxy's API key, or null/undefined when it runs unauthenticated.
 * @param sessionId Optional session ID to target; omit to clear all sessions.
 * @returns The JSON response body from the proxy.
 */
export async function clearProxyCache(
  host: string,
  port: number,
  apiKey?: string | null,
  sessionId?: string
): Promise<{ status: string; message: string }> {
  const url = `http://${host}:${port}/v1/proxy/cache/clear`;
  const headers: Record<string, string> = proxyAuthHeaders(apiKey);
  if (sessionId) {
    headers['X-Gglib-Session-Id'] = sessionId;
  }
  const res = await fetch(url, { method: 'POST', headers });
  if (!res.ok) {
    throw new Error(`Cache clear failed: ${res.status} ${res.statusText}`);
  }
  return res.json();
}
