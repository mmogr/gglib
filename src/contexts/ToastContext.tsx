import { createContext, FC, ReactNode, useCallback, useContext, useState } from 'react';
import type { ToastData, ToastType } from '../components/Toast';

let toastIdCounter = 0;

/**
 * Minimal toast context API - just what consumers need.
 */
export interface ToastContextValue {
  toasts: ToastData[];
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  dismissToast: (id: string) => void;
  clearToasts: () => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

interface ToastProviderProps {
  children: ReactNode;
}

/**
 * Provider component that manages global toast state.
 * All components using useToastContext() will share the same toast queue.
 */
export const ToastProvider: FC<ToastProviderProps> = ({ children }) => {
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

  const clearToasts = useCallback(() => {
    setToasts([]);
  }, []);

  const value: ToastContextValue = {
    toasts,
    showToast,
    dismissToast,
    clearToasts,
  };

  return <ToastContext.Provider value={value}>{children}</ToastContext.Provider>;
};

/**
 * Hook to access the global toast context.
 * Must be used within a ToastProvider.
 *
 * @example
 * const { showToast } = useToastContext();
 * showToast('Download complete!', 'success');
 */
export function useToastContext(): ToastContextValue {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToastContext must be used within a ToastProvider');
  }
  return context;
}
