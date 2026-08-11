import type { FC } from 'react';
import { PromptProgressBar } from '../PromptProgressBar';
import type {
  ActiveConnectionSnapshot,
  ConnectionPhase,
} from '../../services/transport/types/dashboard';

const phaseLabels: Record<ConnectionPhase, string> = {
  queued: 'Queued',
  processing_prompt: 'Processing prompt',
  generating: 'Generating',
};

interface ConnectionRowProps {
  connection: ActiveConnectionSnapshot;
}

/**
 * One in-flight request: which model it is against and what it is doing.
 *
 * The prompt-progress bar only appears while the prompt is being processed —
 * during generation there is no total to measure against, so a bar would be
 * inventing a denominator.
 */
export const ConnectionRow: FC<ConnectionRowProps> = ({ connection }) => (
  <div className="p-md rounded-base bg-surface-elevated">
    <div className="flex items-center justify-between mb-sm">
      <span className="text-sm font-semibold text-text truncate">{connection.model_name}</span>
      <span className="text-xs text-text-muted">{phaseLabels[connection.phase]}</span>
    </div>
    {connection.phase === 'processing_prompt' && (
      <PromptProgressBar
        processed={connection.prompt_processed ?? null}
        total={connection.prompt_total ?? null}
      />
    )}
  </div>
);
