/**
 * ProxyAdmissionPanel.
 *
 * Which models hold VRAM, what is queued behind them, and why the second slot
 * is or is not in use.
 *
 * This is the panel that explains slowness. When a chat client and an
 * embeddings client share one endpoint, the thing a user needs to see is not
 * "requests are pending" but *why*: one model is loaded, another is wanted, and
 * the swap between them is being batched rather than paid for per request. A
 * queue depth with nothing beside it would be alarming; a queue depth next to a
 * swap count that is barely moving is the system working.
 *
 * The second-slot line is deliberately always present, even when nothing is
 * co-loaded. An empty slot on a card with 12 GB free is exactly the case a user
 * would otherwise assume is a bug, so the backend sends the reason with the
 * state and this renders it verbatim.
 *
 * @module components/ProxyAdmissionPanel
 */

import type { FC } from 'react';
import { Layers, Layers2 } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { Icon } from './ui/Icon';
import type {
  AdmissionSnapshot,
  QueuedModelSnapshot,
  ResidentSlotSnapshot,
  SecondarySlotState,
} from '../services/transport/types/dashboard';
import { formatCount } from '../utils/format';

export interface ProxyAdmissionPanelProps {
  /** `null`/`undefined` on a proxy that predates admission control. */
  admission?: AdmissionSnapshot | null;
}

/** Whole seconds, or a coarse minutes figure once a wait gets long. */
function formatWait(ms: number): string {
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}

/** How long a model has been loaded, in the same shape as a wait. */
function formatResidency(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

/**
 * Colour for the second-slot state.
 *
 * Semantic, not decorative: a co-resident model is a healthy state, and every
 * refusal is merely informational — none of them is a failure, so none of them
 * gets danger red. An unreadable VRAM budget is the machine's limitation, not
 * gglib's fault and not the user's.
 */
function secondarySlotTone(state: SecondarySlotState): string {
  return state === 'resident' ? 'text-success' : 'text-text-muted';
}

/**
 * Two layers for a slot that holds a model, one outline for a slot that does
 * not. The distinction is the whole point of the row, so it earns an icon.
 */
const SLOT_ICON: Record<SecondarySlotState, LucideIcon> = {
  resident: Layers,
  available: Layers2,
  too_large: Layers2,
  no_headroom: Layers2,
  unknown_footprint: Layers2,
  unknown_budget: Layers2,
};

/** One resident model and what it is doing. */
const ResidentSlot: FC<{ slot: ResidentSlotSnapshot }> = ({ slot }) => (
  <div className="flex items-baseline justify-between gap-md p-md rounded-base bg-surface-elevated">
    <div className="flex items-baseline gap-sm min-w-0">
      <span className="text-sm text-text truncate">{slot.model_name}</span>
      <span className="text-xs text-text-muted shrink-0">
        {slot.is_primary ? 'primary' : 'secondary'}
      </span>
    </div>
    <span className="text-xs text-text-muted font-mono tabular-nums shrink-0">
      {slot.inflight > 0 ? `${formatCount(slot.inflight)} in flight · ` : 'idle · '}
      {formatResidency(slot.resident_for_secs)}
    </span>
  </div>
);

/** One model with requests waiting for it. */
const QueuedModel: FC<{ queued: QueuedModelSnapshot }> = ({ queued }) => (
  <div className="flex items-baseline justify-between gap-md">
    <span className="text-xs text-text-muted truncate">{queued.model_name}</span>
    <span className="text-sm text-text font-mono tabular-nums shrink-0">
      {formatCount(queued.waiting)} waiting · oldest {formatWait(queued.oldest_wait_ms)}
    </span>
  </div>
);

export const ProxyAdmissionPanel: FC<ProxyAdmissionPanelProps> = ({ admission }) => {
  if (!admission) {
    return <p className="text-sm text-text-muted">Admission control is not reporting.</p>;
  }

  const { slots, queued, total_queued: totalQueued, total_swaps: totalSwaps } = admission;
  const secondary = admission.secondary_slot;

  return (
    <div className="flex flex-col gap-sm">
      {slots.length > 0 ? (
        <div className="flex flex-col gap-sm">
          {slots.map((slot) => (
            <ResidentSlot key={slot.slot} slot={slot} />
          ))}
        </div>
      ) : (
        <p className="text-sm text-text-muted">No model is loaded yet.</p>
      )}

      <div className="flex items-start gap-sm p-md rounded-base bg-surface-elevated">
        <Icon
          icon={SLOT_ICON[secondary.state]}
          className={`shrink-0 mt-[2px] ${secondarySlotTone(secondary.state)}`}
          size={14}
        />
        <p className="text-xs text-text-muted">{secondary.detail}</p>
      </div>

      {queued.length > 0 && (
        <div className="flex flex-col gap-xs p-md rounded-base bg-surface-elevated">
          {queued.map((entry) => (
            <QueuedModel key={entry.model_name} queued={entry} />
          ))}
        </div>
      )}

      {/*
        The pair, not either alone. Swaps on their own look like a cost; queued
        requests on their own look like a backlog. Side by side they show how
        many requests one swap served.
      */}
      <div className="flex items-baseline justify-between gap-md">
        <span className="text-xs text-text-muted">Requests admitted</span>
        <span className="text-sm text-text font-mono tabular-nums">{formatCount(totalQueued)}</span>
      </div>
      <div className="flex items-baseline justify-between gap-md">
        <span className="text-xs text-text-muted">Model swaps</span>
        <span className="text-sm text-text font-mono tabular-nums">{formatCount(totalSwaps)}</span>
      </div>
    </div>
  );
};

export default ProxyAdmissionPanel;
