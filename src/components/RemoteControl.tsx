import { FC, useEffect, useRef, useState } from 'react';
import { Globe } from 'lucide-react';
import { useClickOutside } from '../hooks/useClickOutside';
import { useRemoteState } from '../services/remoteRegistry';
import { refreshRemoteStatus } from '../services/remoteEvents';
import { useToastContext } from '../contexts/ToastContext';
import { Icon } from './ui/Icon';
import { Button } from './ui/Button';
import { cn } from '../utils/cn';
import { ConnectSection, ServeSection } from './remote';

interface RemoteControlProps {
  buttonClassName?: string;
  buttonActiveClassName?: string;
  statusDotClassName?: string;
  statusDotActiveClassName?: string;
  /** Icon-only trigger for narrow headers; the label moves to title/aria-label. */
  compact?: boolean;
}

/**
 * The remote tunnel's popover (ADR 0012): this machine's proxy on another
 * machine, or another machine's on this one. A sibling of `ProxyControl` in
 * the header, with the same trigger shape; the dot is lit when either side
 * is up.
 */
const RemoteControl: FC<RemoteControlProps> = ({
  buttonClassName,
  buttonActiveClassName,
  statusDotClassName,
  statusDotActiveClassName,
  compact = false,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const { status } = useRemoteState();
  const dropdownRef = useRef<HTMLDivElement>(null);
  const { showToast } = useToastContext();

  useClickOutside(dropdownRef, () => setIsOpen(false), isOpen);

  // Opening asks the daemon again: the status is cheap and the panel is the
  // one place the fields the events do not carry are read.
  useEffect(() => {
    if (isOpen) refreshRemoteStatus();
  }, [isOpen]);

  const active = (status?.enabled ?? false) || status?.connected != null;

  const buttonClasses = cn(
    buttonClassName ?? 'gap-sm px-md relative',
    active && (buttonActiveClassName ?? 'text-text'),
  );
  const dotClasses = cn(
    statusDotClassName ?? 'w-2 h-2 rounded-full bg-success animate-pulse',
    active && statusDotActiveClassName,
  );

  const notice = (message: string, kind: 'success' | 'error' | 'info') => showToast(message, kind);

  return (
    <div className="relative inline-flex" ref={dropdownRef}>
      <Button
        variant="ghost"
        className={buttonClasses}
        onClick={() => setIsOpen(!isOpen)}
        type="button"
        title={compact ? 'Remote' : undefined}
        aria-label={compact ? 'Remote' : undefined}
      >
        <span aria-hidden>
          <Icon icon={Globe} size={16} />
        </span>
        {!compact && <span>Remote</span>}
        {active && <span className={dotClasses}></span>}
      </Button>

      {isOpen && (
        <div className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 min-w-[min(380px,calc(100vw-32px))] max-h-[calc(100vh-100px)] overflow-y-auto bg-surface-elevated rounded-lg shadow-xl p-base z-dropdown text-text phone:absolute phone:top-[calc(100%+var(--spacing-sm))] phone:right-0 phone:left-auto phone:translate-x-0 phone:translate-y-0 phone:min-w-[380px] phone:max-h-[80vh] phone:overflow-y-auto">
          <div className="flex justify-between items-center mb-base pb-md border-b border-border-light">
            <h3 className="m-0 text-lg font-semibold text-text">Remote</h3>
            <span className="text-xs text-text-muted">end-to-end encrypted, no account</span>
          </div>

          {status === null ? (
            <p className="text-xs text-text-muted m-0">Asking the daemon…</p>
          ) : (
            <>
              <ServeSection onNotice={notice} />
              <div className="my-base border-t border-border-light" />
              <ConnectSection onNotice={notice} />
            </>
          )}

          <div className="mt-md pt-md border-t border-border-light">
            <small className="text-text-muted text-xs leading-normal">
              One key, two doors: the tunnel enforces the same API key the proxy does. Nothing is
              persisted — a restart forgets both sides.
            </small>
          </div>
        </div>
      )}
    </div>
  );
};

export default RemoteControl;
