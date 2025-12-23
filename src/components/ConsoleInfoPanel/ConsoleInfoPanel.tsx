import { FC, useState, useEffect, useCallback, useRef } from 'react';
import { Copy, StopCircle } from 'lucide-react';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import SidebarTabs from '../ModelLibraryPanel/SidebarTabs';
import { useServerState } from '../../services/serverEvents';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import './ConsoleInfoPanel.css';

interface ConsoleInfoPanelProps {
  modelId: number;
  modelName: string;
  serverPort: number;
  contextLength?: number;
  startTime: number; // Unix timestamp in seconds
  onStopServer: () => Promise<void>;
  activeTab: ChatPageTabId;
  onTabChange: (tab: ChatPageTabId) => void;
}

interface ServerMetrics {
  kvCacheUsageRatio: number | null;
  kvCacheTokens: number | null;
  nTokensMax: number;
  promptTokensTotal: number;
  predictedTokensTotal: number;
  requestsProcessing: number;
}

/**
 * Format uptime duration in a human-readable way
 */
const formatUptime = (startTime: number): string => {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - startTime;

  if (diff < 60) {
    return `${diff}s`;
  } else if (diff < 3600) {
    const mins = Math.floor(diff / 60);
    const secs = diff % 60;
    return `${mins}m ${secs}s`;
  } else {
    const hours = Math.floor(diff / 3600);
    const mins = Math.floor((diff % 3600) / 60);
    return `${hours}h ${mins}m`;
  }
};

/**
 * Left panel in Console view showing model info, context usage, and stop button.
 */
const ConsoleInfoPanel: FC<ConsoleInfoPanelProps> = ({
  modelId,
  modelName,
  serverPort,
  contextLength,
  startTime,
  onStopServer,
  activeTab,
  onTabChange,
}) => {
  const [uptime, setUptime] = useState(() => formatUptime(startTime));
  const [metrics, setMetrics] = useState<ServerMetrics | null>(null);
  const [isStopping, setIsStopping] = useState(false);
  const uptimeIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Get server state from registry - undefined means not running
  // Polling resumes automatically when status changes to 'running' via server:running event
  const serverState = useServerState(modelId);
  const isRunning = serverState?.status === 'running';

  // Update uptime every second - sync to wall clock to prevent flicker
  useEffect(() => {
    // Clear any existing interval
    if (uptimeIntervalRef.current) {
      clearInterval(uptimeIntervalRef.current);
    }
    
    // Initial update
    setUptime(formatUptime(startTime));
    
    // Calculate ms until next whole second to sync updates
    const msUntilNextSecond = 1000 - (Date.now() % 1000);
    
    // First, wait until next whole second, then start interval
    const timeout = setTimeout(() => {
      setUptime(formatUptime(startTime));
      uptimeIntervalRef.current = setInterval(() => {
        setUptime(formatUptime(startTime));
      }, 1000);
    }, msUntilNextSecond);
    
    return () => {
      clearTimeout(timeout);
      if (uptimeIntervalRef.current) {
        clearInterval(uptimeIntervalRef.current);
      }
    };
  }, [startTime]);

  // Poll server metrics using setTimeout recursion + AbortController
  // Only polls when server status is 'running'
  // On fetch failure: stops local loop, clears metrics, does not affect global state
  // Polling resumes automatically when status changes to 'running' via server:running event
  useEffect(() => {
    // Don't poll if server is not running
    if (!isRunning) {
      setMetrics(null);
      return;
    }

    let cancelled = false;
    const controller = new AbortController();

    const fetchMetrics = async (): Promise<void> => {
      try {
        const response = await fetch(`http://127.0.0.1:${serverPort}/metrics`, {
          signal: controller.signal,
        });
        if (response.ok && !cancelled) {
          const text = await response.text();
          const parsed = parsePrometheusMetrics(text);
          setMetrics(parsed);
        }
      } catch (error) {
        // On any error (including abort), clear metrics and stop polling for this effect instance
        if (!cancelled) {
          setMetrics(null);
          // Don't schedule next tick on error - effect will re-run when isRunning changes
          return;
        }
      }

      // Schedule next fetch if not cancelled and still running
      if (!cancelled && isRunning) {
        setTimeout(fetchMetrics, 2000);
      }
    };

    // Start polling immediately
    fetchMetrics();

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [serverPort, isRunning]);

  const handleStopServer = useCallback(async () => {
    setIsStopping(true);
    try {
      await onStopServer();
    } finally {
      setIsStopping(false);
    }
  }, [onStopServer]);

  const contextUsagePercent = metrics?.kvCacheUsageRatio != null 
    ? Math.round(metrics.kvCacheUsageRatio * 100) 
    : metrics?.nTokensMax != null && contextLength 
      ? Math.round((metrics.nTokensMax / contextLength) * 100)
      : null;

  return (
    <div className="mcc-panel console-info-panel">
      <div className="mcc-panel-header">
        {/* View Tabs */}
        <div className="console-info-tabs">
          <SidebarTabs<ChatPageTabId>
            tabs={CHAT_PAGE_TABS}
            activeTab={activeTab}
            onTabChange={onTabChange}
          />
        </div>

        <div className="console-info-header">
          <div className="console-info-title-group">
            <span className="console-info-label">Server running</span>
            <h2 className="console-info-title">{modelName}</h2>
          </div>
        </div>
      </div>

      <div className="mcc-panel-content">
        <div className="console-info-content">
          {/* Server Info Section */}
          <section className="console-info-section">
            <h3>Server Info</h3>
            <div className="console-info-grid">
              <div className="console-info-row">
                <span className="console-info-key">Port</span>
                <span className="console-info-value">
                  <code>{serverPort}</code>
                  <Button
                    iconOnly
                    size="sm"
                    variant="ghost"
                    onClick={() => navigator.clipboard.writeText(`http://127.0.0.1:${serverPort}`)}
                    title="Copy server URL"
                  >
                    <Icon icon={Copy} size={14} />
                  </Button>
                </span>
              </div>
              <div className="console-info-row">
                <span className="console-info-key">Uptime</span>
                <span className="console-info-value">{uptime}</span>
              </div>
              {contextLength && (
                <div className="console-info-row">
                  <span className="console-info-key">Context Size</span>
                  <span className="console-info-value">{contextLength.toLocaleString()} tokens</span>
                </div>
              )}
            </div>
          </section>

          {/* Context Usage Section */}
          <section className="console-info-section">
            <h3>Context Usage</h3>
            {contextUsagePercent !== null ? (
              <div className="console-info-context-usage">
                <div className="context-usage-bar">
                  <div 
                    className="context-usage-fill"
                    style={{ width: `${Math.min(100, contextUsagePercent)}%` }}
                  />
                </div>
                <div className="context-usage-stats">
                  <span className="context-usage-percent">{contextUsagePercent}%</span>
                  {(metrics?.kvCacheTokens != null || metrics?.nTokensMax != null) && (
                    <span className="context-usage-tokens">
                      {(metrics.kvCacheTokens ?? metrics.nTokensMax ?? 0).toLocaleString()} tokens
                    </span>
                  )}
                </div>
              </div>
            ) : (
              <p className="console-info-hint">
                No usage yet
              </p>
            )}
          </section>

          {/* Request Stats Section */}
          {metrics && (
            <section className="console-info-section">
              <h3>Statistics</h3>
              <div className="console-info-grid">
                <div className="console-info-row">
                  <span className="console-info-key">Prompt Tokens</span>
                  <span className="console-info-value">{metrics.promptTokensTotal.toLocaleString()}</span>
                </div>
                <div className="console-info-row">
                  <span className="console-info-key">Generated Tokens</span>
                  <span className="console-info-value">{metrics.predictedTokensTotal.toLocaleString()}</span>
                </div>
                <div className="console-info-row">
                  <span className="console-info-key">Active Requests</span>
                  <span className="console-info-value">{metrics.requestsProcessing}</span>
                </div>
              </div>
            </section>
          )}

          {/* API Endpoints Section */}
          <section className="console-info-section">
            <h3>API Endpoints</h3>
            <div className="console-info-endpoints">
              <div className="console-info-endpoint">
                <code>POST /v1/chat/completions</code>
                <span className="endpoint-hint">OpenAI-compatible chat</span>
              </div>
              <div className="console-info-endpoint">
                <code>POST /v1/completions</code>
                <span className="endpoint-hint">Text completion</span>
              </div>
              <div className="console-info-endpoint">
                <code>GET /health</code>
                <span className="endpoint-hint">Health check</span>
              </div>
            </div>
          </section>

          {/* Stop Server Button */}
          <section className="console-info-section console-info-actions">
            <Button
              variant="danger"
              size="lg"
              onClick={handleStopServer}
              isLoading={isStopping}
              leftIcon={!isStopping ? <Icon icon={StopCircle} size={18} /> : undefined}
            >
              {isStopping ? 'Stopping...' : 'Stop Server'}
            </Button>
          </section>
        </div>
      </div>
    </div>
  );
};

/**
 * Parse Prometheus-format metrics text into structured data
 */
function parsePrometheusMetrics(text: string): ServerMetrics {
  const getMetricValue = (name: string): number | null => {
    const regex = new RegExp(`^${name}\\s+([\\d.]+)`, 'm');
    const match = text.match(regex);
    return match ? parseFloat(match[1]) : null;
  };

  return {
    kvCacheUsageRatio: getMetricValue('llamacpp:kv_cache_usage_ratio'),
    kvCacheTokens: getMetricValue('llamacpp:kv_cache_tokens'),
    nTokensMax: getMetricValue('llamacpp:n_tokens_max') ?? 0,
    promptTokensTotal: getMetricValue('llamacpp:prompt_tokens_total') ?? 0,
    predictedTokensTotal: getMetricValue('llamacpp:tokens_predicted_total') ?? 0,
    requestsProcessing: getMetricValue('llamacpp:requests_processing') ?? 0,
  };
}

export default ConsoleInfoPanel;
