import { FC, useEffect, useState, useRef } from "react";
import { ChevronDown, ChevronUp, LayoutDashboard, Repeat2, Trash2 } from "lucide-react";
import { getTransport } from "../services/transport";
import { clearProxyCache } from "../services/clients/proxyDashboard";
import { useClickOutside } from "../hooks/useClickOutside";
import { formatError } from "../utils/errors";
import { useProxyState } from "../services/proxyRegistry";
import { useSettings } from "../hooks/useSettings";
import { Icon } from "./ui/Icon";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { Checkbox } from "./ui/Checkbox";
import { cn } from '../utils/cn';
import { Stack, Label } from './primitives';
import { useToastContext } from '../contexts/ToastContext';
import { ProxyDashboardModal } from './ProxyDashboardModal';
import { EndpointCopyBar, ProxyStatusPill, ProxyToggleButton } from './proxy';

interface ProxyConfig {
  host: string;
  port: number;
  /**
   * Explicit per-run context override. `undefined` means "no override" —
   * the backend is the single source of truth and resolves the default
   * from user settings (explicit override > settings default > built-in).
   * Only ever set when the user manually edits the field.
   */
  default_context?: number;
}
interface ProxyControlProps {
  buttonClassName?: string;
  buttonActiveClassName?: string;
  statusDotClassName?: string;
  statusDotActiveClassName?: string;
  /** Icon-only trigger for narrow headers; the label moves to title/aria-label. */
  compact?: boolean;
}

const ProxyControl: FC<ProxyControlProps> = ({
  buttonClassName,
  buttonActiveClassName,
  statusDotClassName,
  statusDotActiveClassName,
  compact = false,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const proxyState = useProxyState();
  const { settings } = useSettings();
  const [config, setConfig] = useState<ProxyConfig>({
    host: "127.0.0.1",
    port: settings?.proxyPort ?? 8080,
    // Intentionally no default_context: the backend resolves the default
    // from settings. Seeding it here would send an accidental explicit
    // override (and could freeze a stale value captured before settings
    // finished loading).
  });
  const [loading, setLoading] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [cache, setCache] = useState(false);
  const [cacheDiskGb, setCacheDiskGb] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [allowedHosts, setAllowedHosts] = useState('');
  const [pinnedModel, setPinnedModel] = useState<string | null>(null);
  const [showDashboard, setShowDashboard] = useState(false);
  const [clearing, setClearing] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const { showToast } = useToastContext();

  // Close dropdown when clicking outside
  useClickOutside(dropdownRef, () => setIsOpen(false), isOpen);

  // The registry tracks running/port; the pin only travels on the status
  // response, so fetch it when the popover opens.
  useEffect(() => {
    if (!isOpen || !proxyState.running) {
      setPinnedModel(null);
      return;
    }
    let cancelled = false;
    void getTransport()
      .getProxyStatus()
      .then((s) => {
        if (!cancelled) setPinnedModel(s.pinned_model ?? null);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [isOpen, proxyState.running]);

  const handleStart = async () => {
    try {
      setLoading(true);
      // Only include fields the user explicitly set — omitting them lets
      // the backend resolve defaults from settings.
      const { default_context, ...rest } = config;
      const hosts = allowedHosts
        .split(',')
        .map((h) => h.trim())
        .filter(Boolean);
      const diskGb = parseInt(cacheDiskGb, 10);
      await getTransport().startProxy({
        ...rest,
        ...(default_context !== undefined ? { default_context } : {}),
        ...(cache ? { cache: true } : {}),
        ...(cache && Number.isFinite(diskGb) && diskGb > 0 ? { cache_disk_gb: diskGb } : {}),
        ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
        ...(hosts.length > 0 ? { allowed_hosts: hosts } : {}),
      });
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

  const activePort = proxyState.port ?? config.port;

  const handleCopied = () => showToast('Proxy URL copied to clipboard!', 'success');

  // Neutral trigger: running state reads from the green dot, not a filled pill.
  const buttonClasses = cn(
    buttonClassName ?? 'gap-sm px-md relative',
    proxyState.running && (buttonActiveClassName ?? 'text-text'),
  );

  const dotClasses = cn(
    statusDotClassName ?? 'w-2 h-2 rounded-full bg-success animate-pulse',
    proxyState.running && statusDotActiveClassName,
  );

  return (
    <div className="relative inline-flex" ref={dropdownRef}>
      <Button
        variant="ghost"
        className={buttonClasses}
        onClick={() => setIsOpen(!isOpen)}
        type="button"
        title={compact ? 'Proxy' : undefined}
        aria-label={compact ? 'Proxy' : undefined}
      >
        <span aria-hidden>
          <Icon icon={Repeat2} size={16} />
        </span>
        {!compact && <span>Proxy</span>}
        {proxyState.running && <span className={dotClasses}></span>}
      </Button>

      {isOpen && (
        <div className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 min-w-[min(350px,calc(100vw-32px))] max-h-[calc(100vh-100px)] overflow-y-auto bg-surface-elevated rounded-lg shadow-xl p-base z-dropdown text-text phone:absolute phone:top-[calc(100%+var(--spacing-sm))] phone:right-0 phone:left-auto phone:translate-x-0 phone:translate-y-0 phone:min-w-[350px] phone:max-h-[80vh] phone:overflow-y-auto">
          <div className="flex justify-between items-center mb-base pb-md border-b border-border-light">
            <h3 className="m-0 text-lg font-semibold text-text">OpenAI Proxy</h3>
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
                  <EndpointCopyBar host={config.host} port={activePort} onCopied={handleCopied} />
                </Stack>
              </div>

              <Button
                variant="secondary"
                className="w-full p-sm mb-md rounded-base text-sm font-medium"
                onClick={() => setShowDashboard(true)}
                leftIcon={<Icon icon={LayoutDashboard} size={14} />}
              >
                View Dashboard
              </Button>

              <Button
                variant="secondary"
                className="w-full p-sm mb-md rounded-base text-sm font-medium"
                onClick={async () => {
                  setClearing(true);
                  try {
                    const result = await clearProxyCache(
                      config.host,
                      activePort,
                      settings?.proxyApiKey
                    );
                    showToast(result.message || 'Cache cleared', 'success');
                  } catch (err) {
                    showToast(`Failed to clear cache: ${formatError(err)}`, 'error');
                  } finally {
                    setClearing(false);
                  }
                }}
                disabled={clearing}
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
              {showSettings && (
                <div className="mb-md">
                  <div className="mb-md">
                    <Label size="xs" muted className="mb-xs" htmlFor="proxy-host">Host</Label>
                    <Input
                      id="proxy-host"
                      type="text"
                      value={config.host}
                      onChange={(e) => setConfig({ ...config, host: e.target.value })}
                    />
                  </div>
                  <div className="mb-md">
                    <Label size="xs" muted className="mb-xs" htmlFor="proxy-port">Proxy port</Label>
                    <Input
                      id="proxy-port"
                      type="number"
                      className="font-mono tabular-nums"
                      value={config.port}
                      onChange={(e) => setConfig({ ...config, port: parseInt(e.target.value) })}
                    />
                  </div>
                  <div className="mb-md">
                    <Label size="xs" muted className="mb-xs" htmlFor="proxy-default-context">Default context</Label>
                    <Input
                      id="proxy-default-context"
                      type="number"
                      className="font-mono tabular-nums"
                      value={config.default_context ?? ''}
                      placeholder={`${settings?.defaultContextSize ?? 'server default'} (from settings)`}
                      onChange={(e) => {
                        const parsed = parseInt(e.target.value);
                        setConfig({
                          ...config,
                          default_context: Number.isNaN(parsed) ? undefined : parsed,
                        });
                      }}
                    />
                  </div>
                  <div className="mb-md">
                    <Checkbox
                      checked={cache}
                      onChange={(e) => setCache(e.target.checked)}
                      label="Persist KV cache slots to disk"
                      description="Keeps prompt caches across model swaps and restarts."
                    />
                    {cache && (
                      <div className="mt-sm">
                        <Label size="xs" muted className="mb-xs" htmlFor="proxy-cache-disk">Disk budget (GiB)</Label>
                        <Input
                          id="proxy-cache-disk"
                          type="number"
                          min={1}
                          className="font-mono tabular-nums"
                          value={cacheDiskGb}
                          placeholder="Default budget"
                          onChange={(e) => setCacheDiskGb(e.target.value)}
                        />
                      </div>
                    )}
                  </div>
                  <div className="mb-md">
                    <Label size="xs" muted className="mb-xs" htmlFor="proxy-api-key">API key</Label>
                    <Input
                      id="proxy-api-key"
                      type="text"
                      value={apiKey}
                      placeholder="None — endpoint is open on this host"
                      onChange={(e) => setApiKey(e.target.value)}
                    />
                  </div>
                  <div>
                    <Label size="xs" muted className="mb-xs" htmlFor="proxy-allowed-hosts">Allowed hosts</Label>
                    <Input
                      id="proxy-allowed-hosts"
                      type="text"
                      value={allowedHosts}
                      placeholder="Comma-separated, beyond loopback"
                      onChange={(e) => setAllowedHosts(e.target.value)}
                    />
                  </div>
                </div>
              )}

              <Button
                variant="outline"
                className="w-full mb-md"
                onClick={() => setShowSettings(!showSettings)}
                leftIcon={<Icon icon={showSettings ? ChevronUp : ChevronDown} size={14} />}
              >
                Connection options
              </Button>

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
        host={config.host}
        port={activePort}
        apiKey={settings?.proxyApiKey}
      />
    </div>
  );
};

export default ProxyControl;
