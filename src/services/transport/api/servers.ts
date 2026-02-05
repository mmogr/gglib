/**
 * Servers API module.
 * Handles llama.cpp server lifecycle management.
 */

import { post, get } from './client';
import type { ModelId } from '../types/ids';
import type { ServeConfig, ServeResponse, ServerInfo } from '../types/servers';
import { toStartServerRequest } from '../mappers';

/**
 * Start a llama.cpp server for a model.
 */
export async function serveModel(config: ServeConfig): Promise<ServeResponse> {
  const request = toStartServerRequest(config);
  return post<ServeResponse>('/api/servers/start', { id: config.id, ...request });
}

/**
 * Stop a running server for a model.
 */
export async function stopServer(modelId: ModelId): Promise<void> {
  await post<void>('/api/servers/stop', { model_id: modelId });
}

/**
 * List all running servers.
 */
export async function listServers(): Promise<ServerInfo[]> {
  return get<ServerInfo[]>('/api/servers');
}
