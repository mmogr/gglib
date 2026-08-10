import type { FC } from 'react';
import { ContextUsageDonut } from '../ContextUsageDonut';
import { Readout, Sparkline } from '../primitives';
import { useMetricHistory } from '../../hooks/useMetricHistory';
import { formatRate } from '../../utils/formatRate';
import { tokensInUse, type SlotSnapshot } from '../../services/transport/types/dashboard';

interface SlotCardProps {
  slot: SlotSnapshot;
  /** Donut diameter in px. The tray popover renders these smaller. */
  size?: number;
  /**
   * Snapshot identity — one change per dashboard event. Omitting it means an
   * unchanged counter never records, so the rate readout freezes at its last
   * value instead of decaying to zero when the slot idles.
   */
  tick?: unknown;
  /**
   * Identity of the served model. Slot ids survive a model swap, so without
   * this the sparkline would stitch two models' throughput into one series.
   */
  resetKey?: unknown;
}

/**
 * One llama.cpp inference slot: context-usage donut plus a live generation
 * readout. Owns its own metric history — per-slot series belong with per-slot
 * component identity, which keeps the parent sections presentational and the
 * tray and modal rendering identical.
 */
export const SlotCard: FC<SlotCardProps> = ({ slot, size = 80, tick, resetKey }) => {
  const nextToken = Array.isArray(slot.next_token) ? slot.next_token[0] : slot.next_token;
  const genRate = useMetricHistory(nextToken?.n_decoded, { mode: 'rate', tick, resetKey });

  return (
    <div className="flex flex-col items-center gap-sm p-md rounded-base bg-surface-elevated">
      <ContextUsageDonut
        used={tokensInUse(slot)}
        total={slot.n_ctx ?? null}
        size={size}
        strokeWidth={size / 10}
      />
      <Readout
        size="sm"
        align="center"
        label={`Slot ${slot.id}${slot.is_processing ? ' · active' : ''}`}
        value={genRate.latest != null ? formatRate(genRate.latest) : '—'}
        unit="tok/s"
        trend={
          <Sparkline
            values={genRate.values}
            ariaLabel={`Slot ${slot.id} generation rate, recent history`}
            className="text-primary"
          />
        }
      />
    </div>
  );
};
