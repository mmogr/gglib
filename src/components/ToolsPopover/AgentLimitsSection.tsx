/**
 * Agent limits inside the Tools popover — the GUI surface for
 * `AgentRequestConfig`'s BYO-MCP knobs. Values persist client-side
 * (`services/agentOverrides`), apply to every chat sent from this client,
 * and are read fresh at each send.
 */

import { FC, useState } from 'react';
import { Input } from '../ui/Input';
import { Checkbox } from '../ui/Checkbox';
import {
  MAX_OBSERVATION_STEPS_CEILING,
  MAX_PARALLEL_TOOLS_CEILING,
  TOOL_TIMEOUT_MS_CEILING,
  TOOL_TIMEOUT_MS_FLOOR,
  readStoredAgentOverrides,
  writeStoredAgentOverrides,
  type StoredAgentOverrides,
} from '../../services/agentOverrides';

interface NumberRowProps {
  id: string;
  label: string;
  placeholder: string;
  min: number;
  max: number;
  value: number | undefined;
  onChange: (value: number | undefined) => void;
}

const NumberRow: FC<NumberRowProps> = ({ id, label, placeholder, min, max, value, onChange }) => (
  <div className="flex items-center justify-between gap-sm">
    <label htmlFor={id} className="text-xs text-text-secondary">
      {label}
    </label>
    <Input
      id={id}
      type="number"
      size="sm"
      min={min}
      max={max}
      className="w-24 font-mono tabular-nums"
      value={value ?? ''}
      placeholder={placeholder}
      onChange={(e) => {
        const parsed = parseInt(e.target.value, 10);
        onChange(
          Number.isFinite(parsed) && parsed > 0
            ? Math.min(Math.max(parsed, min), max)
            : undefined,
        );
      }}
    />
  </div>
);

const clamp = (value: number | undefined, min: number, max: number) =>
  value === undefined ? undefined : Math.min(Math.max(value, min), max);

/** Stale stored values are clamped on seed so the UI never shows what the wire won't send. */
function seedOverrides(): StoredAgentOverrides {
  const stored = readStoredAgentOverrides();
  return {
    ...stored,
    toolTimeoutMs: clamp(stored.toolTimeoutMs, TOOL_TIMEOUT_MS_FLOOR, TOOL_TIMEOUT_MS_CEILING),
    maxParallelTools: clamp(stored.maxParallelTools, 1, MAX_PARALLEL_TOOLS_CEILING),
    maxObservationSteps: clamp(stored.maxObservationSteps, 1, MAX_OBSERVATION_STEPS_CEILING),
  };
}

export const AgentLimitsSection: FC = () => {
  const [overrides, setOverrides] = useState<StoredAgentOverrides>(seedOverrides);
  const classificationDisabled = overrides.observationTools?.length === 0;
  const [observationText, setObservationText] = useState(() => {
    const tools = seedOverrides().observationTools;
    return tools && tools.length > 0 ? tools.join(', ') : '';
  });

  const update = (partial: Partial<StoredAgentOverrides>) => {
    const next = { ...overrides, ...partial };
    setOverrides(next);
    writeStoredAgentOverrides(next);
  };

  const parseTools = (raw: string): string[] | undefined => {
    const tools = raw
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean);
    return tools.length > 0 ? tools : undefined;
  };

  return (
    <div className="px-[14px] py-[10px] border-t border-border-light flex flex-col gap-sm">
      <div className="flex items-baseline justify-between gap-sm">
        <span className="text-xs font-semibold text-text">Agent limits</span>
        <span className="text-2xs text-text-muted">applies to all chats on this device</span>
      </div>
      <NumberRow
        id="agent-tool-timeout"
        label="Tool timeout (ms)"
        placeholder="30000"
        min={TOOL_TIMEOUT_MS_FLOOR}
        max={TOOL_TIMEOUT_MS_CEILING}
        value={overrides.toolTimeoutMs}
        onChange={(v) => update({ toolTimeoutMs: v })}
      />
      <NumberRow
        id="agent-max-parallel"
        label="Parallel tool calls"
        placeholder="25"
        min={1}
        max={MAX_PARALLEL_TOOLS_CEILING}
        value={overrides.maxParallelTools}
        onChange={(v) => update({ maxParallelTools: v })}
      />
      <NumberRow
        id="agent-max-observation"
        label="Observation steps"
        placeholder="15"
        min={1}
        max={MAX_OBSERVATION_STEPS_CEILING}
        value={overrides.maxObservationSteps}
        onChange={(v) => update({ maxObservationSteps: v })}
      />
      <Checkbox
        checked={classificationDisabled}
        onChange={(e) =>
          update({
            observationTools: e.target.checked ? [] : parseTools(observationText),
          })
        }
        label={
          <span className="text-xs text-text-secondary">Disable observation classification</span>
        }
      />
      <div className="flex flex-col gap-xs">
        <label htmlFor="agent-observation-tools" className="text-xs text-text-secondary">
          Observation tools
        </label>
        <Input
          id="agent-observation-tools"
          type="text"
          size="sm"
          className="font-mono"
          disabled={classificationDisabled}
          value={observationText}
          placeholder="built-ins (snapshot, screenshot, …)"
          onChange={(e) => {
            setObservationText(e.target.value);
            update({ observationTools: parseTools(e.target.value) });
          }}
        />
        <p className="m-0 text-2xs text-text-muted">
          Comma-separated tool names counted as observations. Blank uses the built-ins.
        </p>
      </div>
    </div>
  );
};
