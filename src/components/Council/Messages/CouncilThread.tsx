/**
 * Renders the full council deliberation thread.
 *
 * Maps completed contributions with round separators injected between
 * round boundaries, appends the live-streaming agent turn (if any),
 * and renders the synthesis at the end.
 *
 * Designed to be placed inside the chat viewport alongside (or instead
 * of) the regular message thread.
 *
 * @module components/Council/Messages/CouncilThread
 */

import { type FC, useMemo, useRef } from 'react';
import { useCouncilContext } from '../../../contexts/CouncilContext';
import type { CouncilAgent, CouncilConfig } from '../../../types/council';
import { CouncilMessage } from './CouncilMessage';
import { SynthesisMessage } from './SynthesisMessage';
import { RoundSeparator } from './RoundSeparator';
import { CouncilSetupPanel } from '../Setup/CouncilSetupPanel';
import type { DiffStatus } from '../Setup/AgentDiffBadge';

export interface CouncilThreadProps {
  onRun: (config: CouncilConfig) => void;
  onCancel: () => void;
  onUpdateAgent?: (agentId: string, changes: Partial<CouncilAgent>) => void;
  onRemoveAgent?: (agentId: string) => void;
  onAddAgent?: () => void;
  onFillAgent?: (agentId: string) => Promise<void>;
}

type ThreadItem =
  | { kind: 'round'; round: number }
  | { kind: 'contribution'; index: number }
  | { kind: 'streaming' }
  | { kind: 'synthesis' };

export const CouncilThread: FC<CouncilThreadProps> = ({
  onRun, onCancel, onUpdateAgent, onRemoveAgent, onAddAgent, onFillAgent,
}) => {
  const { session } = useCouncilContext();
  const prevAgentsRef = useRef<CouncilAgent[]>([]);

  // Compute per-agent diff statuses after a refinement
  const diffStatuses = useMemo<Record<string, DiffStatus>>(() => {
    const prev = prevAgentsRef.current;
    if (prev.length === 0) return {};
    const prevMap = new Map(prev.map((a) => [a.id, a]));
    const statuses: Record<string, DiffStatus> = {};
    for (const agent of session.suggestedAgents) {
      const old = prevMap.get(agent.id);
      if (!old) {
        statuses[agent.id] = 'new';
      } else if (
        old.name !== agent.name ||
        old.persona !== agent.persona ||
        old.perspective !== agent.perspective
      ) {
        statuses[agent.id] = 'modified';
      } else {
        statuses[agent.id] = 'unchanged';
      }
    }
    return statuses;
  }, [session.suggestedAgents]);

  // Snapshot agents when entering setup so the next refinement can diff
  if (session.phase === 'setup' && prevAgentsRef.current !== session.suggestedAgents) {
    prevAgentsRef.current = session.suggestedAgents;
  }

  // Build a flat list of render items with round separators injected
  const items = useMemo<ThreadItem[]>(() => {
    const result: ThreadItem[] = [];
    let lastRound = -1;

    for (let i = 0; i < session.contributions.length; i++) {
      const c = session.contributions[i];
      if (c.round !== lastRound) {
        result.push({ kind: 'round', round: c.round });
        lastRound = c.round;
      }
      result.push({ kind: 'contribution', index: i });
    }

    // Active streaming turn
    if (session.activeAgentId) {
      if (session.currentRound !== lastRound) {
        result.push({ kind: 'round', round: session.currentRound });
      }
      result.push({ kind: 'streaming' });
    }

    // Synthesis (streaming or complete)
    if (session.phase === 'synthesizing' || session.phase === 'complete') {
      result.push({ kind: 'synthesis' });
    }

    return result;
  }, [session.contributions, session.activeAgentId, session.currentRound, session.phase]);

  // Idle phase: nothing to render
  if (session.phase === 'idle') {
    return null;
  }

  // Suggesting phase with no prior agents: nothing to render yet
  if (session.phase === 'suggesting' && session.suggestedAgents.length === 0) {
    return null;
  }

  // Suggesting phase during refinement: show setup panel disabled
  if (session.phase === 'suggesting' && session.suggestedAgents.length > 0) {
    return (
      <CouncilSetupPanel
        topic={session.topic}
        agents={session.suggestedAgents}
        rounds={session.suggestedRounds}
        synthesisGuidance={session.suggestedSynthesisGuidance}
        onRun={onRun}
        onCancel={onCancel}
        disabled
      />
    );
  }

  // Setup phase: show the setup panel
  if (session.phase === 'setup') {
    return (
      <CouncilSetupPanel
        topic={session.topic}
        agents={session.suggestedAgents}
        rounds={session.suggestedRounds}
        synthesisGuidance={session.suggestedSynthesisGuidance}
        diffStatuses={diffStatuses}
        onRun={onRun}
        onCancel={onCancel}
        onUpdateAgent={onUpdateAgent}
        onRemoveAgent={onRemoveAgent}
        onAddAgent={onAddAgent}
        onFillAgent={onFillAgent}
      />
    );
  }

  // Error state with existing agents: show setup panel with error banner
  if (session.phase === 'error' && session.suggestedAgents.length > 0) {
    return (
      <>
        <div className="w-full text-sm text-danger px-md py-sm">
          Refinement failed: {session.error ?? 'Unknown error'}
        </div>
        <CouncilSetupPanel
          topic={session.topic}
          agents={session.suggestedAgents}
          rounds={session.suggestedRounds}
          synthesisGuidance={session.suggestedSynthesisGuidance}
          diffStatuses={diffStatuses}
          onRun={onRun}
          onCancel={onCancel}
          onUpdateAgent={onUpdateAgent}
          onRemoveAgent={onRemoveAgent}
          onAddAgent={onAddAgent}
          onFillAgent={onFillAgent}
        />
      </>
    );
  }

  // Error state with no agents
  if (session.phase === 'error' && items.length === 0) {
    return (
      <div className="w-full text-sm text-danger px-md py-sm">
        Council error: {session.error ?? 'Unknown error'}
      </div>
    );
  }

  // Deliberating / synthesizing / complete
  return (
    <div className="flex flex-col gap-md w-full">
      {items.map((item) => {
        switch (item.kind) {
          case 'round':
            return <RoundSeparator key={`round-${item.round}`} round={item.round} />;

          case 'contribution': {
            const c = session.contributions[item.index];
            return (
              <CouncilMessage
                key={`contrib-${c.agentId}-${c.round}`}
                contribution={c}
              />
            );
          }

          case 'streaming':
            return (
              <CouncilMessage
                key="streaming"
                streaming={{
                  agentId: session.activeAgentId!,
                  agentName: session.activeAgentName,
                  color: session.activeAgentColor,
                  contentiousness: session.activeAgentContentiousness,
                  text: session.activeAgentText,
                  reasoning: session.activeAgentReasoning,
                  toolCalls: session.activeToolCalls,
                }}
              />
            );

          case 'synthesis':
            return (
              <SynthesisMessage
                key="synthesis"
                text={session.synthesisText}
                isStreaming={session.phase === 'synthesizing'}
              />
            );
        }
      })}

      {/* Trailing error after partial progress */}
      {session.phase === 'error' && session.error && (
        <div className="text-sm text-danger px-md py-sm rounded-base border border-danger/20 bg-danger/5">
          {session.error}
        </div>
      )}
    </div>
  );
};
