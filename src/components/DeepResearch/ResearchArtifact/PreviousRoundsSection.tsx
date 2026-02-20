import React, { useState } from 'react';
import { History, ChevronDown, ChevronRight, Layers } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import type { RoundSummary } from './types';

interface PreviousRoundsSectionProps {
  roundSummaries: RoundSummary[];
}

/**
 * Previous rounds section — shows compressed summaries from prior research rounds.
 * Only visible when there are completed rounds (currentRound > 1).
 */
const PreviousRoundsSection: React.FC<PreviousRoundsSectionProps> = ({ roundSummaries }) => {
  const [expanded, setExpanded] = useState(false);

  if (roundSummaries.length === 0) return null;

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div
        className="flex items-center justify-between mb-2.5"
        onClick={() => setExpanded(!expanded)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            setExpanded(!expanded);
          }
        }}
        style={{ cursor: 'pointer' }}
      >
        <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
          <Icon icon={History} size={14} />
          Previous Rounds
        </span>
        <span className="text-[11px] text-text-muted font-normal">
          {roundSummaries.length} round{roundSummaries.length !== 1 ? 's' : ''}
          <Icon
            icon={expanded ? ChevronDown : ChevronRight}
            size={14}
            style={{ marginLeft: 4 }}
          />
        </span>
      </div>
      {expanded && (
        <div className="flex flex-col gap-2.5 px-3 py-2.5">
          {roundSummaries.map((round) => (
            <div key={round.round} className="p-3 bg-background-tertiary border border-border rounded-lg border-l-[3px] border-l-[#a78bfa]">
              <div className="flex items-center justify-between mb-2">
                <span className="flex items-center gap-1.5 text-xs font-semibold text-[#a78bfa]">
                  <Icon icon={Layers} size={12} />
                  Round {round.round}
                  {round.perspective && (
                    <span className="font-medium text-[#c4b5fd] italic ml-1">
                      ({round.perspective})
                    </span>
                  )}
                </span>
                <span className="text-[11px] text-text-muted">
                  {round.factCountAtEnd} facts · {round.questionsAnsweredThisRound.length} questions
                </span>
              </div>
              <div className="text-xs text-text-secondary leading-normal mb-2">
                {round.summary}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};

PreviousRoundsSection.displayName = 'PreviousRoundsSection';

export { PreviousRoundsSection };
