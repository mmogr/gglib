/**
 * Servers transport sub-interface.
 * Handles llama.cpp server lifecycle management.
 */

import type { ModelId } from './ids';
import type { ServeConfig, ServerInfo } from '../../../types';

// Re-export existing types
export type { ServeConfig, ServerInfo };

/**
 * Response from starting a server.
 */
export interface ServeResponse {
  port: number;
  message: string;
}

/**
 * Servers transport operations.
 */
export interface ServersTransport {
  /** Start a llama.cpp server for a model. */
  serveModel(config: ServeConfig): Promise<ServeResponse>;

  /** Stop a running server for a model. */
  stopServer(modelId: ModelId): Promise<void>;

  /** List all running servers. */
  listServers(): Promise<ServerInfo[]>;
}
