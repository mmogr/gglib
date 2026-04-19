/**
 * Renders a completed council session from persisted data.
 *
 * Unlike CouncilThread (which reads live state from CouncilContext),
 * this component accepts a static SerializableCouncilSession prop and
 * renders the full historical debate read-only.
 *
 * @module components/Council/Messages/HistoricalCouncilThread
 */

import { type FC, useMemo } from 'react';
import type { SerializableCouncilSession } from '../../../types/council';
import { CouncilMessage } from './CouncilMessage';
import { JudgeMessage } from './JudgeMessage';
import { StanceTable } from './StanceTable';
import { CompactionNotice } from './CompactionNotice';
import { SynthesisMessage } from './SynthesisMessage';
import { RoundSeparator } from './RoundSeparator';

export interface HistoricalCouncilThreadProps {
  session: SerializableCouncilSession;
}

type ThreadItem =
  | { kind: 'round'; round: number }
  | { kind: 'contribution'; index: number }
  | { kind: 'judge'; assessmentIndex: number }
  | { kind: 'stances' }
  | { kind: 'compacted'; compactedIndex: number }
  | { kind: 'synthesis' };

export const HistoricalCouncilThread: FC<HistoricalCouncilThreadProps> = ({ session }) => {
  const items = useMemo<ThreadItem[]>(() => {
    const result: ThreadItem[] = [];
    let lastRound = -1;

    const judges = session.judgeAssessments ?? [];
    const compacted = session.compactedRounds ?? [];
    const judgeByRound = new Map(judges.map((a, i) => [a.round, i]));
    const compactedByRound = new Map(compacted.map((c, i) => [c.round, i]));
    const lastJudgeRound = judges.length > 0 ? judges[judges.length - 1].round : -1;

    const pushPostRound = (round: number) => {
      const jIdx = judgeByRound.get(round);
      if (jIdx !== undefined) {
        result.push({ kind: 'judge', assessmentIndex: jIdx });
        if (round === lastJudgeRound && (session.stances?.length ?? 0) > 0) {
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
        if (lastRound >= 0) pushPostRound(lastRound);
        result.push({ kind: 'round', round: c.round });
        lastRound = c.round;
      }
      result.push({ kind: 'contribution', index: i });
    }

    if (lastRound >= 0) pushPostRound(lastRound);

    if (session.synthesisText) {
      result.push({ kind: 'synthesis' });
    }

    return result;
  }, [session]);

  if (items.length === 0 && session.error) {
    return (
      <div className="text-sm text-danger px-md py-sm">
        Council error: {session.error}
      </div>
    );
  }

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
          case 'judge': {
            const judges = session.judgeAssessments ?? [];
            const a = judges[item.assessmentIndex];
            return (
              <JudgeMessage
                key={`judge-${a.round}`}
                assessment={a}
              />
            );
          }
          case 'stances':
            return <StanceTable key="stances" stances={session.stances ?? []} />;
          case 'compacted': {
            const compacted = session.compactedRounds ?? [];
            const cr = compacted[item.compactedIndex];
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
                isStreaming={false}
              />
            );
        }
      })}

      {session.error && (
        <div className="text-sm text-danger px-md py-sm rounded-base border border-danger/20 bg-danger/5">
          {session.error}
        </div>
      )}
    </div>
  );
};
