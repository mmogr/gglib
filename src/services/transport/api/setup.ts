/**
 * Setup API module.
 * Handles first-run system setup status checks and provisioning.
 */

import { get, post } from './client';
import { parseFrame, streamSse } from './sse';
import type {
  SetupStatus,
  LlamaInstallProgress,
  VulkanStatus,
  LlamaStatus,
  LlamaUpdateCheck,
  LlamaUninstallOutcome,
  BuildEvent,
  Diagnostics,
  ModelRecommendation,
} from '../../../types/setup';

/**
 * Get the current system setup status.
 */
export async function getSetupStatus(): Promise<SetupStatus> {
  return get<SetupStatus>('/api/config/system/setup-status');
}

/**
 * Get Vulkan build-readiness status.
 */
export async function getVulkanStatus(): Promise<VulkanStatus> {
  return get<VulkanStatus>('/api/config/system/vulkan-status');
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
  return streamSse('/api/config/system/install-llama', {
    onFrame: (frame) => {
      if (frame.event === 'progress') {
        const progress = parseFrame<LlamaInstallProgress>(frame);
        if (progress) onProgress(progress);
      } else if (frame.event === 'complete') {
        onComplete();
      } else if (frame.event === 'error') {
        onError(parseFrame<{ message?: string }>(frame)?.message ?? 'Unknown error');
      }
    },
    onError,
  });
}

/**
 * Set up the Python fast-download helper environment.
 */
export async function setupPython(): Promise<void> {
  return post<void>('/api/config/system/setup-python');
}

/**
 * What llama.cpp install is present — version, health, acceleration, and what
 * the binary reports about itself. Local and cheap; safe on mount.
 */
export async function getLlamaStatus(): Promise<LlamaStatus> {
  return get<LlamaStatus>('/api/config/system/llama-status');
}

/**
 * How far behind upstream the llama.cpp checkout is.
 *
 * POST because it runs `git fetch` — seconds of network, not a page-load
 * request. Only meaningful for a source install; a prebuilt one has no
 * repository to compare (`repoPresent: false`).
 */
export async function checkLlamaUpdates(): Promise<LlamaUpdateCheck> {
  return post<LlamaUpdateCheck>('/api/config/system/llama-check-updates');
}

/** Remove llama.cpp: source checkout, binaries and build config. */
export async function uninstallLlama(): Promise<LlamaUninstallOutcome> {
  return post<LlamaUninstallOutcome>('/api/config/system/uninstall-llama');
}

/**
 * Pull upstream and rebuild llama.cpp, streaming build progress.
 *
 * Same event vocabulary as building from source. Aborting stops the stream,
 * not the build — the compile continues on the daemon.
 */
export function streamLlamaUpdate(
  onEvent: (event: BuildEvent) => void,
  onError: (error: string) => void,
  /** Called when the stream closes cleanly, whether or not it reported a result. */
  onClose?: () => void,
): () => void {
  return streamSse('/api/config/system/update-llama', {
    onFrame: (frame) => {
      // Every payload carries its own `type`, so the SSE event name is
      // redundant here and the parsed object is the single source of truth.
      const event = parseFrame<BuildEvent>(frame);
      if (event) onEvent(event);
    },
    onClose,
    onError,
  });
}

/**
 * Dependency matrix, resolved paths, detected acceleration and accelerator
 * state — `gglib config check-deps`, `paths` and `fast-downloads status` in
 * one request, because the panel reads them together.
 */
export async function getDiagnostics(): Promise<Diagnostics> {
  return get<Diagnostics>('/api/config/system/diagnostics');
}

/** Provision the hf_xet download accelerator. */
export async function enableFastDownloads(): Promise<void> {
  return post<void>('/api/config/system/setup-python');
}

/** Remove it. Downloads revert to native HTTP — slower, not broken. */
export async function disableFastDownloads(): Promise<{ removed: boolean }> {
  return post<{ removed: boolean }>('/api/config/system/disable-fast-downloads');
}

/**
 * A model sized to this machine, from the same shortlist `gglib up` uses.
 *
 * Resolves to `null` when nothing fits — that is the answer, not a failure.
 */
export async function getRecommendedModel(): Promise<ModelRecommendation | null> {
  return get<ModelRecommendation | null>('/api/config/system/recommend-model');
}
