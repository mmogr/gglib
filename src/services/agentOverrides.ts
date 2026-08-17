/**
 * Client-persisted agent overrides, applied to every chat sent from this
 * client — the GUI counterpart of
 * `gglib chat`'s `--tool-timeout-ms`, `--max-parallel`, `--observation-tool`,
 * and `--max-observation-steps` flags.
 *
 * One store for the whole client, not per conversation. Stored in
 * localStorage (`gglib.chat.agentOverrides`, following the
 * `usePanelResize` key convention) rather than server settings: these are
 * request-scoped knobs on `AgentRequestConfig`, not persisted server state.
 * The runtime reads them fresh at send time via `readStoredAgentOverrides`,
 * so the popover UI and the request path cannot drift within a session.
 */

import type { ReasoningEffortLevel } from '../constants/reasoningEffort';

const STORAGE_KEY = 'gglib.chat.agentOverrides';

/** Bounds mirror `crates/gglib-core/src/domain/agent/config.rs`. */
export const TOOL_TIMEOUT_MS_FLOOR = 100;
export const TOOL_TIMEOUT_MS_CEILING = 60_000;
export const MAX_PARALLEL_TOOLS_CEILING = 50;
export const MAX_OBSERVATION_STEPS_CEILING = 100;

export interface StoredAgentOverrides {
  /** Per-tool-call timeout in ms (server default 30 000, ceiling 60 000). */
  toolTimeoutMs?: number;
  /** Parallel tool-call cap (server default 25, ceiling 50). */
  maxParallelTools?: number;
  /** Observation-classification step cap (server default 15, ceiling 100). */
  maxObservationSteps?: number;
  /**
   * Tool names classified as observations. Absent = the server's built-ins;
   * an empty array disables classification entirely.
   */
  observationTools?: string[];
  /**
   * Reasoning effort asked of the chat template, where it reads the variable.
   *
   * Stored beside the agent knobs because it has the same lifetime — a chat
   * setting the user chooses once — but it is **not** an `AgentConfig` field
   * and does not travel in `config`. `POST /api/agent/chat` takes both
   * reasoning controls at the top level of the body, which is why
   * {@link reasoningOverridesToWire} exists separately from
   * {@link agentOverridesToWire}. Putting them in `config` would have sent two
   * keys `AgentRequestConfig` does not declare, and serde would have dropped
   * them without a word — the exact silent-discard failure this arc is about.
   */
  reasoningEffort?: ReasoningEffortLevel;
  /** Thinking-token ceiling. `-1` defers to the launch default; `0` stops thinking. */
  reasoningBudgetTokens?: number;
}

export function readStoredAgentOverrides(): StoredAgentOverrides {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as StoredAgentOverrides;
    return typeof parsed === 'object' && parsed !== null ? parsed : {};
  } catch {
    return {};
  }
}

export function writeStoredAgentOverrides(overrides: StoredAgentOverrides): void {
  try {
    const entries = Object.entries(overrides).filter(([, v]) => v !== undefined);
    if (entries.length === 0) {
      localStorage.removeItem(STORAGE_KEY);
    } else {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(Object.fromEntries(entries)));
    }
  } catch {
    // Storage unavailable (private mode) — overrides just don't persist.
  }
}

/**
 * The stored overrides as `AgentRequestConfig` wire fields (snake_case),
 * clamped to the server's ceilings so a stale stored value cannot 4xx.
 */
export function agentOverridesToWire(): {
  tool_timeout_ms?: number;
  max_parallel_tools?: number;
  max_observation_steps?: number;
  observation_tools?: string[];
  /**
   * Never carries the reasoning keys. Stated as a type rather than a comment
   * so that spreading this into `config` cannot start smuggling them there.
   */
  reasoning_effort?: never;
  reasoning_budget_tokens?: never;
} {
  const stored = readStoredAgentOverrides();
  return {
    ...(stored.toolTimeoutMs !== undefined && {
      tool_timeout_ms: Math.min(
        Math.max(stored.toolTimeoutMs, TOOL_TIMEOUT_MS_FLOOR),
        TOOL_TIMEOUT_MS_CEILING,
      ),
    }),
    ...(stored.maxParallelTools !== undefined && {
      max_parallel_tools: Math.min(stored.maxParallelTools, MAX_PARALLEL_TOOLS_CEILING),
    }),
    ...(stored.maxObservationSteps !== undefined && {
      max_observation_steps: Math.min(stored.maxObservationSteps, MAX_OBSERVATION_STEPS_CEILING),
    }),
    ...(stored.observationTools !== undefined && { observation_tools: stored.observationTools }),
  };
}

/**
 * The stored reasoning controls as top-level `POST /api/agent/chat` fields.
 *
 * Separate from {@link agentOverridesToWire} because they sit in a different
 * part of the body, not because they have a different lifetime — see
 * {@link StoredAgentOverrides.reasoningEffort}.
 *
 * Only the budget is clamped, and only at the bottom: `-1` is the floor the
 * server validates against, and a stored `-2` would 400 every chat. There is
 * no ceiling to clamp to — the backend's is `i32::MAX` — and no clamp at all
 * for the effort, which is an enum the server rejects loudly rather than a
 * number it silently mishandles.
 */
export function reasoningOverridesToWire(): {
  reasoning_effort?: ReasoningEffortLevel;
  reasoning_budget_tokens?: number;
} {
  const stored = readStoredAgentOverrides();
  return {
    ...(stored.reasoningEffort !== undefined && { reasoning_effort: stored.reasoningEffort }),
    ...(stored.reasoningBudgetTokens !== undefined && {
      reasoning_budget_tokens: Math.max(stored.reasoningBudgetTokens, -1),
    }),
  };
}
