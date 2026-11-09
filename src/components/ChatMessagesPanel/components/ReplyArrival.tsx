import { FC } from 'react';
import { StatusMark } from '../../status/StatusMark';
import { PromptProgressBar } from '../../PromptProgressBar';
import type { PromptReading } from '../../../types/messages';
import { promptLines } from '../../../readings/turnFigures';

/**
 * A reply that has no token yet, in its own place: one status line, what it
 * waits for or how far its prompt is read, and the bar under it while the
 * prompt is read. A wait is the daemon's own sentence, shown as it came
 * ("Loading Qwen3-4B… 12 s").
 * Gone with the first token, when the fold and the text say what it is
 * doing.
 */
export const ReplyArrival: FC<{ phase: string; prompt?: PromptReading }> = ({ phase, prompt }) => {
  const figures = prompt ? promptLines(prompt, phase) : [];
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
