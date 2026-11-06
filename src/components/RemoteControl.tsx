import { FC, useCallback, useEffect } from 'react';
import { useRemoteState } from '../services/remoteRegistry';
import { refreshRemoteStatus } from '../services/remoteEvents';
import { useToastContext } from '../contexts/ToastContext';
import { cn } from '../utils/cn';
import { DevicesSection, JoinSection, ServeSection } from './remote';

interface RemotePopoverProps {
  open: boolean;
  /** Where the panel sits relative to the line that owns it. */
  className?: string;
}

/**
 * Today's remote panel (ADR 0012): this machine's proxy on another machine,
 * or another machine's on this one. Anchored by the sidebar's devices line,
 * which owns the open state and the click-outside, until the Devices place
 * replaces both (V1).
 */
export const RemotePopover: FC<RemotePopoverProps> = ({ open, className }) => {
  const { status } = useRemoteState();
  const { showToast } = useToastContext();

  // Opening asks the daemon again: the status is cheap and the panel is the
  // one place the fields the events do not carry are read.
  useEffect(() => {
    if (open) refreshRemoteStatus();
  }, [open]);

  // Memoised because the sections take it as a prop and put it in dependency
  // arrays; a fresh closure every render makes every one of those a no-op.
  const notice = useCallback(
    (message: string, kind: 'success' | 'error' | 'info') => showToast(message, kind),
    [showToast],
  );

  if (!open) return null;

  return (
    <div
      className={cn(
        'min-w-[min(380px,calc(100vw-32px))] max-h-[calc(100vh-100px)] overflow-y-auto bg-surface-elevated rounded-lg shadow-xl p-base z-dropdown text-text',
        className,
      )}
    >
      <div className="flex justify-between items-center mb-base pb-md border-b border-border-light">
        <h3 className="m-0 text-base font-semibold text-text">Remote</h3>
        <span className="text-xs text-text-muted">end-to-end encrypted, no account</span>
      </div>

      {status === null ? (
        <p className="text-xs text-text-muted m-0">Asking the daemon…</p>
      ) : (
        <>
          <ServeSection onNotice={notice} />
          <div className="my-base border-t border-border-light" />
          <DevicesSection onNotice={notice} />
          <div className="my-base border-t border-border-light" />
          <JoinSection onNotice={notice} />
        </>
      )}

      <div className="mt-md pt-md border-t border-border-light">
        <small className="text-text-muted text-xs leading-normal">
          A key per device: retiring one leaves every other connected, and no device's key
          ever reaches the proxy behind the tunnel. Remote access stays on across a restart;
          the connection to another machine does not.
        </small>
      </div>
    </div>
  );
};
