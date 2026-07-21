/**
 * ProxyDashboardModal.
 *
 * Self-contained live view of a running proxy's dashboard: active
 * connections (with per-request prompt-processing progress bars) and
 * inference slots (with per-slot context-usage donuts), backed by
 * `useProxyDashboard()`'s native `EventSource` subscription.
 *
 * Triggered from `ProxyControl.tsx`'s "View Dashboard" button, following the
 * same self-contained `{isOpen, onClose}` Modal pattern as `SettingsModal`/
 * `LlamaInstallModal` rather than threading state through
 * `ModelControlCenterPage` — the dashboard is proxy-wide, not tied to a
 * specific selected model.
 *
 * @module components/ProxyDashboardModal
 */

import type { FC } from 'react';
import { Modal } from './ui/Modal';
import { ContextUsageDonut } from './ContextUsageDonut';
import { PromptProgressBar } from './PromptProgressBar';
import { ProxyCachePanel } from './ProxyCachePanel';
import { useProxyDashboard } from '../hooks/useProxyDashboard';
import { tokensInUse, type ActiveConnectionSnapshot, type ConnectionPhase, type SlotSnapshot } from '../services/transport/types/dashboard';

export interface ProxyDashboardModalProps {
  isOpen: boolean;
  onClose: () => void;
  host: string;
  port: number;
}

const phaseLabels: Record<ConnectionPhase, string> = {
  queued: 'Queued',
  processing_prompt: 'Processing prompt',
  generating: 'Generating',
};

function ConnectionRow({ connection }: { connection: ActiveConnectionSnapshot }) {
  return (
    <div className="p-md rounded-base border border-border bg-surface-elevated">
      <div className="flex items-center justify-between mb-sm">
        <span className="text-sm font-semibold text-text truncate">{connection.model_name}</span>
        <span className="text-xs text-text-muted">{phaseLabels[connection.phase]}</span>
      </div>
      {connection.phase === 'processing_prompt' && (
        <PromptProgressBar processed={connection.prompt_processed ?? null} total={connection.prompt_total ?? null} />
      )}
    </div>
  );
}

function SlotCard({ slot }: { slot: SlotSnapshot }) {
  return (
    <div className="flex flex-col items-center gap-sm p-md rounded-base border border-border bg-surface-elevated">
      <ContextUsageDonut used={tokensInUse(slot)} total={slot.n_ctx ?? null} size={80} strokeWidth={8} />
      <span className="text-xs text-text-muted">
        Slot {slot.id}
        {slot.is_processing ? ' · active' : ''}
      </span>
    </div>
  );
}

export const ProxyDashboardModal: FC<ProxyDashboardModalProps> = ({ isOpen, onClose, host, port }) => {
  const { snapshot, connected } = useProxyDashboard({ host, port: isOpen ? port : null });

  return (
    <Modal
      open={isOpen}
      onClose={onClose}
      title="Proxy Dashboard"
      description={connected ? `Live · ${snapshot?.total_requests ?? 0} total requests` : 'Connecting…'}
      size="lg"
    >
      <div className="flex flex-col gap-lg">
        <section>
          <h3 className="text-xs font-semibold uppercase text-text-secondary mb-sm">
            Active Connections {snapshot ? `(${snapshot.active_connections.length})` : ''}
          </h3>
          {snapshot && snapshot.active_connections.length > 0 ? (
            <div className="flex flex-col gap-sm">
              {snapshot.active_connections.map((connection) => (
                <ConnectionRow key={connection.id} connection={connection} />
              ))}
            </div>
          ) : (
            <p className="text-sm text-text-muted">No active connections.</p>
          )}
        </section>

        <section>
          <h3 className="text-xs font-semibold uppercase text-text-secondary mb-sm">Prompt Cache</h3>
          <ProxyCachePanel cache={snapshot?.cache} />
        </section>

        <section>
          <h3 className="text-xs font-semibold uppercase text-text-secondary mb-sm">Inference Slots</h3>
          {snapshot && snapshot.slots_available && snapshot.slots.length > 0 ? (
            <div className="flex flex-wrap gap-md">
              {snapshot.slots.map((slot) => (
                <SlotCard key={slot.id} slot={slot} />
              ))}
            </div>
          ) : (
            <p className="text-sm text-text-muted">{snapshot?.slots_status ?? 'Slot metrics unavailable.'}</p>
          )}
        </section>
      </div>
    </Modal>
  );
};

export default ProxyDashboardModal;
