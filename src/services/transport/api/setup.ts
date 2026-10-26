/**
 * Setup API module.
 * Handles first-run system setup status checks and provisioning.
 */

import { get, post } from './client';
import { parseFrame, streamSse } from './sse';
import type {
  SetupStatus,
  LlamaProgressEvent,
  Diagnostics,
  EngineStatus,
  ModelRecommendation,
  RuntimeKind,
  UninstallOutcome,
} from '../../../types/setup';

/**
 * Get the current system setup status.
 */
export async function getSetupStatus(): Promise<SetupStatus> {
  return get<SetupStatus>('/api/config/system/setup-status');
}


/**
 * Install `kind`'s pinned download, streaming install progress. It is also
 * the update: an install over an older engine replaces it. Refused while a
 * model runs on that engine, and where the install would be a source build,
 * which is the command line's.
 *
 * Both engines stream the same events under the same SSE names. Every payload
 * carries its own `type`, so the SSE event name is redundant and the parsed
 * object is the single source of truth.
 *
 * @param onEvent Called for every install event, in arrival order
 * @param onError Called when the transport itself fails
 * @param onClose Called when the stream closes cleanly, whether or not it reported a result
 * @returns A function that stops reading. The install continues on the daemon.
 */
export function streamEngineInstall(
  kind: RuntimeKind,
  onEvent: (event: LlamaProgressEvent) => void,
  onError: (error: string) => void,
  onClose?: () => void,
): () => void {
  return streamSse(`/api/config/system/engines/${kind}/install`, {
    onFrame: (frame) => {
      const event = parseFrame<LlamaProgressEvent>(frame);
      if (event) onEvent(event);
    },
    onClose,
    onError,
  });
}

/**
 * What `kind` has installed and how, where it stands against its pin, what
 * an install would download and what to say about it, and the model running
 * on it. Local and cheap; safe on mount.
 */
export async function getEngineStatus(kind: RuntimeKind): Promise<EngineStatus> {
  return get<EngineStatus>(`/api/config/system/engines/${kind}`);
}

/** Remove what `kind`'s install put there. Refused (409) while a model runs on it. */
export async function uninstallEngine(kind: RuntimeKind): Promise<UninstallOutcome> {
  return post<UninstallOutcome>(`/api/config/system/engines/${kind}/uninstall`);
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
