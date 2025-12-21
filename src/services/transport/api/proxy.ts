/**
 * Proxy API module.
 * Handles multi-model proxy server management.
 */

import { get, post } from './client';
import type { ProxyConfig, ProxyStatus } from '../types/proxy';

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
 * Stop the proxy server.
 */
export async function stopProxy(): Promise<void> {
  await post<void>('/api/proxy/stop', null);
}
