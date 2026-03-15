import React from 'react';
import { AlertTriangle } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';

interface ChatAlertsProps {
  chatError: string | null;
  isServerConnected: boolean;
  onClose?: () => void;
}

export const ChatAlerts: React.FC<ChatAlertsProps> = ({
  chatError,
  isServerConnected,
  onClose,
}) => (
  <>
    {chatError && (
      <div className="py-sm px-md bg-danger/10 border border-danger rounded-sm text-danger text-sm shrink-0">
        {chatError}
      </div>
    )}

    {!isServerConnected && (
      <div className="flex items-center justify-between gap-md py-sm px-md bg-[var(--color-warning-alpha,rgba(255,193,7,0.1))] border border-[var(--color-warning,#ffc107)] rounded-sm text-[var(--color-warning-text,#856404)] text-sm shrink-0">
        <span className="inline-flex items-center gap-2">
          <Icon icon={AlertTriangle} size={16} />
          Server not running — Chat is read-only
        </span>
        {onClose && (
          <Button variant="secondary" size="sm" onClick={onClose}>
            Close
          </Button>
        )}
      </div>
    )}
  </>
);
