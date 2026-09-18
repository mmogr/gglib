/**
 * State for the agent-guard settings (agentic sampling cap, stagnation limit).
 *
 * Same rationale as `useDesktopSettings`: the group owns its own state and
 * hands back a ready-made slice of the update request.
 *
 * @module components/SettingsModal/useAgentGuardSettings
 */

import { useCallback, useEffect, useState } from 'react';
import type { AppSettings, UpdateSettingsRequest } from '../../types';
import type { LoopGuardMode } from '../../types/generated/LoopGuardMode';

export interface AgentGuardSettingsValues {
  /** What the proxy's loop guard does with a tripped history. */
  loopGuardMode: LoopGuardMode;
  /** Inverse polarity on the wire: unset means enabled. */
  agenticSampling: boolean;
  /** Inverse polarity on the wire: unset means enabled. */
  toolCallRepair: boolean;
  /** Raw input string; blank = server default. */
  maxStagnationSteps: string;
}

const DEFAULTS: AgentGuardSettingsValues = {
  loopGuardMode: 'note',
  agenticSampling: true,
  toolCallRepair: true,
  maxStagnationSteps: '',
};

export interface UseAgentGuardSettingsResult {
  values: AgentGuardSettingsValues;
  /** Restore the built-in defaults, for the form's Reset action. */
  reset: () => void;
  setValue: <K extends keyof AgentGuardSettingsValues>(
    key: K,
    value: AgentGuardSettingsValues[K],
  ) => void;
  updates: Pick<
    UpdateSettingsRequest,
    'loopGuardMode' | 'agenticSampling' | 'toolCallRepair' | 'maxStagnationSteps'
  >;
}

/** Track the agent-guard fields, seeded from persisted settings. */
export function useAgentGuardSettings(settings: AppSettings | null): UseAgentGuardSettingsResult {
  const [values, setValues] = useState<AgentGuardSettingsValues>(DEFAULTS);

  useEffect(() => {
    if (settings) {
      setValues({
        // Unset means the default, which is to note rather than refuse.
        loopGuardMode: settings.loopGuardMode ?? 'note',
        // Unset means enabled, the same absent-means-protective rule.
        agenticSampling: settings.agenticSampling !== false,
        toolCallRepair: settings.toolCallRepair !== false,
        maxStagnationSteps: settings.maxStagnationSteps?.toString() ?? '',
      });
    }
  }, [settings]);

  const reset = useCallback(() => setValues(DEFAULTS), []);

  const setValue = useCallback(
    <K extends keyof AgentGuardSettingsValues>(key: K, value: AgentGuardSettingsValues[K]) => {
      setValues((previous) => ({ ...previous, [key]: value }));
    },
    [],
  );

  const parsed = parseInt(values.maxStagnationSteps.trim(), 10);

  return {
    values,
    setValue,
    reset,
    updates: {
      loopGuardMode: values.loopGuardMode,
      agenticSampling: values.agenticSampling,
      toolCallRepair: values.toolCallRepair,
      maxStagnationSteps: Number.isFinite(parsed) ? parsed : null,
    },
  };
}
