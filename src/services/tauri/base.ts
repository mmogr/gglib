// Shared utilities for Tauri/REST API service layer
// Used by all domain modules

import { invoke } from "@tauri-apps/api/core";
import { getApiBase } from "../../utils/apiBase";
import { isTauriApp } from "../../utils/platform";

// Re-export for convenience
export { isTauriApp };

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

/**
 * Fetch from the REST API with the correct base URL.
 */
export async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const apiBase = await getApiBase();
  return fetch(`${apiBase}${path}`, init);
}

/**
 * Parse an API response, throwing on error.
 */
export async function parseApiResponse<T>(
  response: Response,
  errorPrefix: string
): Promise<T | undefined> {
  if (!response.ok) {
    let errorMessage = errorPrefix;
    try {
      const error: ApiResponse<unknown> = await response.json();
      errorMessage = error.error || errorMessage;
    } catch {
      errorMessage = response.statusText || errorMessage;
    }
    throw new Error(errorMessage);
  }

  try {
    const data: ApiResponse<T> = await response.json();
    return data.data;
  } catch {
    throw new Error('Invalid response from server');
  }
}

/**
 * Invoke a Tauri command with typed response.
 */
export async function tauriInvoke<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  return await invoke<T>(command, args);
}
