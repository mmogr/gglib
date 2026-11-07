/**
 * Whether each of the Live tab's frame sections has anything to show, once
 * a frame has come. Pure: a frame in, yes or no out. A section that has nothing is not drawn; its
 * name joins the one line the tab says for all of them.
 *
 * @module places/diagnostics/live/sections
 */

import type { DashboardSnapshot } from '../../../services/transport/types/dashboard';

type Frame = DashboardSnapshot;

/** A request in flight, or one the proxy keeps. */
export const hasRequests = (s: Frame): boolean => s.active_connections.length > 0 || s.recent_requests.length > 0;

/**
 * A model loaded, a request waiting for one, a render on the GPU or waiting
 * for it, or a request admitted or a swap made since the endpoint turned on:
 * the counters stay after an unload, as the CLI prints them.
 */
export const hasLoaded = (s: Frame): boolean =>
  s.admission.slots.length > 0 ||
  s.admission.queued.length > 0 ||
  s.admission.generation.render !== null ||
  s.admission.generation.waiting > 0 ||
  s.admission.total_queued > 0 ||
  s.admission.total_swaps > 0;

/** The running model's launch, once a request has resolved one. */
export const hasLaunch = (s: Frame): boolean => s.launch !== null;

/** The prompt cache's configuration, once a request has resolved a model. */
export const hasPromptCache = (s: Frame): boolean => s.cache !== null;

/** A GUI-chat turn measured, or one that came back without the figures. */
export const hasAgentCache = (s: Frame): boolean => s.agent_usage.reporting_requests > 0 || s.agent_usage.unreported_requests > 0;

/**
 * Anything the readback has said: a comparison or the reason it cannot make
 * one (blind is shown, never folded), the build's defaults or why they could
 * not be read, a reasoning record, or client values dropped.
 */
export const hasSampling = (s: Frame): boolean => {
  const a = s.sampling_audit;
  return (
    a.state.state !== 'not_yet_observed' ||
    a.baseline.state !== 'not_yet_read' ||
    a.reasoning.latest !== null ||
    a.client_fields_discarded > 0 ||
    a.client_fields_rejected > 0
  );
};

/** A model the proxy has counted a request for. */
export const hasSignals = (s: Frame): boolean => Object.keys(s.per_model_defects).length > 0;

/** llama.cpp's slots, or, while a model is loaded, the proxy's reason it reports none. */
export const hasSlots = (s: Frame): boolean =>
  (s.slots_available && s.slots.length > 0) || (s.slots_status !== null && s.admission.slots.length > 0);
