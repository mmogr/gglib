/**
 * Single agent card for council setup.
 *
 * Displays agent persona, perspective, a contentiousness slider, and a
 * collapsible tool-filter picker that matches CLI `tools <N>` parity.
 * Injects `--agent-color` CSS custom property for ambient tinting.
 *
 * @module components/Council/Setup/AgentCard
 */

import { type FC, useState, useCallback } from 'react';
import type { CouncilAgent } from '../../../types/council';
import { contentiousnessColor, contentiousnessLabel } from '../../../types/council';
import { cn } from '../../../utils/cn';
import { EditableTextField } from './EditableTextField';
import { AgentDiffBadge, type DiffStatus } from './AgentDiffBadge';
import { Sparkles } from 'lucide-react';
import { Icon } from '../../ui/Icon';

export interface AvailableTool {
  /** Sanitized name shown in the UI (e.g. `web_search`). */
  displayName: string;
  /** Wire name sent to the backend (e.g. `my_server:web_search` for MCP, or plain for built-ins). */
  backendName: string;
  /** LLM-facing description surfaced as tooltip/secondary text. */
  description: string;
}

interface AgentCardProps {
  agent: CouncilAgent;
  diffStatus?: DiffStatus;
  onContentiousnessChange?: (agentId: string, value: number) => void;
  onUpdate?: (agentId: string, changes: Partial<CouncilAgent>) => void;
  onRemove?: (agentId: string) => void;
  onFillAgent?: (agentId: string) => Promise<void>;
  /**
   * Full list of tools available to the backend.  When provided, renders a
   * collapsible tool-picker that maps to `CouncilAgent.tool_filter`.
   */
  availableTools?: AvailableTool[];
  disabled?: boolean;
}

export const AgentCard: FC<AgentCardProps> = ({
  agent, diffStatus = 'unchanged', onContentiousnessChange, onUpdate, onRemove, onFillAgent, availableTools, disabled,
}) => {
  const [isFilling, setIsFilling] = useState(false);
  const color = contentiousnessColor(agent.contentiousness);
  const label = contentiousnessLabel(agent.contentiousness);

  // ── Tool-filter helpers ────────────────────────────────────────────────────

  const isToolEnabled = useCallback(
    (backendName: string) =>
      agent.tool_filter === undefined || agent.tool_filter.includes(backendName),
    [agent.tool_filter],
  );

  const handleToolToggle = useCallback(
    (backendName: string, checked: boolean) => {
      if (!onUpdate || !availableTools) return;
      const allBackendNames = availableTools.map((t) => t.backendName);
      const current: string[] =
        agent.tool_filter ?? allBackendNames; // undefined → all enabled
      const next = checked
        ? [...current, backendName].filter((n, i, a) => a.indexOf(n) === i)
        : current.filter((n) => n !== backendName);
      // all checked → revert to undefined (no filter); keep explicit array otherwise
      const newFilter =
        next.length === allBackendNames.length ? undefined : next;
      onUpdate(agent.id, { tool_filter: newFilter });
    },
    [agent.id, agent.tool_filter, availableTools, onUpdate],
  );

  const handleSetAll = useCallback(() => {
    onUpdate?.(agent.id, { tool_filter: undefined });
  }, [agent.id, onUpdate]);

  const handleSetNone = useCallback(() => {
    onUpdate?.(agent.id, { tool_filter: [] });
  }, [agent.id, onUpdate]);

  // ── Tool-filter summary label ──────────────────────────────────────────────

  const toolSummary = (() => {
    if (!availableTools || availableTools.length === 0) return null;
    if (agent.tool_filter === undefined) return 'All tools';
    const count = agent.tool_filter.length;
    return `${count} of ${availableTools.length} tool${availableTools.length !== 1 ? 's' : ''}`;
  })();

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
        {onFillAgent && !disabled && (
          <button
            type="button"
            onClick={async () => {
              setIsFilling(true);
              try { await onFillAgent(agent.id); } finally { setIsFilling(false); }
            }}
            disabled={isFilling}
            className="text-text-muted hover:text-primary transition-colors p-0.5 disabled:pointer-events-none"
            aria-label={`AI-fill ${agent.name}`}
            title="Fill details with AI"
          >
            {isFilling ? (
              <span className="inline-block w-[14px] h-[14px] border-2 border-text-muted border-t-primary rounded-full animate-spin-360" />
            ) : (
              <Icon icon={Sparkles} size={14} />
            )}
          </button>
        )}
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

      {/* Tool filter (collapsed, only when tools are available) */}
      {availableTools && availableTools.length > 0 && (
        <details className="text-xs">
          <summary className="cursor-pointer text-text-secondary hover:text-text transition-colors flex items-center gap-xs">
            <span>Tools</span>
            <span
              className={cn(
                'ml-1 px-xs rounded text-[10px] font-medium',
                agent.tool_filter === undefined
                  ? 'bg-surface-alt text-text-muted'
                  : agent.tool_filter.length === 0
                    ? 'bg-danger/15 text-danger'
                    : 'bg-primary/15 text-primary',
              )}
            >
              {toolSummary}
            </span>
            {!disabled && onUpdate && (
              <span className="ml-auto flex gap-xs" onClick={(e) => e.preventDefault()}>
                <button
                  type="button"
                  onClick={handleSetAll}
                  className="text-text-muted hover:text-text transition-colors px-1"
                  title="Allow all tools"
                >
                  All
                </button>
                <button
                  type="button"
                  onClick={handleSetNone}
                  className="text-text-muted hover:text-text transition-colors px-1"
                  title="Disallow all tools"
                >
                  None
                </button>
              </span>
            )}
          </summary>
          <div className="mt-xs flex flex-col gap-xs pl-xs">
            {availableTools.map((tool) => (
              <label
                key={tool.backendName}
                className="flex items-start gap-sm cursor-pointer group"
              >
                <input
                  type="checkbox"
                  checked={isToolEnabled(tool.backendName)}
                  onChange={(e) => handleToolToggle(tool.backendName, e.target.checked)}
                  disabled={disabled || !onUpdate}
                  className="mt-0.5 shrink-0 accent-[var(--agent-color)] disabled:cursor-not-allowed"
                />
                <span className="flex flex-col">
                  <span className="text-text-secondary group-hover:text-text transition-colors">
                    {tool.displayName}
                  </span>
                  {tool.description && (
                    <span className="text-text-muted text-[10px] leading-snug">
                      {tool.description}
                    </span>
                  )}
                </span>
              </label>
            ))}
          </div>
        </details>
      )}

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
