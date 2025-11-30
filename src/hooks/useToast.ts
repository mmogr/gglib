import { useState, useCallback } from 'react';
import type { ToastData, ToastType } from '../components/Toast';

let toastIdCounter = 0;

export interface UseToastReturn {
  toasts: ToastData[];
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  dismissToast: (id: string) => void;
  clearAllToasts: () => void;
}

/**
 * Hook for managing toast notifications.
 * 
 * @example
 * const { toasts, showToast, dismissToast } = useToast();
 * 
 * // Show a success toast
 * showToast('Operation completed!', 'success');
 * 
 * // Show an error toast with custom duration
 * showToast('Something went wrong', 'error', 6000);
 * 
 * // In your component JSX
 * <ToastContainer toasts={toasts} onDismiss={dismissToast} />
 */
export function useToast(): UseToastReturn {
  const [toasts, setToasts] = useState<ToastData[]>([]);

  const showToast = useCallback((message: string, type: ToastType = 'info', duration?: number) => {
    const id = `toast-${++toastIdCounter}`;
    const newToast: ToastData = {
      id,
      message,
      type,
      duration,
    };
    setToasts((prev) => [...prev, newToast]);
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  }, []);

  const clearAllToasts = useCallback(() => {
    setToasts([]);
  }, []);

  return {
    toasts,
    showToast,
    dismissToast,
    clearAllToasts,
  };
}
