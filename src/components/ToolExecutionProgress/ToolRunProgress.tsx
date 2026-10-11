/**
 * How far a running tool has got, under its row: what it is doing in words,
 * a step bar, and the picture it is making as far as that has got.
 *
 * For a tool that takes minutes (an image render). The words and the bar
 * are its last `tool_progress` event; the picture is the run's latest
 * preview frame for the call, which is small (about 128 px a side) and is
 * shown at twice that, scaled up smoothly so it reads as a picture
 * sharpening and not as blocks.
 *
 * @module ToolRunProgress
 */

import type { FC } from 'react';
import { PromptProgressBar } from '../PromptProgressBar';
import { previewSrc, type PreviewFrame } from '../../hooks/useGglibRuntime/runPreviews';
import type { ToolProgress } from '../../types/messages';

/** What a tool is doing, in words: its stage, and its counts where it gave them. */
export function progressWords(progress: ToolProgress): string {
  switch (progress.stage) {
    case 'queued':
      return progress.position != null && progress.position > 0 ? `Queued, ${progress.position} in line` : 'Queued';
    case 'loading':
      return 'Loading';
    case 'sampling': {
      const steps = progress.done != null && progress.total ? ` ${progress.done} of ${progress.total}` : '';
      const pass = progress.pass != null && progress.pass > 1 ? `, image ${progress.pass}` : '';
      return `Sampling${steps}${pass}`;
    }
    case 'decoding':
      return 'Decoding';
    case 'finishing':
      return 'Finishing';
    default:
      // A stage from a newer gglib: said as it came.
      return String(progress.stage);
  }
}

/** The bar's fill: the steps done while sampling, all of it after, none before. */
function barOf(progress: ToolProgress): { done: number; total: number } {
  if (progress.stage === 'decoding' || progress.stage === 'finishing') return { done: 1, total: 1 };
  if (progress.stage === 'sampling' && progress.done != null && progress.total) {
    return { done: progress.done, total: progress.total };
  }
  return { done: 0, total: 1 };
}

export const ToolRunProgress: FC<{ progress?: ToolProgress; preview?: PreviewFrame }> = ({ progress, preview }) => {
  if (!progress && !preview) return null;
  const bar = progress ? barOf(progress) : null;
  return (
    <div className="flex flex-col gap-xs px-2 pb-2">
      {progress && bar && (
        <>
          <span className="font-mono tabular-nums text-2xs text-text-muted">
            {progressWords(progress)}
          </span>
          <PromptProgressBar processed={bar.done} total={bar.total} className="max-w-[256px]" />
        </>
      )}
      {preview && (
        <img
          src={previewSrc(preview)}
          alt={`Preview of the image being made, step ${preview.step} of ${preview.total}`}
          className="block w-[256px] max-w-full h-auto rounded-base [image-rendering:auto]"
        />
      )}
    </div>
  );
};
