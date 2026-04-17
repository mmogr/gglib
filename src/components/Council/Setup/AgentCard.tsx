/**
 * Single agent card for council setup.
 *
 * Displays agent persona, perspective, and a contentiousness slider.
 * Injects `--agent-color` CSS custom property for ambient tinting.
 *
 * @module components/Council/Setup/AgentCard
 */

import type { FC } from 'react';
import type { CouncilAgent } from '../../../types/council';
import { contentiousnessColor, contentiousnessLabel } from '../../../types/council';
import { cn } from '../../../utils/cn';
import { EditableTextField } from './EditableTextField';
import { AgentDiffBadge, type DiffStatus } from './AgentDiffBadge';

interface AgentCardProps {
  agent: CouncilAgent;
  diffStatus?: DiffStatus;
  onContentiousnessChange?: (agentId: string, value: number) => void;
  onUpdate?: (agentId: string, changes: Partial<CouncilAgent>) => void;
  onRemove?: (agentId: string) => void;
  disabled?: boolean;
}

export const AgentCard: FC<AgentCardProps> = ({
  agent, diffStatus = 'unchanged', onContentiousnessChange, onUpdate, onRemove, disabled,
}) => {
  const color = contentiousnessColor(agent.contentiousness);
  const label = contentiousnessLabel(agent.contentiousness);

  return (
    <div
      className={cn(
        'rounded-base border p-md flex flex-col gap-sm transition-colors duration-200',
        'bg-[color-mix(in_srgb,var(--agent-color)_8%,var(--color-surface))]',
        'border-[color-mix(in_srgb,var(--agent-color)_25%,var(--color-border))]',
      )}
      style={{ '--agent-color': color } as React.CSSProperties}
    >
      {/* Header: name + color dot + diff badge + delete */}
      <div className="flex items-center gap-sm">
        <span
          className="w-3 h-3 rounded-full shrink-0"
          style={{ backgroundColor: color }}
          aria-hidden
        />
        <EditableTextField
          value={agent.name}
          onChange={(v) => onUpdate?.(agent.id, { name: v })}
          disabled={disabled || !onUpdate}
          className="text-sm font-medium text-text"
          aria-label={`${agent.name} name`}
        />
        <AgentDiffBadge status={diffStatus} />
        <div className="ml-auto">
          {onRemove && !disabled && (
            <button
              type="button"
              onClick={() => onRemove(agent.id)}
              className="text-text-muted hover:text-danger transition-colors text-xs px-1"
              aria-label={`Remove ${agent.name}`}
            >
              &times;
            </button>
          )}
        </div>
      </div>

      {/* Perspective (editable one-liner) */}
      <EditableTextField
        value={agent.perspective}
        onChange={(v) => onUpdate?.(agent.id, { perspective: v })}
        disabled={disabled || !onUpdate}
        className="text-xs text-text-muted leading-relaxed"
        aria-label={`${agent.name} perspective`}
      />

      {/* Persona (collapsed, editable) */}
      <details className="text-xs">
        <summary className="cursor-pointer text-text-secondary hover:text-text transition-colors">
          Persona
        </summary>
        <EditableTextField
          value={agent.persona}
          onChange={(v) => onUpdate?.(agent.id, { persona: v })}
          disabled={disabled || !onUpdate}
          multiline
          className="mt-xs text-text-muted leading-relaxed"
          aria-label={`${agent.name} persona`}
        />
      </details>

      {/* Contentiousness slider */}
      <div className="flex flex-col gap-xs mt-xs">
        <div className="flex items-center justify-between">
          <span className="text-xs text-text-secondary">Contentiousness</span>
          <span
            className="text-xs font-medium px-xs rounded"
            style={{ color }}
          >
            {label} ({agent.contentiousness.toFixed(1)})
          </span>
        </div>
        <input
          type="range"
          min={0}
          max={1}
          step={0.1}
          value={agent.contentiousness}
          onChange={(e) => onContentiousnessChange?.(agent.id, parseFloat(e.target.value))}
          disabled={disabled}
          className="w-full accent-[var(--agent-color)] h-1.5 cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
          aria-label={`${agent.name} contentiousness`}
        />
      </div>
    </div>
  );
};
