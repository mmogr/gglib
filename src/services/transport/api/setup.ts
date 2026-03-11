/**
 * Setup API module.
 * Handles first-run system setup status checks and provisioning.
 */

import { get, post } from './client';
import { getApiBaseUrl, getAuthHeaders } from './client';
import type { SetupStatus, LlamaInstallProgress } from '../../../types/setup';

/**
 * Get the current system setup status.
 */
export async function getSetupStatus(): Promise<SetupStatus> {
  return get<SetupStatus>('/api/system/setup-status');
}

/**
 * Install llama.cpp pre-built binaries with progress streaming.
 * 
 * Uses SSE to stream download progress events.
 * 
 * @param onProgress Called with download progress updates
 * @param onComplete Called when installation finishes successfully
 * @param onError Called when installation fails
 * @returns An abort function to cancel the installation
 */
export function streamLlamaInstall(
  onProgress: (progress: LlamaInstallProgress) => void,
  onComplete: () => void,
  onError: (error: string) => void,
): () => void {
  const controller = new AbortController();
  const baseUrl = getApiBaseUrl();
  const headers = getAuthHeaders();

  // Use fetch directly for SSE streaming (POST request)
  fetch(`${baseUrl}/api/system/install-llama`, {
    method: 'POST',
    headers: {
      ...headers,
      Accept: 'text/event-stream',
    },
    signal: controller.signal,
  })
    .then(async (response) => {
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      if (!response.body) {
        throw new Error('No response body for SSE stream');
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        let currentEventType = '';
        for (const line of lines) {
          if (line.startsWith('event: ')) {
            currentEventType = line.slice(7).trim();
          } else if (line.startsWith('data: ')) {
            const data = line.slice(6);
            try {
              if (currentEventType === 'progress') {
                const progress: LlamaInstallProgress = JSON.parse(data);
                onProgress(progress);
              } else if (currentEventType === 'complete') {
                onComplete();
              } else if (currentEventType === 'error') {
                const errorData = JSON.parse(data);
                onError(errorData.message || 'Unknown error');
              }
            } catch {
              // Ignore parse errors for partial data
            }
            currentEventType = '';
          }
        }
      }
    })
    .catch((err) => {
      if (err.name !== 'AbortError') {
        onError(err.message || 'Installation failed');
      }
    });

  return () => controller.abort();
}

/**
 * Set up the Python fast-download helper environment.
 */
export async function setupPython(): Promise<void> {
  return post<void>('/api/system/setup-python');
}
