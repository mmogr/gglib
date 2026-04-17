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
import { SynthesisMessage } from './SynthesisMessage';
import { RoundSeparator } from './RoundSeparator';

export interface HistoricalCouncilThreadProps {
  session: SerializableCouncilSession;
}

type ThreadItem =
  | { kind: 'round'; round: number }
  | { kind: 'contribution'; index: number }
  | { kind: 'synthesis' };

export const HistoricalCouncilThread: FC<HistoricalCouncilThreadProps> = ({ session }) => {
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

    if (session.synthesisText) {
      result.push({ kind: 'synthesis' });
    }

    return result;
  }, [session.contributions, session.synthesisText]);

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
