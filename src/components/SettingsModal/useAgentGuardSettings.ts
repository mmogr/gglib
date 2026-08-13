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

export interface AgentGuardSettingsValues {
  /** Inverse polarity on the wire: unset means enabled. */
  agenticSampling: boolean;
  /** Inverse polarity on the wire: unset means enabled. */
  toolCallRepair: boolean;
  /** Raw input string; blank = server default. */
  maxStagnationSteps: string;
}

const DEFAULTS: AgentGuardSettingsValues = {
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
  updates: Pick<UpdateSettingsRequest, 'agenticSampling' | 'toolCallRepair' | 'maxStagnationSteps'>;
}

/** Track the agent-guard fields, seeded from persisted settings. */
export function useAgentGuardSettings(settings: AppSettings | null): UseAgentGuardSettingsResult {
  const [values, setValues] = useState<AgentGuardSettingsValues>(DEFAULTS);

  useEffect(() => {
    if (settings) {
      setValues({
        // Unset means enabled, like proxyLoopDetection.
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
      agenticSampling: values.agenticSampling,
      toolCallRepair: values.toolCallRepair,
      maxStagnationSteps: Number.isFinite(parsed) ? parsed : null,
    },
  };
}
