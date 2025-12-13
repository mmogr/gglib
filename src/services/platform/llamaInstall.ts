/**
 * Llama binary installation utilities
 * TRANSPORT_EXCEPTION: Uses Tauri invoke for llama binary management.
 * UI components should import from 'services/platform' rather than checking isTauriApp directly.
 */

import { isDesktop } from './detect';

export interface LlamaStatus {
  installed: boolean;
  canDownload: boolean;
}

export interface LlamaInstallProgress {
  status: 'started' | 'downloading' | 'completed' | 'error';
  downloaded: number;
  total: number;
  percentage: number;
  message: string;
}

/**
 * Check if llama.cpp is installed and if it can be downloaded.
 * On web, always returns { installed: true, canDownload: false }.
 */
export async function checkLlamaInstalled(): Promise<LlamaStatus> {
  if (!isDesktop()) {
    return { installed: true, canDownload: false };
  }

  const { invoke } = await import('@tauri-apps/api/core');
  const result = await invoke<{ installed: boolean; can_download: boolean }>('check_llama_status');
  return {
    installed: result.installed,
    canDownload: result.can_download,
  };
}

/**
 * Install llama.cpp binary.
 * No-op on web.
 */
export async function installLlama(): Promise<void> {
  if (!isDesktop()) {
    return;
  }

  const { invoke } = await import('@tauri-apps/api/core');
  await invoke<string>('install_llama');
}

/**
 * Listen for llama installation progress events.
 * Returns an unsubscribe function.
 * No-op on web (returns empty function).
 */
export async function listenLlamaProgress(
  callback: (progress: LlamaInstallProgress) => void
): Promise<() => void> {
  if (!isDesktop()) {
    return () => {};
  }

  const { listen } = await import('@tauri-apps/api/event');
  const unlisten = await listen<LlamaInstallProgress>('llama-install-progress', (event) => {
    callback(event.payload);
  });
  return unlisten;
}
