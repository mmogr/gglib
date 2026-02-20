import { FC, useState } from 'react';
import { RotateCcw, ServerOff, Square } from 'lucide-react';
import { appLogger } from '../../services/platform';
import { ServerInfo } from '../../types';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import SidebarTabs from '../ModelLibraryPanel/SidebarTabs';
import { ServerHealthIndicator } from '../ServerHealthIndicator';
import { Row } from '../primitives';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';

interface ServerListProps {
  servers: ServerInfo[];
  onStopServer: (modelId: number) => Promise<void>;
  onSelectModel?: (modelId: number, view?: 'chat' | 'console') => void;
  /** Compact mode for popover display */
  compact?: boolean;
  /** Show header with count and refresh button */
  showHeader?: boolean;
  onRefresh?: () => void;
}

const ServerList: FC<ServerListProps> = ({
  servers,
  onStopServer,
  onSelectModel,
  compact = false,
  showHeader = false,
  onRefresh,
}) => {
  // Track which server has expanded tabs (only one at a time)
  const [expandedServerId, setExpandedServerId] = useState<number | null>(null);

  const handleStop = async (modelId: number, e: React.MouseEvent) => {
    e.stopPropagation(); // Prevent triggering onSelectModel
    try {
      await onStopServer(modelId);
    } catch (error) {
      appLogger.error('component.server', 'Failed to stop server', { error, modelId });
    }
  };

  const handleServerClick = (modelId: number) => {
    // Toggle expanded state for this server
    setExpandedServerId(prev => prev === modelId ? null : modelId);
  };

  const handleTabSelect = (modelId: number, tab: ChatPageTabId) => {
    onSelectModel?.(modelId, tab);
  };

  if (servers.length === 0) {
    return (
      <div className={cn("flex flex-col items-center justify-center text-center text-text-muted", compact ? "p-base" : "p-xl")}>
        <div className="text-2xl mb-sm opacity-50" aria-hidden>
          <Icon icon={ServerOff} size={22} />
        </div>
        <p className="my-xs text-sm">No active servers</p>
        {!compact && <p className="my-xs text-xs opacity-70">Start a model to see it here</p>}
      </div>
    );
  }

  return (
    <div className={cn("flex flex-col", compact && "gap-xs")}>
      {showHeader && (
        <div className="flex justify-between items-center pb-sm border-b border-border mb-sm">
          <span className="text-sm font-semibold text-text">
            Active Servers ({servers.length})
          </span>
          {onRefresh && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onRefresh}
              title="Refresh servers"
              iconOnly
            >
              <Icon icon={RotateCcw} size={14} />
            </Button>
          )}
        </div>
      )}

      <div className="flex flex-col gap-sm">
        {servers.map((server) => (
          <div
            key={server.modelId}
            className={cn("flex flex-col gap-0 bg-background border border-border rounded-md transition duration-200 overflow-hidden", expandedServerId === server.modelId && "border-primary")}
          >
            <div 
              className={cn("flex justify-between items-center gap-sm py-sm px-md", onSelectModel && "cursor-pointer hover:bg-background-hover")}
              onClick={() => handleServerClick(server.modelId)}
            >
              <div className="flex-1 min-w-0">
                <Row gap="sm" align="center" className="font-medium text-sm overflow-hidden text-ellipsis whitespace-nowrap">
                  {server.modelName}
                  <ServerHealthIndicator modelId={server.modelId} />
                </Row>
                <div className="flex items-center gap-sm text-xs text-text-muted mt-xs">
                  <span className="font-mono">:{server.port}</span>
                  <span className="text-success font-medium">{server.status}</span>
                </div>
              </div>
              <Button
                variant="danger"
                size="sm"
                className={cn("shrink-0 !bg-transparent !border !border-border !text-text hover:!bg-danger hover:!border-danger hover:!text-white", compact ? "!p-xs" : "!py-xs !px-sm")}
                onClick={(e) => handleStop(server.modelId, e)}
                title="Stop server"
                leftIcon={<Icon icon={Square} size={14} />}
              >
                {!compact && 'Stop'}
              </Button>
            </div>
            {expandedServerId === server.modelId && onSelectModel && (
              <div className="border-t border-border bg-background-secondary">
                <SidebarTabs<ChatPageTabId>
                  tabs={CHAT_PAGE_TABS}
                  activeTab="chat"
                  onTabChange={(tab) => handleTabSelect(server.modelId, tab)}
                />
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};

export default ServerList;
