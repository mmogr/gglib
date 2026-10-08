/**
 * Servers API module.
 * Handles llama.cpp server lifecycle management.
 */

import { post, get } from './client';
import type { ModelId } from '../types/ids';
import type { ServeConfig, ServerInfo, ToolSupportResponse } from '../../../types';
import type { StartServerResponse } from '../../../types/generated/StartServerResponse';
import type { StopServerBody } from '../../../types/generated/StopServerBody';
import { toStartServerRequest } from '../mappers';

/**
 * Start a llama.cpp server for a model.
 */
export async function serveModel(config: ServeConfig): Promise<StartServerResponse> {
  const request = toStartServerRequest(config);
  return post<StartServerResponse>('/api/servers/start', { id: config.id, ...request });
}

/**
 * Stop a running server for a model.
 */
export async function stopServer(modelId: ModelId): Promise<void> {
  const body: StopServerBody = { model_id: modelId };
  await post<void>('/api/servers/stop', body);
}

/**
 * List all running servers.
 */
export async function listServers(): Promise<ServerInfo[]> {
  return get<ServerInfo[]>('/api/servers');
}

/**
 * Retrieve tool-calling capability for a running server's model.
 */
export async function getServerToolSupport(modelId: ModelId): Promise<ToolSupportResponse> {
  return get<ToolSupportResponse>(`/api/servers/${modelId}/tool-support`);
}
