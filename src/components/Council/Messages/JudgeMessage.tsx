/**
 * Renders a judge assessment in the council thread.
 *
 * Amber/gold accent to distinguish from agent turns and synthesis.
 * Supports both streaming (live `activeText`) and completed states
 * (from `JudgeAssessment`). Shows consensus/continue badge.
 *
 * @module components/Council/Messages/JudgeMessage
 */

import type { FC } from 'react';
import { Gavel } from 'lucide-react';
import type { JudgeAssessment } from '../../../types/council';
import { cn } from '../../../utils/cn';
import { Icon } from '../../ui/Icon';
import MarkdownMessageContent from '../../ChatMessagesPanel/components/MarkdownMessageContent';

export interface JudgeMessageProps {
  /** Completed assessment (for finished evaluations). */
  assessment?: JudgeAssessment;
  /** Live-streaming text (for the active judge turn). */
  streamingText?: string;
  /** Round being evaluated (used during streaming). */
  streamingRound?: number;
}

export const JudgeMessage: FC<JudgeMessageProps> = ({
  assessment,
  streamingText,
  streamingRound,
}) => {
  const text = assessment?.summary ?? streamingText ?? '';
  const round = assessment?.round ?? streamingRound;
  const isStreaming = !assessment && streamingText !== undefined;
  const consensusReached = assessment?.consensusReached;

  return (
    <div
      className={cn(
        'rounded-base border-l-[3px] p-md transition-colors duration-200',
        'bg-[color-mix(in_srgb,#d97706_6%,var(--color-surface))]',
        'border-[color-mix(in_srgb,#d97706_40%,transparent)]',
      )}
    >
      {/* Header */}
      <div className="flex items-center gap-sm mb-sm">
        <span className="w-6 h-6 rounded-full flex items-center justify-center bg-amber-600/20 shrink-0">
          <Icon icon={Gavel} size={14} className="text-amber-600" />
        </span>
        <span className="text-sm font-semibold text-amber-600">Judge</span>
        {round !== undefined && (
          <span className="text-xs text-text-muted">Round {round + 1}</span>
        )}
        {consensusReached !== undefined && (
          <span
            className={cn(
              'ml-auto text-xs font-medium px-sm py-[1px] rounded-full',
              consensusReached
                ? 'bg-success/15 text-success'
                : 'bg-warning/15 text-warning',
            )}
          >
            {consensusReached ? 'Consensus' : 'Continue'}
          </span>
        )}
      </div>

      {/* Content */}
      {text ? (
        <MarkdownMessageContent text={text} />
      ) : isStreaming ? (
        <span className="text-sm text-text-muted animate-pulse">Evaluating…</span>
      ) : null}
    </div>
  );
};
