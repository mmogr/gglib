/**
 * Servers transport types.
 * Handles llama.cpp server lifecycle management.
 */

import type { ServeConfig, ServerInfo } from '../../../types';

// Re-export existing types
export type { ServeConfig, ServerInfo };

/**
 * Response from starting a server.
 */
import type { StartServerResponse as ServeResponse } from '../../../types/generated/StartServerResponse';
export type { ServeResponse };
