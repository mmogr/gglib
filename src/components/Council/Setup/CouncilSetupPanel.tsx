/**
 * Council setup panel — displayed inline in the chat column before
 * deliberation starts.
 *
 * Renders the suggested agent cards with editable contentiousness,
 * a topic display, and the "Run Council" action.
 *
 * @module components/Council/Setup/CouncilSetupPanel
 */

import { type FC, useState, useCallback, useEffect, useMemo } from 'react';
import type { CouncilAgent, CouncilConfig } from '../../../types/council';
import { Button } from '../../ui/Button';
import { AgentCard } from './AgentCard';
import { AddAgentButton } from './AddAgentButton';
import type { DiffStatus } from './AgentDiffBadge';
import { cn } from '../../../utils/cn';
import { getToolRegistry } from '../../../services/tools/registry';

interface CouncilSetupPanelProps {
  topic: string;
  agents: CouncilAgent[];
  rounds: number;
  synthesisGuidance?: string;
  /** Per-agent diff status after a refinement. Keyed by agent id. */
  diffStatuses?: Record<string, DiffStatus>;
  onRun: (config: CouncilConfig) => void;
  onCancel: () => void;
  onUpdateAgent?: (agentId: string, changes: Partial<CouncilAgent>) => void;
  onRemoveAgent?: (agentId: string) => void;
  onAddAgent?: () => void;
  onFillAgent?: (agentId: string) => Promise<void>;
  disabled?: boolean;
}

export const CouncilSetupPanel: FC<CouncilSetupPanelProps> = ({
  topic,
  agents: initialAgents,
  rounds: initialRounds,
  synthesisGuidance,
  diffStatuses,
  onRun,
  onCancel,
  onUpdateAgent,
  onRemoveAgent,
  onAddAgent,
  onFillAgent,
  disabled,
}) => {
  const [agents, setAgents] = useState<CouncilAgent[]>(initialAgents);
  const [rounds, setRounds] = useState(initialRounds);

  // Snapshot available tools from the registry at mount time.
  // The registry is already populated at app init (built-ins + active MCP servers).
  const availableTools = useMemo(() => getToolRegistry().getAllAsBackendTools(), []);

  // Sync local state when context-driven changes arrive (add/remove/update agent).
  useEffect(() => {
    setAgents(initialAgents);
  }, [initialAgents]);

  const handleContentiousnessChange = useCallback((agentId: string, value: number) => {
    setAgents((prev) =>
      prev.map((a) => (a.id === agentId ? { ...a, contentiousness: value } : a)),
    );
  }, []);

  const handleRun = useCallback(() => {
    onRun({ agents, topic, rounds, synthesis_guidance: synthesisGuidance });
  }, [agents, topic, rounds, synthesisGuidance, onRun]);

  return (
    <div className={cn(
      'flex flex-col gap-md p-md rounded-base',
      'border border-border bg-surface',
      'max-w-[42rem] w-full mx-auto',
    )}>
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-semibold text-text m-0">Council Setup</h3>
          <p className="text-xs text-text-muted m-0 mt-xs">
            {agents.length} agents &middot; {rounds} round{rounds !== 1 ? 's' : ''}
          </p>
        </div>
        <Button variant="ghost" size="sm" onClick={onCancel} disabled={disabled}>
          Cancel
        </Button>
      </div>

      {/* Topic */}
      <div className="px-sm py-xs bg-background rounded-base">
        <p className="text-sm text-text m-0 leading-relaxed">&ldquo;{topic}&rdquo;</p>
      </div>

      {/* Agent cards grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-sm">
        {agents.map((agent) => (
          <AgentCard
            key={agent.id}
            agent={agent}
            diffStatus={diffStatuses?.[agent.id]}
            onContentiousnessChange={handleContentiousnessChange}
            onUpdate={onUpdateAgent}
            onRemove={onRemoveAgent}
            onFillAgent={onFillAgent}
            availableTools={availableTools}
            disabled={disabled}
          />
        ))}
        {onAddAgent && !disabled && <AddAgentButton onClick={onAddAgent} disabled={disabled} />}
      </div>

      {/* Rounds control + Run */}
      <div className="flex items-center justify-between pt-sm border-t border-border">
        <label className="flex items-center gap-sm text-sm text-text-secondary">
          Rounds
          <input
            type="number"
            min={1}
            max={10}
            value={rounds}
            onChange={(e) => setRounds(Math.max(1, Math.min(10, parseInt(e.target.value) || 1)))}
            disabled={disabled}
            className="w-16 py-xs px-sm border border-border rounded-base bg-surface text-text text-sm text-center focus:outline-none focus:border-primary disabled:opacity-50"
          />
        </label>
        <Button variant="primary" size="sm" onClick={handleRun} disabled={disabled || agents.length === 0}>
          Run Council
        </Button>
      </div>
    </div>
  );
};
