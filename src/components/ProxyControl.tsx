import { FC, useState, useEffect, useRef } from "react";
import { ClipboardCopy, Power, Repeat2 } from "lucide-react";
import { getProxyStatus, startProxy, stopProxy } from "../services/clients/servers";
import { setProxyState } from "../services/platform";
import { useClickOutside } from "../hooks/useClickOutside";
import { Icon } from "./ui/Icon";
import styles from './ProxyControl.module.css';

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

  const buttonClasses = [
    buttonClassName ?? styles.proxyButton,
    status.running ? (buttonActiveClassName ?? styles.proxyButtonActive) : '',
  ].filter(Boolean).join(' ');

  const dotClasses = [
    statusDotClassName ?? styles.statusDot,
    status.running ? statusDotActiveClassName ?? '' : '',
  ].filter(Boolean).join(' ');

  return (
    <div className={styles.proxyControl} ref={dropdownRef}>
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
        <div className={styles.proxyDropdown}>
          <div className={styles.dropdownHeader}>
            <h3>OpenAI Proxy</h3>
            <span className={`${styles.statusBadge} ${status.running ? styles.running : styles.stopped}`}>
              {status.running ? 'Running' : 'Stopped'}
            </span>
          </div>

          {status.running ? (
            <>
              <div className={styles.proxyInfo}>
                <div className={styles.infoRow}>
                  <label>URL:</label>
                  <div className={styles.urlDisplay}>
                    <code>http://{config.host}:{status.port}/v1</code>
                    <button onClick={copyProxyUrl} className={styles.copyButton} title="Copy URL">
                      <Icon icon={ClipboardCopy} size={14} />
                    </button>
                  </div>
                </div>
                {status.current_model && (
                  <div className={styles.infoRow}>
                    <label>Current Model:</label>
                    <span>{status.current_model}</span>
                  </div>
                )}
              </div>

              <button
                className={`${styles.actionButton} ${styles.stop}`}
                onClick={handleStop}
                disabled={loading}
              >
                <span className="inline-flex items-center gap-2">
                  <Icon icon={Power} size={14} />
                  {loading ? 'Stopping...' : 'Stop Proxy'}
                </span>
              </button>
            </>
          ) : (
            <>
              {showSettings && (
                <div className={styles.settingsSection}>
                  <div className={styles.formGroup}>
                    <label>Host:</label>
                    <input
                      type="text"
                      value={config.host}
                      onChange={(e) => setConfig({ ...config, host: e.target.value })}
                    />
                  </div>
                  <div className={styles.formGroup}>
                    <label>Proxy Port:</label>
                    <input
                      type="number"
                      value={config.port}
                      onChange={(e) => setConfig({ ...config, port: parseInt(e.target.value) })}
                    />
                  </div>
                  <div className={styles.formGroup}>
                    <label>Default Context:</label>
                    <input
                      type="number"
                      value={config.default_context}
                      onChange={(e) => setConfig({ ...config, default_context: parseInt(e.target.value) })}
                    />
                  </div>
                </div>
              )}

              <button
                className={styles.settingsToggle}
                onClick={() => setShowSettings(!showSettings)}
              >
                {showSettings ? '▲ Hide' : '▼ Show'} Settings
              </button>

              <button
                className={`${styles.actionButton} ${styles.start}`}
                onClick={handleStart}
                disabled={loading}
              >
                <span className="inline-flex items-center gap-2">
                  <Icon icon={Power} size={14} />
                  {loading ? 'Starting...' : 'Start Proxy'}
                </span>
              </button>

              <div className={styles.helpText}>
                <small>
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
