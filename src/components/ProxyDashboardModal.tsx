/**
 * ProxyDashboardModal.
 *
 * Self-contained live view of a running proxy's dashboard: active
 * connections (with per-request prompt-processing progress bars), VRAM
 * residency and the admission queue, and inference slots (with per-slot
 * context-usage donuts), backed by `useProxyDashboard()`'s SSE subscription.
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
import { ProxyAdmissionPanel } from './ProxyAdmissionPanel';
import { CacheUsageRows, ProxyCachePanel } from './ProxyCachePanel';
import { ProxyLaunchPanel } from './ProxyLaunchPanel';
import { ProxySamplingPanel } from './ProxySamplingPanel';
import { ActiveConnectionsSection, InferenceSlotsSection } from './proxy';
import { useProxyDashboard } from '../hooks/useProxyDashboard';

export interface ProxyDashboardModalProps {
  isOpen: boolean;
  onClose: () => void;
  host: string;
  port: number;
  /** The proxy's API key, when it requires one. See `useProxyDashboard`. */
  apiKey?: string | null;
}

export const ProxyDashboardModal: FC<ProxyDashboardModalProps> = ({
  isOpen,
  onClose,
  host,
  port,
  apiKey,
}) => {
  const { snapshot, connected } = useProxyDashboard({
    host,
    port: isOpen ? port : null,
    apiKey,
  });

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
      <div className="flex flex-col gap-lg tabular-nums">
        <ActiveConnectionsSection snapshot={snapshot} />

        {/*
          Directly under active connections, because it is the answer to the
          question those connections raise: a request sitting at "queued" is
          either waiting on llama.cpp or waiting on a model swap, and only this
          panel can tell the user which.
        */}
        <section>
          <h3 className="text-xs font-semibold text-text mb-sm">
            VRAM Residency
          </h3>
          <ProxyAdmissionPanel admission={snapshot?.admission} />
        </section>

        <section>
          <h3 className="text-xs font-semibold text-text mb-sm">
            Launch Decisions
          </h3>
          <ProxyLaunchPanel launch={snapshot?.launch} />
        </section>

        {/*
          Directly after the launch decisions, because it is the check on
          them: those rows say what gglib decided, and this one says whether
          llama-server agrees it received it.
        */}
        <section>
          <h3 className="text-xs font-semibold text-text mb-sm">
            Sampling Readback
          </h3>
          <ProxySamplingPanel audit={snapshot?.sampling_audit} />
        </section>

        <section>
          <h3 className="text-xs font-semibold text-text mb-sm">Prompt Cache</h3>
          <ProxyCachePanel cache={snapshot?.cache} />
        </section>

        <section>
          <h3 className="text-xs font-semibold text-text mb-sm">
            Agent Cache (GUI Chat)
          </h3>
          {/*
            A separate population from the proxied figure above: GUI-chat runs
            talk to llama-server directly, so their reuse profile is nothing
            like a user's conversation and must not be averaged in.
          */}
          <CacheUsageRows usage={snapshot?.agent_usage} />
        </section>

        <InferenceSlotsSection snapshot={snapshot} />
      </div>
    </Modal>
  );
};

export default ProxyDashboardModal;
