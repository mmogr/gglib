import { FC } from 'react';
import { StatusMark } from '../../status/StatusMark';
import { PromptProgressBar } from '../../PromptProgressBar';
import type { PromptReading, TurnWaiting } from '../../../types/messages';
import { promptLines, waitingLines, type ArrivingPhase } from '../../../readings/turnFigures';

/**
 * A reply that has no token yet, in its own place: one status line, what it
 * waits for or how far its prompt is read, and the bar under it while the
 * prompt is read. One queued behind an image render says the step that
 * render is on. Gone with the first token, when the fold and the text say
 * what it is doing.
 */
export const ReplyArrival: FC<{ phase: ArrivingPhase; prompt?: PromptReading; waiting?: TurnWaiting }> = ({
  phase,
  prompt,
  waiting,
}) => {
  const figures = [
    ...(waiting && phase === 'Queued behind an image render' ? waitingLines(waiting) : []),
    ...(prompt ? promptLines(prompt, phase) : []),
  ];
  return (
    <div role="status" className="flex flex-col gap-xs">
      <div className="flex flex-wrap items-center gap-x-md gap-y-xs text-xs text-text-muted tabular-nums">
        <StatusMark kind="changing" word={phase} className="text-xs" />
        {figures.map((line) => (
          <span key={line}>{line}</span>
        ))}
      </div>
      {prompt && phase === 'Reading the prompt' && (
        <PromptProgressBar processed={prompt.processed} total={prompt.total} className="max-w-[240px]" />
      )}
    </div>
  );
};
