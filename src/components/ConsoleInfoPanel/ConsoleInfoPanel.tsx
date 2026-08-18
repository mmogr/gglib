import { FC, useState, useCallback } from 'react';
import { StopCircle } from 'lucide-react';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import { Tabs } from '../ui/Tabs';
import { useServerState } from '../../services/serverEvents';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { Stack } from '../primitives';
import { useServerMetrics } from './useServerMetrics';
import { useUptime } from './useUptime';
import { ServerInfoSection, ApiEndpointsSection } from './StaticSections';
import { ContextUsageSection, StatisticsSection } from './TelemetrySections';

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

/**
 * Left panel in Console view: model info, live telemetry readouts, and the
 * stop button. Polling, uptime, and the section renderings live in sibling
 * modules — this file only composes them.
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
  const [isStopping, setIsStopping] = useState(false);

  // Server state from the registry — undefined means not running. Polling
  // resumes automatically when status returns to 'running' via `server_started`.
  const serverState = useServerState(modelId);
  const isRunning = serverState?.status === 'running';

  const uptime = useUptime(startTime);
  const metrics = useServerMetrics(serverPort, isRunning);
  // One server run = one history: a restart must not inherit the last run's charts.
  const runKey = `${serverPort}:${startTime}`;

  const handleStopServer = useCallback(async () => {
    setIsStopping(true);
    try {
      await onStopServer();
    } finally {
      setIsStopping(false);
    }
  }, [onStopServer]);

  return (
    <div className="flex flex-col overflow-y-auto overflow-x-hidden relative tabular-nums flex-1 md:h-full md:min-h-0">
      <div className="p-md border-b border-border-light shrink-0">
        <div className="mb-md">
          <Tabs<ChatPageTabId>
            tabs={CHAT_PAGE_TABS}
            activeId={activeTab}
            onChange={onTabChange}
            aria-label="Chat views"
          />
        </div>

        <div className="flex items-start justify-between gap-md">
          <Stack gap="xs">
            <span className="text-xs font-medium text-text-muted">Server running</span>
            <h2 className="m-0 text-lg font-semibold text-text break-words">{modelName}</h2>
          </Stack>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col gap-lg p-md">
          <ServerInfoSection serverPort={serverPort} uptime={uptime} contextLength={contextLength} />
          <ContextUsageSection metrics={metrics} runKey={runKey} contextLength={contextLength} />
          <StatisticsSection metrics={metrics} runKey={runKey} />
          <ApiEndpointsSection />

          <section className="flex flex-col gap-sm mt-auto pt-md">
            <Button
              variant="dangerGhost"
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
  );
};

export default ConsoleInfoPanel;
