import type { FC, ReactNode } from 'react';
import { ConnectionRow } from './ConnectionRow';
import { ModelSignalsCard, countOf, hasSignal } from './ModelSignalsCard';
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
 * Per-model signals: what failed, what went in circles, and for which model —
 * the section `gglib proxy dashboard` prints, under the same rules (#1092).
 *
 * Only a model with something to report gets a card, since listing every
 * clean one would bury the one that is not. A clean run says so over its
 * denominator, and a run that has recorded nothing says that instead: "none"
 * is a claim only evidence earns. Nothing at all before the first snapshot.
 * Cards come in name order, as the CLI's do: the proxy sends a hash map, and
 * its order can change from one frame to the next.
 */
export const ModelSignalsSection: FC<SectionProps> = ({ snapshot }) => {
  if (!snapshot) return null;
  const perModel = Object.entries(snapshot.per_model_defects).sort(([a], [b]) =>
    a < b ? -1 : a > b ? 1 : 0,
  );
  const reporting = perModel.filter(([, counts]) => hasSignal(counts));
  // Agent turns are named beside requests, never added to them: one client
  // conversation is many agent turns, and a sum would be neither.
  const requests = perModel.reduce((sum, [, c]) => sum + c.requests, 0);
  const turns = perModel.reduce((sum, [, c]) => sum + c.agent_guard_scanned, 0);
  const across = `${countOf(requests, 'request')}${turns > 0 ? ` and ${countOf(turns, 'agent turn')}` : ''}`;

  return (
    <section>
      <SectionHeading>Per-Model Signals</SectionHeading>
      {reporting.length > 0 ? (
        <div className="flex flex-col gap-sm">
          {reporting.map(([model, counts]) => (
            <ModelSignalsCard key={model} model={model} counts={counts} />
          ))}
        </div>
      ) : (
        <p className="text-sm text-text-muted">
          {perModel.length === 0
            ? 'Nothing recorded yet.'
            : `None across ${across}, ${countOf(perModel.length, 'model')}.`}
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
