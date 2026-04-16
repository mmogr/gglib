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

import { type FC, useMemo } from 'react';
import { useCouncilContext } from '../../../contexts/CouncilContext';
import type { CouncilConfig } from '../../../types/council';
import { CouncilMessage } from './CouncilMessage';
import { SynthesisMessage } from './SynthesisMessage';
import { RoundSeparator } from './RoundSeparator';
import { CouncilSetupPanel } from '../Setup/CouncilSetupPanel';

export interface CouncilThreadProps {
  onRun: (config: CouncilConfig) => void;
  onCancel: () => void;
}

type ThreadItem =
  | { kind: 'round'; round: number }
  | { kind: 'contribution'; index: number }
  | { kind: 'streaming' }
  | { kind: 'synthesis' };

export const CouncilThread: FC<CouncilThreadProps> = ({ onRun, onCancel }) => {
  const { session } = useCouncilContext();

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

  // Idle / suggesting phases: nothing to render
  if (session.phase === 'idle' || session.phase === 'suggesting') {
    return null;
  }

  // Setup phase: show the setup panel
  if (session.phase === 'setup') {
    return (
      <CouncilSetupPanel
        topic={session.topic}
        agents={session.suggestedAgents}
        rounds={session.suggestedRounds}
        synthesisGuidance={session.suggestedSynthesisGuidance}
        onRun={onRun}
        onCancel={onCancel}
      />
    );
  }

  // Error state
  if (session.phase === 'error' && items.length === 0) {
    return (
      <div className="text-sm text-danger px-md py-sm">
        Council error: {session.error ?? 'Unknown error'}
      </div>
    );
  }

  // Deliberating / synthesizing / complete
  return (
    <div className="flex flex-col gap-md">
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
