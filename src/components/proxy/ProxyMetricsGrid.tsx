import type { FC, ReactNode } from 'react';
import { ConnectionRow } from './ConnectionRow';
import { RequestThroughput } from './RequestThroughput';
import { SlotCard } from './SlotCard';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';

interface SectionProps {
  /** Latest dashboard snapshot, or `null` before the first event arrives. */
  snapshot: DashboardSnapshot | null;
  /** Render at popover scale: smaller donuts. */
  compact?: boolean;
}

/** Section heading on the contract's type ladder — shared so proxy surfaces can't drift. */
export const SectionHeading: FC<{ children: ReactNode }> = ({ children }) => (
  <h3 className="m-0 text-sm font-semibold text-text mb-sm">{children}</h3>
);

/** In-flight requests, with a count once a snapshot has arrived. */
export const ActiveConnectionsSection: FC<SectionProps> = ({ snapshot }) => {
  const connections = snapshot?.active_connections ?? [];

  return (
    <section>
      <SectionHeading>Active Connections{snapshot ? ` (${connections.length})` : ''}</SectionHeading>
      <RequestThroughput snapshot={snapshot} />
      {connections.length > 0 ? (
        <div className="flex flex-col gap-sm">
          {connections.map((connection) => (
            <ConnectionRow key={connection.id} connection={connection} />
          ))}
        </div>
      ) : (
        <p className="text-sm text-text-muted">No active connections.</p>
      )}
    </section>
  );
};

/**
 * Per-slot context usage.
 *
 * Falls back to the snapshot's own `slots_status` string when llama.cpp is not
 * reporting slots, so the reason shows through instead of an empty panel.
 */
export const InferenceSlotsSection: FC<SectionProps> = ({ snapshot, compact = false }) => {
  const hasSlots = Boolean(snapshot?.slots_available && snapshot.slots.length > 0);

  return (
    <section>
      <SectionHeading>Inference Slots</SectionHeading>
      {hasSlots ? (
        <div className="flex flex-wrap gap-md">
          {snapshot?.slots.map((slot) => (
            <SlotCard
              key={slot.id}
              slot={slot}
              size={compact ? 56 : 80}
              tick={snapshot}
              resetKey={snapshot?.launch?.model_name}
            />
          ))}
        </div>
      ) : (
        <p className="text-sm text-text-muted">
          {snapshot?.slots_status ?? 'Slot metrics unavailable.'}
        </p>
      )}
    </section>
  );
};

/**
 * Both live-metric sections back to back.
 *
 * The dashboard modal renders the two sections individually because its cache
 * panels sit between them; the tray popover has no such interleaving and uses
 * this. Presentational on purpose — the snapshot arrives as a prop, so each
 * surface owns its own `useProxyDashboard` subscription while rendering
 * identical output.
 */
export const ProxyMetricsGrid: FC<SectionProps> = ({ snapshot, compact = false }) => (
  <>
    <ActiveConnectionsSection snapshot={snapshot} />
    <InferenceSlotsSection snapshot={snapshot} compact={compact} />
  </>
);
