import { FC, useRef, useEffect } from 'react';
import { RotateCcw } from 'lucide-react';
import { ServerInfo } from '../../types';
import { ServerList } from '../ServerList';
import { useClickOutside } from '../../hooks/useClickOutside';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';

interface RunsPopoverProps {
  isOpen: boolean;
  onClose: () => void;
  servers: ServerInfo[];
  onStopServer: (modelId: number) => Promise<void>;
  onSelectModel: (modelId: number, view?: 'chat' | 'console') => void;
  onRefresh?: () => void;
}

const RunsPopover: FC<RunsPopoverProps> = ({
  isOpen,
  onClose,
  servers,
  onStopServer,
  onSelectModel,
  onRefresh,
}) => {
  const popoverRef = useRef<HTMLDivElement>(null);

  // Close when clicking outside
  useClickOutside(popoverRef, onClose, isOpen);

  // Auto-close when last server stops
  useEffect(() => {
    if (isOpen && servers.length === 0) {
      onClose();
    }
  }, [isOpen, servers.length, onClose]);

  const handleSelectModel = (modelId: number, view?: 'chat' | 'console') => {
    onSelectModel(modelId, view);
    onClose();
  };

  const handleStopServer = async (modelId: number) => {
    await onStopServer(modelId);
    // Popover will auto-close via the useEffect if this was the last server
  };

  if (!isOpen) return null;

  return (
    <div className="absolute top-full right-0 mt-xs bg-surface border border-border rounded-md shadow-[0_4px_16px_rgba(0,0,0,0.3)] min-w-[280px] max-w-[360px] z-[1000] overflow-hidden" ref={popoverRef}>
      <div className="flex items-center justify-between px-md py-sm border-b border-border bg-surface-elevated">
        <span className="text-sm font-semibold text-text">Running Servers</span>
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
      <div className="max-h-[300px] overflow-y-auto scrollbar-thin">
        <ServerList
          servers={servers}
          onStopServer={handleStopServer}
          onSelectModel={handleSelectModel}
          compact
        />
      </div>
    </div>
  );
};

export default RunsPopover;
