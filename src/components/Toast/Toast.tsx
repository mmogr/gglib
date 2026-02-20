import React, { useEffect, useState, useCallback } from 'react';
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';

export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface ToastData {
  id: string;
  message: string;
  type: ToastType;
  duration?: number;
}

interface ToastItemProps {
  toast: ToastData;
  onDismiss: (id: string) => void;
}

const ToastItem: React.FC<ToastItemProps> = ({ toast, onDismiss }) => {
  const [isExiting, setIsExiting] = useState(false);

  useEffect(() => {
    const duration = toast.duration ?? 4000;
    const exitTimer = setTimeout(() => {
      setIsExiting(true);
    }, duration - 300); // Start exit animation before removal

    const removeTimer = setTimeout(() => {
      onDismiss(toast.id);
    }, duration);

    return () => {
      clearTimeout(exitTimer);
      clearTimeout(removeTimer);
    };
  }, [toast.id, toast.duration, onDismiss]);

  const handleClick = useCallback(() => {
    setIsExiting(true);
    setTimeout(() => onDismiss(toast.id), 300);
  }, [toast.id, onDismiss]);

  const icon = {
    success: CheckCircle2,
    error: XCircle,
    info: Info,
    warning: AlertTriangle,
  }[toast.type];

  return (
    <div
      className={cn(
        'flex items-center gap-sm px-md py-sm rounded-base bg-surface border border-border shadow-[0_8px_24px_rgba(0,0,0,0.22)] text-sm cursor-pointer pointer-events-auto animate-toast-enter transition-[transform,opacity] duration-300 ease-out hover:-translate-x-1',
        toast.type === 'success' && 'border-[#10b981] bg-[linear-gradient(135deg,rgba(16,185,129,0.08)_0%,var(--color-surface)_100%)]',
        toast.type === 'error' && 'border-[#ef4444] bg-[linear-gradient(135deg,rgba(239,68,68,0.08)_0%,var(--color-surface)_100%)]',
        toast.type === 'info' && 'border-primary bg-[linear-gradient(135deg,rgba(99,102,241,0.12)_0%,var(--color-surface)_100%)]',
        toast.type === 'warning' && 'border-[#f59e0b] bg-[linear-gradient(135deg,rgba(245,158,11,0.12)_0%,var(--color-surface)_100%)]',
        isExiting && 'animate-toast-exit',
      )}
      onClick={handleClick}
      role="alert"
      aria-live="polite"
    >
      <span className={cn(
        'text-base font-bold shrink-0 w-5 h-5 flex items-center justify-center',
        toast.type === 'success' && 'text-[#10b981]',
        toast.type === 'error' && 'text-[#ef4444]',
        toast.type === 'info' && 'text-primary',
        toast.type === 'warning' && 'text-[#f59e0b]',
      )}>
        <Icon icon={icon} size={16} />
      </span>
      <span className="flex-1 text-text leading-[1.4]">{toast.message}</span>
      <button
        className="bg-transparent border-none text-text-muted text-lg cursor-pointer p-0 leading-none opacity-70 transition-opacity duration-200 ease-out shrink-0 hover:opacity-100"
        aria-label="Dismiss notification"
        onClick={(event) => {
          event.stopPropagation();
          handleClick();
        }}
      >
        <Icon icon={X} size={14} />
      </button>
    </div>
  );
};

interface ToastContainerProps {
  toasts: ToastData[];
  onDismiss: (id: string) => void;
}

export const ToastContainer: React.FC<ToastContainerProps> = ({ toasts, onDismiss }) => {
  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-lg right-lg z-notification flex flex-col gap-sm max-w-[400px] pointer-events-none max-xs:left-md max-xs:right-md max-xs:bottom-md max-xs:max-w-none" aria-label="Notifications">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onDismiss={onDismiss} />
      ))}
    </div>
  );
};

export default ToastContainer;
