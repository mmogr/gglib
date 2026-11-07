/**
 * Proxy API module.
 * Handles multi-model proxy server management.
 */

import { get, post } from './client';
import type { ProxyConfig, ProxyStatus, StartPinnedRequest } from '../types/proxy';
import type { ClientRecipes } from '../../../types/generated/ClientRecipes';
import type { LoopGuardTripDay } from '../../../types/generated/LoopGuardTripDay';
import type { ProofOutcome } from '../../../types/generated/ProofOutcome';

/**
 * Get current proxy server status.
 */
export async function getProxyStatus(): Promise<ProxyStatus> {
  return get<ProxyStatus>('/api/proxy/status');
}

/**
 * The loop guard's log over the last `sinceDays` days, a day per row, newest
 * first. The daemon clamps the window to the 90 days the log keeps. Read from
 * the database, so it survives a restart.
 */
export async function getLoopGuardTrips(sinceDays: number): Promise<LoopGuardTripDay[]> {
  return get<LoopGuardTripDay[]>(`/api/proxy/loop-guard-trips?since_days=${sinceDays}`);
}

/**
 * Start the multi-model proxy server.
 */
export async function startProxy(config?: Partial<ProxyConfig>): Promise<ProxyStatus> {
  return post<ProxyStatus>('/api/proxy/start', config);
}

/**
 * Start the proxy pinned to one model. The daemon resolves the launch
 * cascade (sampling, MTP, context, cache) server-side, exactly as
 * `gglib serve` does.
 */
export async function startPinnedProxy(request: StartPinnedRequest): Promise<ProxyStatus> {
  return post<ProxyStatus>('/api/proxy/start-pinned', request);
}

/**
 * How a client connects to the running endpoint for model `modelId` (the
 * pin when `null`): the daemon's address, key rule, the id a client sends,
 * and a recipe per client, the block `gglib up` prints.
 */
export async function getProxyClients(modelId: number | null): Promise<ClientRecipes> {
  return get<ClientRecipes>(modelId === null ? '/api/proxy/clients' : `/api/proxy/clients?model=${modelId}`);
}

/**
 * Send one real request through the running endpoint for model `modelId`
 * (the pin when `null`), as `gglib proxy test` does: who answered and in how
 * long, or the daemon's refusal as the error. Held open while the model loads.
 */
export async function proveProxy(modelId: number | null): Promise<ProofOutcome> {
  return post<ProofOutcome>('/api/proxy/prove', { model_id: modelId });
}

/**
 * Stop the proxy server.
 */
export async function stopProxy(): Promise<void> {
  await post<void>('/api/proxy/stop', null);
}

/**
 * Shut the daemon down — the GUI face of `gglib daemon stop`.
 *
 * The daemon owns every running inference server, so this stops those too and
 * leaves the app without a backend until it is started again. The request may
 * not get a clean response: the server is shutting down as it replies, so a
 * transport error here often means it worked.
 */
export async function shutdownDaemon(): Promise<void> {
  return post<void>('/api/daemon/shutdown');
}
