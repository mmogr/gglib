/**
 * Proxy dashboard pieces, as the `/v1/proxy/status/stream` frame sends them.
 *
 * Nothing on the dashboard contract uses `skip_serializing_if` — `gglib-proxy`
 * uses it elsewhere, on `models.rs` and the MCP types, but on nothing the
 * snapshot reaches — so every field of every snapshot is present on every
 * frame and "not applicable" is a `null`. The
 * hand-written mirror these replace had thirty-two fields marked optional
 * that the proxy has never omitted, and the fixtures written against it were
 * naming three or four keys out of eleven.
 *
 * Builders rather than constants, because the interesting part of a dashboard
 * test is always one or two fields and the rest is noise.
 */
import type {
  ActiveConnectionSnapshot,
  DashboardSnapshot,
  SamplingAuditSnapshot,
  SamplingReasoningReadback,
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

const READBACK: SamplingReasoningReadback = {
  effort_support: {
    state: 'not_yet_observed',
    reason: 'no /props read has completed for the running model yet',
  },
  latest: null,
  // The whole of `WIRE_BLIND_REASON`, not a stand-in: `ProxyReasoningRows`
  // renders it as the body of a warning banner, so a shortened copy lets a
  // test assert on text the user never sees.
  wire_blind_reason:
    'llama-server echoes neither reasoning control: reasoning_effort is a chat-template kwarg ' +
    'consumed at render time, and task_params::to_json serialises no reasoning_budget_* field ' +
    "(ADR 0007 finding 7a). These are gglib's own record of what it sent — no readback can " +
    'confirm them.',
};

/**
 * A readback from a proxy that has not seen a request yet.
 *
 * `wire_blind_reason` is the whole of `WIRE_BLIND_REASON` from
 * `audit_records.rs`, not a shortened stand-in: `ProxyReasoningRows` renders
 * it as the body of a warning banner, so a truncated copy lets a test assert
 * on text the user never sees.
 */
export function reasoningReadback(
  overrides: Partial<SamplingReasoningReadback> = {},
): SamplingReasoningReadback {
  return { ...READBACK, ...overrides };
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
  reasoning: READBACK,
  client_field_names: { fields: [], untracked: 0 },
};

/** An audit that has seen nothing yet — the state a fresh proxy reports. */
export function samplingAudit(
  overrides: Partial<SamplingAuditSnapshot> = {},
): SamplingAuditSnapshot {
  return { ...AUDIT, ...overrides };
}

const SNAPSHOT: DashboardSnapshot = {
  active_connections: [],
  slots_available: false,
  slots: [],
  // The poller's own default. `dashboard.rs` can emit only `null`
  // (slots available), the `--no-slots` line, or the poller's reason — the
  // 'Slot metrics unavailable.' string is the GUI's `??` fallback in
  // `ProxyMetricsGrid`, never something the proxy sends.
  slots_status: 'no /slots poll has completed yet',
  recent_requests: [],
  total_requests: 0,
  dialect_residue_total: 0,
  tool_repairs_attempted: 0,
  tool_repairs_succeeded: 0,
  upstream_health: {
    consecutive_strikes: 0,
    total_empty_responses: 0,
    total_upstream_errors: 0,
    total_first_byte_timeouts: 0,
    total_client_aborts: 0,
    total_recycles: 0,
    total_recycle_failures: 0,
  },
  per_model_defects: {},
  cache: null,
  agent_usage: {
    reporting_requests: 0,
    unreported_requests: 0,
    prompt_tokens: 0,
    cached_tokens: 0,
    last_prompt_tokens: null,
    last_cached_tokens: null,
  },
  launch: null,
  admission: {
    slots: [],
    queued: [],
    total_queued: 0,
    total_swaps: 0,
    secondary_slot: { state: 'available', detail: 'No second model has been requested yet.' },
  },
  sampling_audit: AUDIT,
};

/**
 * A whole frame from an idle proxy, with `overrides` applied.
 *
 * All sixteen fields, because that is what the proxy sends. The tests this
 * serves used to build one by naming five and casting — which typechecks,
 * leaves eleven fields `undefined` at runtime, and means a component reading
 * any of them is tested against a frame that cannot arrive.
 *
 * `slots_available: false` pairs with the "unavailable" `slots_status` on
 * purpose: `dashboard.rs` derives both from one `match`, so `true` alongside
 * that message is a combination the proxy cannot produce.
 */
export function dashboardSnapshot(overrides: Partial<DashboardSnapshot> = {}): DashboardSnapshot {
  return { ...SNAPSHOT, ...overrides };
}
