/**
 * Agent limits inside the Tools popover — the GUI surface for
 * `AgentRequestConfig`'s BYO-MCP knobs. Values persist client-side
 * (`services/agentOverrides`), apply to every chat sent from this client,
 * and are read fresh at each send.
 */

import { FC, useState } from 'react';
import { Input } from '../ui/Input';
import { Checkbox } from '../ui/Checkbox';
import { Select } from '../ui/Select';
import { INFERENCE_PARAMS } from '../../constants/inferenceDefaults';
import {
  REASONING_EFFORT_LEVELS,
  type ReasoningEffortLevel,
} from '../../constants/reasoningEffort';
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

const clamp = (value: number | undefined, min: number, max: number) =>
  value === undefined ? undefined : Math.min(Math.max(value, min), max);

/**
 * A bounded integer field that is bounded on commit, never on keystroke.
 *
 * The input is controlled by `value`, so a keystroke this handler answers with
 * `undefined` re-renders the field empty and destroys what was typed. That
 * makes any bound applied here a bound on *prefixes*, not on answers: reaching
 * 30000 in the row floored at 100 means passing through "3" and "30", so a
 * `parsed >= min` test does not make that row strict, it makes it impossible to
 * type into. The same test is invisible on the rows floored at 1, whose every
 * prefix is already in range — which is exactly why it can be introduced
 * without anyone noticing.
 *
 * So: accept any integer while typing and correct it on blur, where the number
 * is finished and clamping it can only mean the user's own value was out of
 * range. `parseInt('')` is `NaN`, which is how clearing the field still clears
 * the override — and it is the reason a plain positivity test was wrong for the
 * reasoning budget, whose -1 ("defer to the launch default") and 0 ("stop
 * thinking") are values rather than absences.
 */
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
        onChange(Number.isFinite(parsed) ? parsed : undefined);
      }}
      onBlur={() => {
        const bounded = clamp(value, min, max);
        if (bounded !== value) onChange(bounded);
      }}
    />
  </div>
);

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
      {/*
        Not an agent limit, and shown here anyway: this is the popover for
        "settings that apply to the chats I send", and a per-turn reasoning
        level is one. It reaches the request as a top-level field rather than
        through `AgentConfig` — see `reasoningOverridesToWire`.
      */}
      <div className="flex items-center justify-between gap-sm">
        <label htmlFor="chat-reasoning-effort" className="text-xs text-text-secondary">
          Reasoning effort
        </label>
        <Select
          id="chat-reasoning-effort"
          size="sm"
          className="w-32"
          value={overrides.reasoningEffort ?? ''}
          onChange={(e) =>
            update({
              reasoningEffort: (e.target.value || undefined) as ReasoningEffortLevel | undefined,
            })
          }
        >
          {/* Blank sends no key, leaving the model's own resolved level in place. */}
          <option value="">model default</option>
          {REASONING_EFFORT_LEVELS.map((level) => (
            <option key={level} value={level}>
              {level}
            </option>
          ))}
        </Select>
      </div>
      <NumberRow
        id="chat-reasoning-budget"
        label="Reasoning budget"
        placeholder="model default"
        // Borrowed from the settings surface rather than restated, so the two
        // places a budget can be typed offer the same range. Note the floor is
        // below zero — -1 is a legal value ("defer to the launch default"),
        // unlike every limit above this row.
        min={INFERENCE_PARAMS.reasoningBudgetTokens.min}
        max={INFERENCE_PARAMS.reasoningBudgetTokens.max}
        value={overrides.reasoningBudgetTokens}
        onChange={(v) => update({ reasoningBudgetTokens: v })}
      />
      <p className="m-0 text-2xs text-text-muted">
        The budget is a hard cap llama.cpp enforces on any model. The effort level is only a
        request to the chat template — a model whose template does not read it is unaffected, and
        the model inspector says which models those are.
      </p>
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
          placeholder="built-ins (read_file, grep_search, snapshot, …)"
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
