/**
 * Tauri platform transport implementation.
 * Provides OS-specific operations via Tauri IPC.
 */

/**
 * Invoke Tauri command.
 */
async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // @ts-expect-error - Tauri API is injected at runtime
  const { invoke } = window.__TAURI_INTERNALS__;
  return invoke(cmd, args);
}

/**
 * Check llama.cpp binary installation status.
 */
export async function checkLlamaStatus(): Promise<{ installed: boolean; version?: string }> {
  return invokeTauri('check_llama_status');
}

/**
 * Install llama.cpp binary (downloads prebuilt binaries).
 */
export async function installLlama(): Promise<void> {
  await invokeTauri('install_llama');
}

/**
 * Open URL in system browser.
 */
export async function openUrl(url: string): Promise<void> {
  await invokeTauri('open_url', { url });
}
