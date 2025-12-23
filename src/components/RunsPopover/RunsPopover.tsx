import { FC, useRef, useEffect } from 'react';
import { RotateCcw } from 'lucide-react';
import { ServerInfo } from '../../types';
import { ServerList } from '../ServerList';
import { useClickOutside } from '../../hooks/useClickOutside';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import './RunsPopover.css';

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
    <div className="runs-popover" ref={popoverRef}>
      <div className="runs-popover-header">
        <span className="runs-popover-title">Running Servers</span>
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
      <div className="runs-popover-content">
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
