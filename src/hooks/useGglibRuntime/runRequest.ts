/**
 * The body of an agent run, `PUT /api/runs/{id}?kind=agent`: the history in
 * wire form, the conversation the daemon saves it to, and where it is sent.
 *
 * @module runRequest
 */

import { getToolRegistry } from '../../services/tools';
import type { GglibMessage } from '../../types/messages';
import type { AgentMessage } from '../../types/generated/AgentMessage';
import type { AgentRequestConfig } from '../../types/generated/AgentRequestConfig';
import type { AgentRunRequest } from '../../types/generated/AgentRunRequest';
import type { ReasoningEffort } from '../../types/generated/ReasoningEffort';
import { convertToWireMessages } from './wireMessages';

/**
 * Partial `AgentConfig` forwarded to the backend.
 *
 * Only includes fields exposed by `AgentRequestConfig` in `gglib-axum`.
 * Internal tuning parameters (`max_stagnation_steps`, `context_budget_chars`,
 * `max_repeated_batch_steps`, `prune_*`) are intentionally absent from the
 * backend DTO to prevent resource exhaustion by untrusted callers.
 *
 * All fields are optional; omitted fields use the backend's
 * `AgentConfig::default()` values.
 */
export interface PartialAgentConfig {
  /** Maps to `AgentConfig::max_iterations` (default 25). */
  max_iterations?: number;
  /** Maps to `AgentConfig::max_parallel_tools` (default 25). */
  max_parallel_tools?: number;
  /** Maps to `AgentConfig::tool_timeout_ms` (default 30 000). */
  tool_timeout_ms?: number;
  /** Tool names classified as observations; `[]` disables classification. */
  observation_tools?: string[];
  /** Maps to `AgentConfig::max_observation_steps` (default 15). */
  max_observation_steps?: number;
}

export interface RunRequestOptions {
  /** The history the run answers, its last message the user's. */
  messages: GglibMessage[];
  /** The conversation the daemon saves the user's message and the reply to. */
  conversationId: number;
  /**
   * The local server this turn is for. Absent for a remote turn, which has
   * none: the body still carries a `port` because the wire type requires a
   * number, and the backend does not consult it on that branch.
   */
  selectedServerPort?: number;
  /** Optional partial `AgentConfig` overrides; omitted fields use backend defaults. */
  config?: PartialAgentConfig;
  /**
   * The two reasoning controls, which the request takes at the **top level**
   * rather than inside `config` — they are per-turn shape, not agent-loop
   * tuning. An omitted field resolves from the profile, per-model, global and
   * floor layers.
   */
  reasoning?: { reasoning_effort?: string; reasoning_budget_tokens?: number };
  /** `false` exposes no tools (an empty `tool_filter`); otherwise permissive. */
  supportsToolCalls?: boolean | null;
  /**
   * Send this turn to the machine on the other end of the remote tunnel
   * (ADR 0012) instead of the server on `selectedServerPort`.
   */
  remote?: boolean;
  /**
   * The model name, spelled as the machine that serves it spells it. Omitted
   * locally, which lets llama-server serve whatever it loaded. **Required
   * with `remote`**: this machine's default is deliberately not substituted,
   * because the far machine may not have it (`docs/remote.md`).
   */
  model?: string;
}

/** A run id the hub accepts: 1–64 of `[A-Za-z0-9_-]`. */
export function mintRunId(): string {
  return `chat-${crypto.randomUUID()}`;
}

/** The config to send: `null` unless at least one field was set. */
function wireConfig(config: PartialAgentConfig | undefined): AgentRequestConfig | null {
  if (!config) return null;
  const defined = Object.fromEntries(Object.entries(config).filter(([, v]) => v !== undefined));
  if (Object.keys(defined).length === 0) return null;
  return {
    max_iterations: null,
    max_parallel_tools: null,
    tool_timeout_ms: null,
    observation_tools: null,
    max_observation_steps: null,
    ...defined,
  };
}

/**
 * The enabled tools in the backend's qualified form (`serverId:name`); `[]`
 * when the model is known not to call tools; `null` (no filter) when the
 * registry has none enabled.
 */
function toolFilter(supportsToolCalls: boolean | null | undefined): string[] | null {
  if (supportsToolCalls === false) return [];
  const registry = getToolRegistry();
  const enabled = registry.getEnabledDefinitions();
  if (enabled.length === 0) return null;
  return enabled.map((def) => registry.getBackendName(def.function.name));
}

/**
 * Build the run's body.
 *
 * @throws when the turn is for the far machine and names no model there,
 *   before anything is sent. Sent empty, the name reaches the far proxy as
 *   `"model": ""` and comes back `404 Model '' not found`: a real answer
 *   through a working tunnel that reads as transport.
 */
export function buildRunRequest(options: RunRequestOptions): AgentRunRequest {
  const { remote = false } = options;
  const model = options.model?.trim() || null;
  if (remote && !model) {
    throw new Error(
      'Chat is set to go to the other machine, but no model there is named. ' +
        'Name one in the Remote panel — this machine’s default is not sent, ' +
        'because the other machine may not have it.',
    );
  }
  return {
    conversation_id: options.conversationId,
    port: options.selectedServerPort ?? 0,
    remote,
    messages: convertToWireMessages(options.messages) as AgentMessage[],
    config: wireConfig(options.config),
    tool_filter: toolFilter(options.supportsToolCalls),
    model,
    reasoning_effort: (options.reasoning?.reasoning_effort as ReasoningEffort | undefined) ?? null,
    reasoning_budget_tokens: options.reasoning?.reasoning_budget_tokens ?? null,
  };
}
