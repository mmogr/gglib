// Server domain operations
// Start/stop llama.cpp servers for models

import { ServeConfig } from "../../types";
import { apiFetch, isTauriApp, tauriInvoke, ApiResponse } from "./base";

export interface ServeResponse {
  port: number;
  message: string;
}

/**
 * Start a llama.cpp server for a model.
 */
export async function serveModel(config: ServeConfig): Promise<ServeResponse> {
  if (isTauriApp) {
    const response = await tauriInvoke<ServeResponse>('serve_model', {
      id: config.id,
      ctxSize: config.ctx_size,
      contextLength: config.context_length,
      mlock: config.mlock || false,
      port: config.port,
      jinja: config.jinja,
    });
    return response;
  } else {
    const response = await apiFetch(`/models/${config.id}/start`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        context_length: config.context_length || (config.ctx_size ? parseInt(config.ctx_size) : undefined),
        port: config.port,
        mlock: config.mlock || false,
        jinja: config.jinja,
      }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to start server');
    }

    const data: ApiResponse<ServeResponse> = await response.json();
    if (!data.data) {
      throw new Error('Invalid server response');
    }
    return data.data;
  }
}

/**
 * Stop a running server for a model.
 */
export async function stopServer(modelId: number): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('stop_server', { modelId });
  } else {
    const response = await apiFetch(`/models/${modelId}/stop`, {
      method: 'POST',
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to stop server');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Server stopped';
  }
}

/**
 * List all running servers.
 */
export async function listServers(): Promise<unknown[]> {
  if (isTauriApp) {
    return await tauriInvoke<unknown[]>('list_servers');
  } else {
    const response = await apiFetch(`/servers`);
    if (!response.ok) {
      throw new Error(`Failed to fetch servers: ${response.statusText}`);
    }
    const data: ApiResponse<unknown[]> = await response.json();
    return data.data || [];
  }
}
