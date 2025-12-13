/**
 * System client module.
 *
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 *
 * @module services/clients/system
 */

import { getTransport } from '../transport';
import type {
  SystemMemoryInfo,
  ModelsDirectoryInfo,
} from '../../types';

/**
 * Get current system memory information.
 */
export async function getSystemMemory(): Promise<SystemMemoryInfo> {
  return getTransport().getSystemMemory();
}

/**
 * Get models directory path and metadata.
 */
export async function getModelsDirectory(): Promise<ModelsDirectoryInfo> {
  return getTransport().getModelsDirectory();
}

/**
 * Set the models directory path.
 */
export async function setModelsDirectory(path: string): Promise<void> {
  return getTransport().setModelsDirectory(path);
}
