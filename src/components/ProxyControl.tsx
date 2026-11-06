import { FC, useState } from "react";
import { LayoutDashboard, Trash2 } from "lucide-react";
import { getTransport } from "../services/transport";
import { clearProxyCache } from "../services/clients/proxyDashboard";
import { formatError } from "../utils/errors";
import { useEndpointFacts, useProxyState } from "../services/proxyRegistry";
import { useSettings } from "../hooks/useSettings";
import { useProxyKey } from "../hooks/useProxyKey";
import { Icon } from "./ui/Icon";
import { Button } from "./ui/Button";
import { cn } from '../utils/cn';
import { Stack, Label } from './primitives';
import { useToastContext } from '../contexts/ToastContext';
import { ProxyDashboardModal } from './ProxyDashboardModal';
import { EndpointCopyBar, ProxyStatusPill, ProxyToggleButton, reachableHost } from './proxy';

interface ProxyPopoverProps {
  /** Whether the panel is drawn; the dashboard it opens outlives the panel. */
  open: boolean;
  onClose: () => void;
  /** Where the panel sits relative to the line that owns it. */
  className?: string;
}

/**
 * Today's proxy panel: the endpoint's URL to copy, the dashboard, Clear
 * Cache and Turn on / off. Anchored by the sidebar's endpoint line, which
 * owns the open state and the click-outside, until the Endpoint place
 * replaces both (N1). The address it dials is the GUI's reading of the
 * stored bind address, as before; the line above it reads the daemon's.
 */
export const ProxyPopover: FC<ProxyPopoverProps> = ({ open, onClose, className }) => {
  const proxyState = useProxyState();
  const { pinnedModel } = useEndpointFacts();
  const { settings } = useSettings();
  // Where this app reaches the endpoint: the stored address is where it
  // BINDS, and a wildcard is nowhere a client can send, so that is reached on
  // loopback; one interface's address is reached on itself. The daemon's own
  // rule, until this panel goes with the Endpoint place (N1).
  const endpointHost = reachableHost(settings?.proxyHost ?? null);
  const [loading, setLoading] = useState(false);
  const [showDashboard, setShowDashboard] = useState(false);
  const [clearing, setClearing] = useState(false);
  const { showToast } = useToastContext();
  // Clear Cache presents the proxy's key; revealed while the popover is open.
  const proxyKey = useProxyKey(open && proxyState.running, settings);

  const handleStart = async () => {
    try {
      setLoading(true);
      // Nothing is named: the daemon starts the endpoint as the settings
      // say (address, port, allowed hosts, prompt cache), the same start the
      // tray and `gglib proxy` with no flags get.
      await getTransport().startProxy({});
    } catch (err) {
      showToast(`Failed to start proxy: ${formatError(err)}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    try {
      setLoading(true);
      await getTransport().stopProxy();
    } catch (err) {
      showToast(`Failed to stop proxy: ${formatError(err)}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  // The port a start without one gets: the stored one, else the default.
  const daemonPort = settings?.proxyPort ?? 8080;
  const activePort = proxyState.port ?? settings?.proxyPort ?? daemonPort;

  const handleCopied = () => showToast('Proxy URL copied to clipboard!', 'success');

  return (
    <>
      {open && (
        <div
          className={cn(
            'min-w-[min(350px,calc(100vw-32px))] max-h-[calc(100vh-100px)] overflow-y-auto bg-surface-elevated rounded-lg shadow-xl p-base z-dropdown text-text',
            className,
          )}
        >
          <div className="flex justify-between items-center mb-base pb-md border-b border-border-light">
            <h3 className="m-0 text-base font-semibold text-text">OpenAI Proxy</h3>
            <ProxyStatusPill running={proxyState.running} />
          </div>

          {proxyState.running ? (
            <>
              {pinnedModel && (
                <p className="text-xs text-text-muted mb-md">
                  Pinned to <span className="font-mono text-text-secondary">{pinnedModel}</span> — requests
                  naming other models are refused.
                </p>
              )}
              <div className="mb-base">
                <Stack gap="xs" className="mb-sm">
                  <Label size="xs" muted>Endpoint URL</Label>
                  <EndpointCopyBar host={endpointHost} port={activePort} onCopied={handleCopied} />
                </Stack>
              </div>

              <Button
                variant="secondary"
                className="w-full p-sm mb-md rounded-base text-sm font-semibold"
                onClick={() => { onClose(); setShowDashboard(true); }}
                leftIcon={<Icon icon={LayoutDashboard} size={14} />}
              >
                View Dashboard
              </Button>

              <Button
                variant="secondary"
                className="w-full p-sm mb-md rounded-base text-sm font-semibold"
                onClick={async () => {
                  setClearing(true);
                  try {
                    const result = await clearProxyCache(endpointHost, activePort, proxyKey);
                    showToast(result.message || 'Cache cleared', 'success');
                  } catch (err) {
                    showToast(`Failed to clear cache: ${formatError(err)}`, 'error');
                  } finally {
                    setClearing(false);
                  }
                }}
                // Waits for the reveal, as the dashboard does: sent before it
                // answers, no key goes, and a keyed proxy refuses.
                disabled={clearing || proxyKey === undefined}
                leftIcon={<Icon icon={Trash2} size={14} />}
              >
                {clearing ? 'Clearing...' : 'Clear Cache'}
              </Button>

              <ProxyToggleButton
                running
                pending={loading}
                onStart={handleStart}
                onStop={handleStop}
              />
            </>
          ) : (
            <>
              <ProxyToggleButton
                running={false}
                pending={loading}
                onStart={handleStart}
                onStop={handleStop}
              />

              <div className="mt-md pt-md border-t border-border-light">
                <small className="text-text-muted text-xs leading-normal">
                  Configure OpenWebUI or other OpenAI-compatible clients to use this proxy.
                  Models will auto-swap based on requests.
                </small>
              </div>
            </>
          )}
        </div>
      )}

      <ProxyDashboardModal
        isOpen={showDashboard}
        onClose={() => setShowDashboard(false)}
        host={endpointHost}
        port={activePort}
      />
    </>
  );
};
