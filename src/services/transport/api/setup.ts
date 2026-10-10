/**
 * Setup API module.
 * Handles first-run system setup status checks and provisioning.
 */

import { get, post } from './client';
import { parseFrame, streamSse } from './sse';
import type {
  SetupStatus,
  LlamaProgressEvent,
  LlamaStatus,
  LlamaUpdateCheck,
  LlamaUninstallOutcome,
  BuildEvent,
  Diagnostics,
  ModelRecommendation,
  ImageRuntimeStatus,
} from '../../../types/setup';

/**
 * Get the current system setup status.
 */
export async function getSetupStatus(): Promise<SetupStatus> {
  return get<SetupStatus>('/api/config/system/setup-status');
}


/**
 * Install llama.cpp pre-built binaries, streaming install progress.
 *
 * Same arrangement as `streamLlamaUpdate`: every payload carries its own
 * `type`, so the SSE event name is redundant and the parsed object is the
 * single source of truth.
 *
 * @param onEvent Called for every install event, in arrival order
 * @param onError Called when the transport itself fails
 * @param onClose Called when the stream closes cleanly, whether or not it reported a result
 * @returns A function that stops reading. The install continues on the daemon.
 */
export function streamLlamaInstall(
  onEvent: (event: LlamaProgressEvent) => void,
  onError: (error: string) => void,
  onClose?: () => void,
): () => void {
  return streamSse('/api/config/system/install-llama', {
    onFrame: (frame) => {
      const event = parseFrame<LlamaProgressEvent>(frame);
      if (event) onEvent(event);
    },
    onClose,
    onError,
  });
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
 * Install stable-diffusion.cpp's pre-built `sd-server`, streaming install
 * progress: the same events, under the same SSE names, as
 * `streamLlamaInstall`, so `InstallProgress` draws both.
 *
 * @returns A function that stops reading. The install continues on the daemon.
 */
export function streamSdInstall(
  onEvent: (event: LlamaProgressEvent) => void,
  onError: (error: string) => void,
  onClose?: () => void,
): () => void {
  return streamSse('/api/config/system/install-sd', {
    onFrame: (frame) => {
      const event = parseFrame<LlamaProgressEvent>(frame);
      if (event) onEvent(event);
    },
    onClose,
    onError,
  });
}

/**
 * What image runtime is installed, what an install would download and what
 * to say about it, and the image model running on it. Local; safe on mount.
 */
export async function getSdStatus(): Promise<ImageRuntimeStatus> {
  return get<ImageRuntimeStatus>('/api/config/system/sd-status');
}

/** Remove the image runtime, `.sd/` whole. Refused (409) while an image model runs. */
export async function uninstallSd(): Promise<LlamaUninstallOutcome> {
  return post<LlamaUninstallOutcome>('/api/config/system/uninstall-sd');
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

/**
 * Provision the hf_xet download accelerator.
 *
 * The route is named for its mechanism — it builds the Python environment the
 * accelerator lives in — and this is named for what a person gets. There was a
 * `setupPython` beside it hitting the same path with the same signature, one
 * name per caller; the setup wizard used one and the diagnostics panel the
 * other, and neither knew about the other.
 */
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
