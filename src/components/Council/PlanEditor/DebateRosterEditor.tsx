/**
 * DebateRosterEditor — right-pane editor for `debate`-kind task nodes.
 *
 * Rendered by PlanEditor when the selected node has `kind.debate`.
 * Lets the user configure the agent roster, debate rounds, and judge.
 * All changes are dispatched via `onApplyConfig` which triggers a
 * `set_debate_config` op in the undo stack.
 *
 * @module components/Council/PlanEditor/DebateRosterEditor
 */

import { type FC, useId, useState } from 'react';
import { Minus, Plus, Users } from 'lucide-react';
import { cn } from '../../../utils/cn';
import type { DebateAgent, DebateConfig } from '../../../types/council';

// ─── Props ────────────────────────────────────────────────────────────────────

export interface DebateRosterEditorProps {
  nodeId: string;
  config: DebateConfig;
  onApplyConfig: (nodeId: string, config: DebateConfig) => void;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const DEFAULT_COLORS = ['#4CAF50', '#2196F3', '#FF9800', '#9C27B0'];

function blankAgent(index: number): DebateAgent {
  return {
    id: `agent-${Date.now()}-${index}`,
    name: '',
    color: DEFAULT_COLORS[index % DEFAULT_COLORS.length],
    persona: '',
    perspective: '',
    contentiousness: 0.5,
  };
}

// ─── AgentRow ─────────────────────────────────────────────────────────────────

interface AgentRowProps {
  agent: DebateAgent;
  index: number;
  canRemove: boolean;
  onChange: (updated: DebateAgent) => void;
  onRemove: () => void;
}

const AgentRow: FC<AgentRowProps> = ({ agent, index, canRemove, onChange, onRemove }) => {
  const nameId = useId();
  const perspId = useId();
  const personaId = useId();
  const contId = useId();

  return (
    <div
      className="rounded-base border border-border bg-surface p-sm flex flex-col gap-xs"
      data-testid={`debate-agent-row-${index}`}
    >
      {/* Header row */}
      <div className="flex items-center gap-xs">
        {/* Color swatch */}
        <span
          className="w-3 h-3 rounded-full shrink-0 border border-border/50"
          style={{ backgroundColor: agent.color }}
          title={`Agent color: ${agent.color}`}
        />
        <input
          id={nameId}
          type="text"
          className="flex-1 text-sm font-medium bg-transparent border-b border-border focus:border-primary/50 focus:outline-none text-text placeholder:text-text-muted py-[2px] transition-colors"
          value={agent.name}
          onChange={e => onChange({ ...agent, name: e.target.value })}
          placeholder={`Agent ${index + 1} name`}
          aria-label={`Agent ${index + 1} name`}
          data-testid={`debate-agent-name-${index}`}
        />
        {canRemove && (
          <button
            type="button"
            onClick={onRemove}
            className="text-danger/60 hover:text-danger transition-colors shrink-0"
            aria-label={`Remove agent ${index + 1}`}
            data-testid={`debate-agent-remove-${index}`}
          >
            <Minus className="w-3 h-3" />
          </button>
        )}
      </div>

      {/* Perspective */}
      <div className="flex flex-col gap-[2px]">
        <label htmlFor={perspId} className="text-xs text-text-muted font-medium">
          Perspective
        </label>
        <input
          id={perspId}
          type="text"
          className="text-xs bg-surface border border-border rounded-sm px-xs py-[3px] text-text placeholder:text-text-muted focus:outline-none focus:border-primary/50 transition-colors"
          value={agent.perspective}
          onChange={e => onChange({ ...agent, perspective: e.target.value })}
          placeholder="The position this agent argues…"
          data-testid={`debate-agent-perspective-${index}`}
        />
      </div>

      {/* Persona */}
      <div className="flex flex-col gap-[2px]">
        <label htmlFor={personaId} className="text-xs text-text-muted font-medium">
          Persona
          <span className="ml-xs font-normal">(optional)</span>
        </label>
        <input
          id={personaId}
          type="text"
          className="text-xs bg-surface border border-border rounded-sm px-xs py-[3px] text-text placeholder:text-text-muted focus:outline-none focus:border-primary/50 transition-colors"
          value={agent.persona}
          onChange={e => onChange({ ...agent, persona: e.target.value })}
          placeholder="Socratic, Devil's advocate…"
          data-testid={`debate-agent-persona-${index}`}
        />
      </div>

      {/* Contentiousness slider */}
      <div className="flex flex-col gap-[2px]">
        <label htmlFor={contId} className="text-xs text-text-muted font-medium flex items-center justify-between">
          <span>Contentiousness</span>
          <span className="font-mono text-text-secondary">{agent.contentiousness.toFixed(2)}</span>
        </label>
        <input
          id={contId}
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={agent.contentiousness}
          onChange={e => onChange({ ...agent, contentiousness: parseFloat(e.target.value) })}
          className="accent-primary w-full h-1 cursor-pointer"
          aria-label={`Agent ${index + 1} contentiousness`}
          data-testid={`debate-agent-cont-${index}`}
        />
        <div className="flex justify-between text-[10px] text-text-muted">
          <span>Agreeable</span>
          <span>Combative</span>
        </div>
      </div>
    </div>
  );
};

// ─── DebateRosterEditor ───────────────────────────────────────────────────────

const DebateRosterEditor: FC<DebateRosterEditorProps> = ({ nodeId, config, onApplyConfig }) => {
  const [draft, setDraft] = useState<DebateConfig>(config);
  const roundsId = useId();
  const judgeId = useId();

  const commit = (next: DebateConfig) => {
    setDraft(next);
    onApplyConfig(nodeId, next);
  };

  const handleAgentChange = (index: number, updated: DebateAgent) => {
    const agents = [...draft.agents];
    agents[index] = updated;
    commit({ ...draft, agents });
  };

  const handleAddAgent = () => {
    if (draft.agents.length >= 4) return;
    const agents = [...draft.agents, blankAgent(draft.agents.length)];
    commit({ ...draft, agents });
  };

  const handleRemoveAgent = (index: number) => {
    if (draft.agents.length <= 2) return;
    const agents = draft.agents.filter((_, i) => i !== index);
    commit({ ...draft, agents });
  };

  const handleRoundsChange = (value: number) => {
    const rounds = Math.min(3, Math.max(1, value));
    commit({ ...draft, rounds });
  };

  const handleJudgeToggle = (checked: boolean) => {
    commit({
      ...draft,
      judge: checked ? { min_rounds_before_stop: 1 } : null,
    });
  };

  return (
    <div
      className="flex flex-col gap-sm p-sm overflow-y-auto scrollbar-thin flex-1 min-h-0"
      data-testid="debate-roster-editor"
    >
      {/* Header */}
      <div className="flex items-center gap-xs text-xs font-medium text-text-secondary">
        <Users className="w-3 h-3" />
        Debate configuration
      </div>

      {/* Rounds */}
      <div className="flex items-center gap-sm">
        <label htmlFor={roundsId} className="text-xs font-medium text-text-secondary shrink-0">
          Rounds
        </label>
        <div className="flex items-center gap-xs ml-auto">
          <button
            type="button"
            onClick={() => handleRoundsChange(draft.rounds - 1)}
            disabled={draft.rounds <= 1}
            className="w-5 h-5 flex items-center justify-center rounded-sm border border-border text-text-secondary hover:text-text hover:bg-surface-hover disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            aria-label="Decrease rounds"
          >
            <Minus className="w-3 h-3" />
          </button>
          <span
            id={roundsId}
            className="text-sm font-semibold text-text w-4 text-center tabular-nums"
            aria-live="polite"
            aria-label={`${draft.rounds} rounds`}
          >
            {draft.rounds}
          </span>
          <button
            type="button"
            onClick={() => handleRoundsChange(draft.rounds + 1)}
            disabled={draft.rounds >= 3}
            className="w-5 h-5 flex items-center justify-center rounded-sm border border-border text-text-secondary hover:text-text hover:bg-surface-hover disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            aria-label="Increase rounds"
          >
            <Plus className="w-3 h-3" />
          </button>
          <span className="text-xs text-text-muted">(1–3)</span>
        </div>
      </div>

      {/* Judge toggle */}
      <label className="flex items-center gap-sm cursor-pointer group" htmlFor={judgeId}>
        <input
          id={judgeId}
          type="checkbox"
          className="accent-primary"
          checked={!!draft.judge}
          onChange={e => handleJudgeToggle(e.target.checked)}
          data-testid="debate-judge-toggle"
        />
        <span className="text-xs font-medium text-text-secondary group-hover:text-text transition-colors">
          Enable judge
        </span>
        <span className="text-xs text-text-muted font-normal">
          (LLM evaluates each round; may stop early)
        </span>
      </label>

      {/* Agents */}
      <div className="flex flex-col gap-xs">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-text-secondary">
            Agents
            <span className="ml-xs text-text-muted font-normal">{draft.agents.length}/4</span>
          </span>
          <button
            type="button"
            onClick={handleAddAgent}
            disabled={draft.agents.length >= 4}
            className={cn(
              'flex items-center gap-xs text-xs px-xs py-[3px] rounded-sm border transition-colors',
              'border-dashed border-border text-text-secondary',
              'hover:border-primary/40 hover:text-primary hover:bg-primary/5',
              'disabled:opacity-40 disabled:cursor-not-allowed',
            )}
            data-testid="debate-add-agent-btn"
          >
            <Plus className="w-3 h-3" />
            Add agent
          </button>
        </div>

        {draft.agents.map((agent, i) => (
          <AgentRow
            key={agent.id}
            agent={agent}
            index={i}
            canRemove={draft.agents.length > 2}
            onChange={updated => handleAgentChange(i, updated)}
            onRemove={() => handleRemoveAgent(i)}
          />
        ))}
      </div>
    </div>
  );
};

export default DebateRosterEditor;
