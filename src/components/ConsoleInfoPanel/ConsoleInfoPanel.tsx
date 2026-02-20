import { FC, useState, useEffect, useCallback, useRef } from 'react';
import { Copy, StopCircle } from 'lucide-react';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import SidebarTabs from '../ModelLibraryPanel/SidebarTabs';
import { useServerState } from '../../services/serverEvents';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';
import { Stack } from '../primitives';

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
  // Polling resumes automatically when status changes to 'running' via server:started event
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
  // Polling resumes automatically when status changes to 'running' via server:started event
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
    <div className="flex flex-col h-full min-h-0 overflow-y-auto overflow-x-hidden border-r border-border relative flex-1 max-md:h-auto max-md:max-h-none max-md:border-r-0 max-md:border-b max-md:border-border">
      <div className="p-base border-b border-border bg-background shrink-0">
        {/* View Tabs */}
        <div className="mb-md">
          <SidebarTabs<ChatPageTabId>
            tabs={CHAT_PAGE_TABS}
            activeTab={activeTab}
            onTabChange={onTabChange}
          />
        </div>

        <div className="flex items-start justify-between gap-md">
          <Stack gap="xs">
            <span className="text-xs text-text-muted uppercase tracking-[0.05em]">Server running</span>
            <h2 className="m-0 text-lg font-semibold text-text break-words">{modelName}</h2>
          </Stack>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col">
        <div className="flex flex-col gap-lg">
          {/* Server Info Section */}
          <section className="flex flex-col gap-sm">
            <h3 className="m-0 text-sm font-semibold text-text-muted uppercase tracking-[0.05em]">Server Info</h3>
            <Stack gap="xs">
              <div className="flex justify-between items-center gap-sm py-xs">
                <span className="text-sm text-text-muted">Port</span>
                <span className="text-sm text-text flex items-center gap-xs [&_code]:bg-background [&_code]:py-[2px] [&_code]:px-[6px] [&_code]:rounded-xs [&_code]:font-mono [&_code]:text-xs">
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
              <div className="flex justify-between items-center gap-sm py-xs">
                <span className="text-sm text-text-muted">Uptime</span>
                <span className="text-sm text-text">{uptime}</span>
              </div>
              {contextLength && (
                <div className="flex justify-between items-center gap-sm py-xs">
                  <span className="text-sm text-text-muted">Context Size</span>
                  <span className="text-sm text-text">{contextLength.toLocaleString()} tokens</span>
                </div>
              )}
            </Stack>
          </section>

          {/* Context Usage Section */}
          <section className="flex flex-col gap-sm">
            <h3 className="m-0 text-sm font-semibold text-text-muted uppercase tracking-[0.05em]">Context Usage</h3>
            {contextUsagePercent !== null ? (
              <Stack gap="xs">
                <div className="h-[8px] bg-background rounded-sm overflow-hidden">
                  <div 
                    className={cn(
                      "h-full rounded-sm transition-[width] duration-300 ease-out bg-gradient-to-r from-primary to-primary-hover",
                      contextUsagePercent >= 80 && "from-warning to-warning-hover"
                    )}
                    style={{ width: `${Math.min(100, contextUsagePercent)}%` }}
                  />
                </div>
                <div className="flex justify-between items-center">
                  <span className="text-lg font-semibold text-text">{contextUsagePercent}%</span>
                  {(metrics?.kvCacheTokens != null || metrics?.nTokensMax != null) && (
                    <span className="text-xs text-text-muted">
                      {(metrics.kvCacheTokens ?? metrics.nTokensMax ?? 0).toLocaleString()} tokens
                    </span>
                  )}
                </div>
              </Stack>
            ) : (
              <p className="text-xs text-text-muted m-0">
                No usage yet
              </p>
            )}
          </section>

          {/* Request Stats Section */}
          {metrics && (
            <section className="flex flex-col gap-sm">
              <h3 className="m-0 text-sm font-semibold text-text-muted uppercase tracking-[0.05em]">Statistics</h3>
              <Stack gap="xs">
                <div className="flex justify-between items-center gap-sm py-xs">
                  <span className="text-sm text-text-muted">Prompt Tokens</span>
                  <span className="text-sm text-text">{metrics.promptTokensTotal.toLocaleString()}</span>
                </div>
                <div className="flex justify-between items-center gap-sm py-xs">
                  <span className="text-sm text-text-muted">Generated Tokens</span>
                  <span className="text-sm text-text">{metrics.predictedTokensTotal.toLocaleString()}</span>
                </div>
                <div className="flex justify-between items-center gap-sm py-xs">
                  <span className="text-sm text-text-muted">Active Requests</span>
                  <span className="text-sm text-text">{metrics.requestsProcessing}</span>
                </div>
              </Stack>
            </section>
          )}

          {/* API Endpoints Section */}
          <section className="flex flex-col gap-sm">
            <h3 className="m-0 text-sm font-semibold text-text-muted uppercase tracking-[0.05em]">API Endpoints</h3>
            <Stack gap="xs">
              <div className="flex flex-col gap-[2px] py-xs px-sm bg-background rounded-sm [&_code]:font-mono [&_code]:text-xs [&_code]:text-text">
                <code>POST /v1/chat/completions</code>
                <span className="text-[10px] text-text-muted">OpenAI-compatible chat</span>
              </div>
              <div className="flex flex-col gap-[2px] py-xs px-sm bg-background rounded-sm [&_code]:font-mono [&_code]:text-xs [&_code]:text-text">
                <code>POST /v1/completions</code>
                <span className="text-[10px] text-text-muted">Text completion</span>
              </div>
              <div className="flex flex-col gap-[2px] py-xs px-sm bg-background rounded-sm [&_code]:font-mono [&_code]:text-xs [&_code]:text-text">
                <code>GET /health</code>
                <span className="text-[10px] text-text-muted">Health check</span>
              </div>
            </Stack>
          </section>

          {/* Stop Server Button */}
          <section className="flex flex-col gap-sm mt-auto pt-md">
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
