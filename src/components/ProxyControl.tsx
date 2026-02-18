import { FC, useState, useEffect, useRef } from "react";
import { ClipboardCopy, Power, Repeat2 } from "lucide-react";
import { getProxyStatus, startProxy, stopProxy } from "../services/clients/servers";
import { setProxyState } from "../services/platform";
import { useClickOutside } from "../hooks/useClickOutside";
import { Icon } from "./ui/Icon";
import { Button } from "./ui/Button";
import { Input } from "./ui/Input";
import { cn } from '../utils/cn';

interface ProxyStatus {
  running: boolean;
  port: number;
  current_model?: string;
  model_port?: number;
}

interface ProxyConfig {
  host: string;
  port: number;
  default_context: number;
}
interface ProxyControlProps {
  buttonClassName?: string;
  buttonActiveClassName?: string;
  statusDotClassName?: string;
  statusDotActiveClassName?: string;
}

const ProxyControl: FC<ProxyControlProps> = ({
  buttonClassName,
  buttonActiveClassName,
  statusDotClassName,
  statusDotActiveClassName,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [status, setStatus] = useState<ProxyStatus>({ running: false, port: 8080 });
  const [config, setConfig] = useState<ProxyConfig>({
    host: "127.0.0.1",
    port: 8080,
    default_context: 8192,
  });
  const [loading, setLoading] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 3000);
    return () => clearInterval(interval);
  }, []);

  // Close dropdown when clicking outside
  useClickOutside(dropdownRef, () => setIsOpen(false), isOpen);

  const loadStatus = async () => {
    try {
      const proxyStatus = await getProxyStatus();
      setStatus(proxyStatus);
    } catch {
      setStatus({ running: false, port: config.port });
    }
  };

  const handleStart = async () => {
    try {
      setLoading(true);
      const proxyStatus = await startProxy(config);
      await setProxyState(true, proxyStatus.port);
      await loadStatus();
    } catch (err) {
      alert(`Failed to start proxy: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    try {
      setLoading(true);
      await stopProxy();
      await setProxyState(false, null);
      await loadStatus();
    } catch (err) {
      alert(`Failed to stop proxy: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const copyProxyUrl = () => {
    const url = `http://${config.host}:${status.port}/v1`;
    navigator.clipboard.writeText(url);
    alert("Proxy URL copied to clipboard!");
  };

  const buttonClasses = cn(
    buttonClassName ?? 'flex items-center gap-sm px-base py-sm bg-[rgba(255,255,255,0.1)] border border-[rgba(255,255,255,0.2)] rounded-md text-white cursor-pointer text-sm font-medium transition-all relative hover:bg-[rgba(255,255,255,0.15)]',
    status.running && (buttonActiveClassName ?? 'bg-[rgba(76,175,80,0.3)] border-[rgba(76,175,80,0.5)]'),
  );

  const dotClasses = cn(
    statusDotClassName ?? 'w-2 h-2 rounded-full bg-success animate-pulse',
    status.running && statusDotActiveClassName,
  );

  return (
    <div className="relative inline-flex" ref={dropdownRef}>
      <button
        className={buttonClasses}
        onClick={() => setIsOpen(!isOpen)}
        type="button"
      >
        <span className="proxy-icon" aria-hidden>
          <Icon icon={Repeat2} size={16} />
        </span>
        <span className="proxy-label">Proxy</span>
        {status.running && <span className={dotClasses}></span>}
      </button>

      {isOpen && (
        <div className="absolute top-[calc(100%+var(--spacing-sm))] right-0 min-w-[350px] bg-background-overlay rounded-lg shadow-xl p-base z-dropdown text-text max-tablet:fixed max-tablet:top-1/2 max-tablet:left-1/2 max-tablet:right-auto max-tablet:-translate-x-1/2 max-tablet:-translate-y-1/2 max-tablet:min-w-[min(350px,calc(100vw-32px))] max-tablet:max-h-[calc(100vh-100px)] max-tablet:overflow-y-auto">
          <div className="flex justify-between items-center mb-base pb-md border-b border-border">
            <h3 className="m-0 text-lg text-text">OpenAI Proxy</h3>
            <span className={cn(
              'px-md py-xs rounded-lg text-xs font-semibold uppercase',
              status.running
                ? 'bg-[color-mix(in_srgb,var(--color-success)_15%,transparent)] text-success'
                : 'bg-[color-mix(in_srgb,var(--color-danger)_15%,transparent)] text-danger'
            )}>
              {status.running ? 'Running' : 'Stopped'}
            </span>
          </div>

          {status.running ? (
            <>
              <div className="mb-base">
                <div className="flex flex-col gap-xs mb-sm">
                  <label className="text-xs font-semibold text-text-secondary uppercase">URL:</label>
                  <div className="flex gap-sm items-center">
                    <code className="flex-1 bg-surface-elevated p-sm rounded-base text-sm border border-border font-mono">http://{config.host}:{status.port}/v1</code>
                    <Button 
                      variant="ghost"
                      size="sm"
                      onClick={copyProxyUrl}
                      className="bg-primary border-none rounded-base p-sm cursor-pointer text-base text-white transition-all hover:bg-primary-hover hover:scale-105"
                      title="Copy URL"
                      iconOnly
                    >
                      <Icon icon={ClipboardCopy} size={14} />
                    </Button>
                  </div>
                </div>
                {status.current_model && (
                  <div className="flex flex-col gap-xs">
                    <label className="text-xs font-semibold text-text-secondary uppercase">Current Model:</label>
                    <span>{status.current_model}</span>
                  </div>
                )}
              </div>

              <Button
                variant="danger"
                className="w-full p-md border-none rounded-md text-sm font-semibold cursor-pointer transition-all bg-danger text-white hover:bg-danger-hover disabled:opacity-60 disabled:cursor-not-allowed"
                onClick={handleStop}
                disabled={loading}
                leftIcon={<Icon icon={Power} size={14} />}
              >
                {loading ? 'Stopping...' : 'Stop Proxy'}
              </Button>
            </>
          ) : (
            <>
              {showSettings && (
                <div className="mb-md">
                  <div className="mb-md">
                    <label className="block text-xs font-semibold text-text-secondary mb-xs uppercase">Host:</label>
                    <Input
                      type="text"
                      value={config.host}
                      onChange={(e) => setConfig({ ...config, host: e.target.value })}
                    />
                  </div>
                  <div className="mb-md">
                    <label className="block text-xs font-semibold text-text-secondary mb-xs uppercase">Proxy Port:</label>
                    <Input
                      type="number"
                      value={config.port}
                      onChange={(e) => setConfig({ ...config, port: parseInt(e.target.value) })}
                    />
                  </div>
                  <div>
                    <label className="block text-xs font-semibold text-text-secondary mb-xs uppercase">Default Context:</label>
                    <Input
                      type="number"
                      value={config.default_context}
                      onChange={(e) => setConfig({ ...config, default_context: parseInt(e.target.value) })}
                    />
                  </div>
                </div>
              )}

              <Button
                variant="ghost"
                className="w-full p-sm bg-transparent border border-border rounded-base cursor-pointer text-sm text-text-secondary mb-md transition-all hover:bg-surface-hover"
                onClick={() => setShowSettings(!showSettings)}
              >
                {showSettings ? '▲ Hide' : '▼ Show'} Settings
              </Button>

              <Button
                variant="primary"
                className="w-full p-md border-none rounded-md text-sm font-semibold cursor-pointer transition-all bg-primary text-white hover:bg-primary-hover disabled:opacity-60 disabled:cursor-not-allowed"
                onClick={handleStart}
                disabled={loading}
                leftIcon={<Icon icon={Power} size={14} />}
              >
                {loading ? 'Starting...' : 'Start Proxy'}
              </Button>

              <div className="mt-md pt-md border-t border-border">
                <small className="text-text-muted text-xs leading-normal">
                  Configure OpenWebUI or other OpenAI-compatible clients to use this proxy.
                  Models will auto-swap based on requests.
                </small>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
};

export default ProxyControl;
