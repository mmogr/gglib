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
