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
 * Connection and slot rendering lives in `components/proxy/` because the tray
 * popover shows the same figures; this file owns only the modal chrome and the
 * cache sections, which are too tall for a popover.
 *
 * @module components/ProxyDashboardModal
 */

import type { FC } from 'react';
import { Modal } from './ui/Modal';
import { CacheUsageRows, ProxyCachePanel } from './ProxyCachePanel';
import { ActiveConnectionsSection, InferenceSlotsSection } from './proxy';
import { useProxyDashboard } from '../hooks/useProxyDashboard';

export interface ProxyDashboardModalProps {
  isOpen: boolean;
  onClose: () => void;
  host: string;
  port: number;
}

export const ProxyDashboardModal: FC<ProxyDashboardModalProps> = ({
  isOpen,
  onClose,
  host,
  port,
}) => {
  const { snapshot, connected } = useProxyDashboard({ host, port: isOpen ? port : null });

  return (
    <Modal
      open={isOpen}
      onClose={onClose}
      title="Proxy Dashboard"
      description={
        connected ? `Live · ${snapshot?.total_requests ?? 0} total requests` : 'Connecting…'
      }
      size="lg"
    >
      <div className="flex flex-col gap-lg">
        <ActiveConnectionsSection snapshot={snapshot} />

        <section>
          <h3 className="text-xs font-semibold uppercase text-text-secondary mb-sm">Prompt Cache</h3>
          <ProxyCachePanel cache={snapshot?.cache} />
        </section>

        <section>
          <h3 className="text-xs font-semibold uppercase text-text-secondary mb-sm">
            Agent Cache (Council · GUI Chat)
          </h3>
          {/*
            A separate population from the proxied figure above: council and
            GUI-chat runs talk to llama-server directly, so their reuse profile
            is nothing like a user's conversation and must not be averaged in.
          */}
          <CacheUsageRows usage={snapshot?.agent_usage} />
        </section>

        <InferenceSlotsSection snapshot={snapshot} />
      </div>
    </Modal>
  );
};

export default ProxyDashboardModal;
