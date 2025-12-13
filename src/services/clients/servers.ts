/**
 * Servers client module.
 *
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 *
 * @module services/clients/servers
 */

import { getTransport } from '../transport';
import type { ModelId } from '../transport/types/ids';
import type { ServeConfig, ServerInfo } from '../../types';
import type { ServeResponse } from '../transport/types/servers';
import type { ProxyConfig, ProxyStatus } from '../transport/types/proxy';

/**
 * Start a llama.cpp server for a model.
 */
export async function serveModel(config: ServeConfig): Promise<ServeResponse> {
  return getTransport().serveModel(config);
}

/**
 * Stop a running server for a model.
 */
export async function stopServer(modelId: ModelId): Promise<void> {
  return getTransport().stopServer(modelId);
}

/**
 * List all running servers.
 */
export async function listServers(): Promise<ServerInfo[]> {
  return getTransport().listServers();
}

// ============================================================================
// Proxy Operations
// ============================================================================

/**
 * Get current proxy server status.
 */
export async function getProxyStatus(): Promise<ProxyStatus> {
  return getTransport().getProxyStatus();
}

/**
 * Start the multi-model proxy server.
 */
export async function startProxy(config?: Partial<ProxyConfig>): Promise<ProxyStatus> {
  return getTransport().startProxy(config);
}

/**
 * Stop the proxy server.
 */
export async function stopProxy(): Promise<void> {
  return getTransport().stopProxy();
}
