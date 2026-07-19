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
 * @module services/clients/proxyDashboard
 */

import { appLogger } from '../platform';
import type { DashboardSnapshot } from '../transport/types/dashboard';

/**
 * Subscribe to a running proxy's live dashboard stream via a native
 * `EventSource` connected to `GET /v1/proxy/status/stream`.
 *
 * The server hydrates this stream itself (the first event is always the
 * current full snapshot, followed by live ticks — see
 * `gglib_sse::Broadcaster::subscribe_with_hydration` on the Rust side), so
 * there is no separate initial-fetch step here: the very first `onSnapshot`
 * call already contains complete state.
 *
 * @param host   Proxy host (typically `127.0.0.1`).
 * @param port   Proxy port.
 * @param onSnapshot Called with each decoded snapshot (hydration + every tick).
 * @param onError    Called when the underlying `EventSource` reports an error
 *                    (the browser will keep retrying the connection itself;
 *                    this is purely informational for the UI).
 * @returns An unsubscribe function that closes the `EventSource`. Callers
 *          (see `hooks/useProxyDashboard.ts`) must invoke this on cleanup —
 *          `EventSource` does not close itself when a component unmounts.
 */
export function subscribeProxyDashboard(
  host: string,
  port: number,
  onSnapshot: (snapshot: DashboardSnapshot) => void,
  onError?: (event: Event) => void
): () => void {
  const url = `http://${host}:${port}/v1/proxy/status/stream`;
  const eventSource = new EventSource(url);

  eventSource.onmessage = (event) => {
    if (!event.data) return;
    try {
      const snapshot = JSON.parse(event.data) as DashboardSnapshot;
      onSnapshot(snapshot);
    } catch (error) {
      appLogger.error('service.server', 'Failed to parse proxy dashboard snapshot', {
        error,
        data: event.data,
      });
    }
  };

  eventSource.onerror = (event) => {
    appLogger.error('service.server', 'Proxy dashboard stream error (browser will auto-retry)', {
      url,
    });
    onError?.(event);
  };

  return () => eventSource.close();
}

/**
 * Clear KV cache via `POST /v1/proxy/cache/clear`.
 *
 * @param host   Proxy host (typically `127.0.0.1`).
 * @param port   Proxy port.
 * @param sessionId Optional session ID to target; omit to clear all sessions.
 * @returns The JSON response body from the proxy.
 */
export async function clearProxyCache(
  host: string,
  port: number,
  sessionId?: string
): Promise<{ status: string; message: string }> {
  const url = `http://${host}:${port}/v1/proxy/cache/clear`;
  const headers: Record<string, string> = {};
  if (sessionId) {
    headers['X-Gglib-Session-Id'] = sessionId;
  }
  const res = await fetch(url, { method: 'POST', headers });
  if (!res.ok) {
    throw new Error(`Cache clear failed: ${res.status} ${res.statusText}`);
  }
  return res.json();
}
