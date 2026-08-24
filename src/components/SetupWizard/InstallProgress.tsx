import type { FC } from 'react';
import { Loader2 } from 'lucide-react';

import { Icon } from '../ui/Icon';
import { formatBytes, formatDuration, formatRate } from '../../utils/format';
import type { LlamaProgressEvent } from '../../types/setup';
import { INSTALL_PHASE_LABELS } from '../../types/setup';

/**
 * Live rendering of a llama.cpp install.
 *
 * The install emits a phase stream, and only the download phase carries bytes —
 * every other phase is a spinner with the phase's own label. Rate and time
 * remaining are measured by the backend and shown as sent; an absent value is
 * an estimator that has not warmed up yet, which is not the same as zero.
 */
export const InstallProgress: FC<{ progress: LlamaProgressEvent | null }> = ({ progress }) => {
  if (progress?.type === 'progress') {
    const pct = progress.total > 0 ? (progress.downloaded / progress.total) * 100 : 0;

    return (
      <div className="flex flex-col gap-2">
        <div className="h-2 bg-background-tertiary rounded overflow-hidden">
          <div
            className="h-full bg-gradient-to-r from-primary to-primary-light rounded transition-[width] duration-300"
            style={{ width: `${pct}%` }}
          />
        </div>
        <div className="flex justify-between text-xs text-text-secondary font-mono tabular-nums">
          <span>{progress.total > 0 ? `${pct.toFixed(1)}%` : 'Starting…'}</span>
          {progress.total > 0 && (
            <span>
              {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
            </span>
          )}
        </div>
        <div className="flex justify-between text-xs text-text-muted font-mono tabular-nums">
          <span>{formatRate(progress.rate_bps)}</span>
          <span>{formatDuration(progress.eta_seconds)} remaining</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2 text-sm text-text-secondary">
      <Icon icon={Loader2} className="animate-spin" size={16} />
      <span>
        {progress?.type === 'phase_started'
          ? INSTALL_PHASE_LABELS[progress.phase]
          : 'Preparing download…'}
      </span>
    </div>
  );
};

export default InstallProgress;
