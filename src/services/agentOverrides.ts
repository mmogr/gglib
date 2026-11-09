/**
 * Client-persisted agent overrides: the Agent limits sheet's tool timeout,
 * parallel calls, observation steps and tools, reasoning effort and budget.
 *
 * One store for the whole client, not per conversation, in localStorage
 * (`gglib.chat.agentOverrides`, following the `gglib.<place>.<what>` key
 * convention). No turn carries them any more: a turn is its conversation
 * and its message, and the daemon runs it on the limits the conversation's
 * settings save. D6 (`feat(gui)/chat-settings-saved`) moves these controls
 * to saved settings and drops this store; until then the sheet keeps them
 * and nothing reads them.
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
   * setting the user chooses once — though it is not an `AgentConfig` field.
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
