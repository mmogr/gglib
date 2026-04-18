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
import { JudgeMessage } from './JudgeMessage';
import { StanceTable } from './StanceTable';
import { CompactionNotice } from './CompactionNotice';
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
  | { kind: 'judge'; assessmentIndex: number }
  | { kind: 'judge-streaming' }
  | { kind: 'stances' }
  | { kind: 'compacted'; compactedIndex: number }
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

    // Lookup maps for post-round items
    const judgeByRound = new Map(session.judgeAssessments.map((a, i) => [a.round, i]));
    const compactedByRound = new Map(session.compactedRounds.map((c, i) => [c.round, i]));
    const lastJudgeRound = session.judgeAssessments.length > 0
      ? session.judgeAssessments[session.judgeAssessments.length - 1].round
      : -1;

    const pushPostRound = (round: number) => {
      const jIdx = judgeByRound.get(round);
      if (jIdx !== undefined) {
        result.push({ kind: 'judge', assessmentIndex: jIdx });
        // Show stances after the most recent completed judge
        if (round === lastJudgeRound && session.stances.length > 0) {
          result.push({ kind: 'stances' });
        }
      }
      const cIdx = compactedByRound.get(round);
      if (cIdx !== undefined) {
        result.push({ kind: 'compacted', compactedIndex: cIdx });
      }
    };

    for (let i = 0; i < session.contributions.length; i++) {
      const c = session.contributions[i];
      if (c.round !== lastRound) {
        // Inject post-round items for the previous round before starting a new one
        if (lastRound >= 0) pushPostRound(lastRound);
        result.push({ kind: 'round', round: c.round });
        lastRound = c.round;
      }
      result.push({ kind: 'contribution', index: i });
    }

    // Post-round items for the final round of contributions
    if (lastRound >= 0) pushPostRound(lastRound);

    // Streaming judge evaluation
    if (session.phase === 'judging') {
      result.push({ kind: 'judge-streaming' });
      if (session.stances.length > 0) {
        result.push({ kind: 'stances' });
      }
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
  }, [
    session.contributions, session.activeAgentId, session.currentRound,
    session.phase, session.judgeAssessments, session.stances,
    session.compactedRounds,
  ]);

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
                  rebuttalTarget: session.activeRebuttalTarget,
                }}
              />
            );

          case 'judge': {
            const a = session.judgeAssessments[item.assessmentIndex];
            return (
              <JudgeMessage
                key={`judge-${a.round}`}
                assessment={a}
              />
            );
          }

          case 'judge-streaming':
            return (
              <JudgeMessage
                key="judge-streaming"
                streamingText={session.activeJudgeText}
                streamingRound={session.activeJudgeRound}
              />
            );

          case 'stances':
            return <StanceTable key={`stances-${session.stances.length}`} stances={session.stances} />;

          case 'compacted': {
            const cr = session.compactedRounds[item.compactedIndex];
            return (
              <CompactionNotice
                key={`compacted-${cr.round}`}
                round={cr.round}
                summary={cr.summary}
              />
            );
          }

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
