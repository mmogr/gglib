import type { FC } from 'react';
import { ContextUsageDonut } from '../ContextUsageDonut';
import { tokensInUse, type SlotSnapshot } from '../../services/transport/types/dashboard';

interface SlotCardProps {
  slot: SlotSnapshot;
  /** Donut diameter in px. The tray popover renders these smaller. */
  size?: number;
}

/** One llama.cpp inference slot and how much of its context is in use. */
export const SlotCard: FC<SlotCardProps> = ({ slot, size = 80 }) => (
  <div className="flex flex-col items-center gap-sm p-md rounded-base border border-border bg-surface-elevated">
    <ContextUsageDonut
      used={tokensInUse(slot)}
      total={slot.n_ctx ?? null}
      size={size}
      strokeWidth={size / 10}
    />
    <span className="text-xs text-text-muted">
      Slot {slot.id}
      {slot.is_processing ? ' · active' : ''}
    </span>
  </div>
);
