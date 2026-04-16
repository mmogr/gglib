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

interface AgentCardProps {
  agent: CouncilAgent;
  onContentiousnessChange?: (agentId: string, value: number) => void;
  disabled?: boolean;
}

export const AgentCard: FC<AgentCardProps> = ({ agent, onContentiousnessChange, disabled }) => {
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
      {/* Header: name + color dot */}
      <div className="flex items-center gap-sm">
        <span
          className="w-3 h-3 rounded-full shrink-0"
          style={{ backgroundColor: color }}
          aria-hidden
        />
        <h4 className="text-sm font-medium text-text m-0">{agent.name}</h4>
      </div>

      {/* Perspective (one-liner) */}
      <p className="text-xs text-text-muted m-0 leading-relaxed">{agent.perspective}</p>

      {/* Persona (collapsed by default to save space) */}
      <details className="text-xs">
        <summary className="cursor-pointer text-text-secondary hover:text-text transition-colors">
          Persona
        </summary>
        <p className="mt-xs text-text-muted leading-relaxed m-0">{agent.persona}</p>
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
