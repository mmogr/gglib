/**
 * Proxy dashboard pieces, as the `/v1/proxy/status/stream` frame sends them.
 *
 * Nothing in `gglib-proxy` uses `skip_serializing_if`, so every field of every
 * snapshot is present on every frame and "not applicable" is a `null`. The
 * hand-written mirror these replace had thirty-two fields marked optional
 * that the proxy has never omitted, and the fixtures written against it were
 * naming three or four keys out of eleven.
 *
 * Builders rather than constants, because the interesting part of a dashboard
 * test is always one or two fields and the rest is noise.
 */
import type {
  ActiveConnectionSnapshot,
  SamplingAuditSnapshot,
  SlotSnapshot,
} from '../../../src/services/transport/types/dashboard';

const SLOT: SlotSnapshot = {
  id: 0,
  id_task: null,
  n_ctx: null,
  is_processing: false,
  params: null,
  n_past: null,
  cache_tokens: null,
  n_prompt_tokens: null,
  n_prompt_tokens_processed: null,
  n_prompt_tokens_cache: null,
  next_token: null,
};

/** An idle slot holding nothing, with `overrides` applied. */
export function slotSnapshot(overrides: Partial<SlotSnapshot> = {}): SlotSnapshot {
  return { ...SLOT, ...overrides };
}

const CONNECTION: ActiveConnectionSnapshot = {
  id: 'conn-1',
  model_name: 'test-model',
  started_at_secs: 0,
  is_streaming: false,
  num_ctx: null,
  phase: 'queued',
  prompt_processed: null,
  prompt_total: null,
  prompt_cached: null,
  prompt_time_ms: null,
};

/** A queued connection that has not begun prefill. */
export function activeConnection(
  overrides: Partial<ActiveConnectionSnapshot> = {},
): ActiveConnectionSnapshot {
  return { ...CONNECTION, ...overrides };
}

const AUDIT: SamplingAuditSnapshot = {
  state: { state: 'not_yet_observed' },
  skipped_ambiguous: 0,
  client_fields_rejected: 0,
  client_fields_discarded: 0,
  recent_divergences: [],
  baseline: { state: 'not_yet_read' },
  published: { intents: 0, fields: [] },
  effort_suppressed: { requests: 0, latest: null },
  reasoning: {
    effort_support: { state: 'not_yet_observed', reason: 'never launched' },
    latest: null,
    wire_blind_reason: '',
  },
  client_field_names: { fields: [], untracked: 0 },
};

/** An audit that has seen nothing yet — the state a fresh proxy reports. */
export function samplingAudit(
  overrides: Partial<SamplingAuditSnapshot> = {},
): SamplingAuditSnapshot {
  return { ...AUDIT, ...overrides };
}
