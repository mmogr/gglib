/**
 * Proxy API module.
 * Handles multi-model proxy server management.
 */

import { get, post } from './client';
import type { ProxyConfig, ProxyStatus, StartPinnedRequest } from '../types/proxy';

/**
 * Get current proxy server status.
 */
export async function getProxyStatus(): Promise<ProxyStatus> {
  return get<ProxyStatus>('/api/proxy/status');
}

/**
 * Start the multi-model proxy server.
 */
export async function startProxy(config?: Partial<ProxyConfig>): Promise<ProxyStatus> {
  return post<ProxyStatus>('/api/proxy/start', config);
}

/**
 * Start the proxy pinned to one model. The daemon resolves the launch
 * cascade (sampling, MTP, context, cache) server-side, exactly as
 * `gglib serve` does.
 */
export async function startPinnedProxy(request: StartPinnedRequest): Promise<ProxyStatus> {
  return post<ProxyStatus>('/api/proxy/start-pinned', request);
}

/**
 * Stop the proxy server.
 */
export async function stopProxy(): Promise<void> {
  await post<void>('/api/proxy/stop', null);
}

/**
 * Shut the daemon down — the GUI face of `gglib daemon stop`.
 *
 * The daemon owns every running inference server, so this stops those too and
 * leaves the app without a backend until it is started again. The request may
 * not get a clean response: the server is shutting down as it replies, so a
 * transport error here often means it worked.
 */
export async function shutdownDaemon(): Promise<void> {
  return post<void>('/api/daemon/shutdown');
}
