import { FC, useState, useEffect, useCallback } from 'react';
import './ConsoleInfoPanel.css';

interface ConsoleInfoPanelProps {
  modelName: string;
  serverPort: number;
  contextLength?: number;
  startTime: number; // Unix timestamp in seconds
  onStopServer: () => Promise<void>;
}

interface ServerMetrics {
  kvCacheUsageRatio: number;
  kvCacheTokens: number;
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
  modelName,
  serverPort,
  contextLength,
  startTime,
  onStopServer,
}) => {
  const [uptime, setUptime] = useState(() => formatUptime(startTime));
  const [metrics, setMetrics] = useState<ServerMetrics | null>(null);
  const [isStopping, setIsStopping] = useState(false);

  // Update uptime every second
  useEffect(() => {
    const interval = setInterval(() => {
      setUptime(formatUptime(startTime));
    }, 1000);
    return () => clearInterval(interval);
  }, [startTime]);

  // Poll server metrics
  useEffect(() => {
    const fetchMetrics = async () => {
      try {
        // Try to fetch from llama-server's /metrics endpoint
        const response = await fetch(`http://127.0.0.1:${serverPort}/metrics`);
        if (response.ok) {
          const text = await response.text();
          // Parse Prometheus format
          const parsed = parsePrometheusMetrics(text);
          setMetrics(parsed);
        }
      } catch {
        // Metrics endpoint may not be enabled - that's okay
        setMetrics(null);
      }
    };

    fetchMetrics();
    const interval = setInterval(fetchMetrics, 2000);
    return () => clearInterval(interval);
  }, [serverPort]);

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
    : null;

  return (
    <div className="mcc-panel console-info-panel">
      <div className="mcc-panel-header">
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
                  <button
                    className="icon-btn icon-btn-sm"
                    onClick={() => navigator.clipboard.writeText(`http://127.0.0.1:${serverPort}`)}
                    title="Copy server URL"
                  >
                    📋
                  </button>
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
                  {metrics?.kvCacheTokens != null && (
                    <span className="context-usage-tokens">
                      {metrics.kvCacheTokens.toLocaleString()} tokens
                    </span>
                  )}
                </div>
              </div>
            ) : (
              <p className="console-info-hint">
                Enable <code>--metrics</code> flag to see live context usage
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
            <button
              className="btn btn-lg btn-danger"
              onClick={handleStopServer}
              disabled={isStopping}
            >
              {isStopping ? '⏳ Stopping...' : '⏹️ Stop Server'}
            </button>
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
  const getMetricValue = (name: string): number => {
    const regex = new RegExp(`^${name}\\s+([\\d.]+)`, 'm');
    const match = text.match(regex);
    return match ? parseFloat(match[1]) : 0;
  };

  return {
    kvCacheUsageRatio: getMetricValue('llamacpp:kv_cache_usage_ratio'),
    kvCacheTokens: getMetricValue('llamacpp:kv_cache_tokens'),
    promptTokensTotal: getMetricValue('llamacpp:prompt_tokens_total'),
    predictedTokensTotal: getMetricValue('llamacpp:tokens_predicted_total'),
    requestsProcessing: getMetricValue('llamacpp:requests_processing'),
  };
}

export default ConsoleInfoPanel;
