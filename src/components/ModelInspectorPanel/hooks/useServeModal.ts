import { useState, useEffect, useCallback } from 'react';

export interface ServeModalState {
  showServeModal: boolean;
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  isServing: boolean;
  setShowServeModal: (show: boolean) => void;
  setCustomContext: (context: string) => void;
  setCustomPort: (port: string) => void;
  setJinjaOverride: (override: boolean | null) => void;
  setIsServing: (serving: boolean) => void;
  openServeModal: () => void;
  closeServeModal: () => void;
  resetServeModal: () => void;
}

/**
 * Hook for managing serve modal state and configuration.
 */
export function useServeModal(modelId: number | undefined): ServeModalState {
  const [showServeModal, setShowServeModal] = useState(false);
  const [customContext, setCustomContext] = useState('');
  const [customPort, setCustomPort] = useState('');
  const [jinjaOverride, setJinjaOverride] = useState<boolean | null>(null);
  const [isServing, setIsServing] = useState(false);

  // Reset jinja override when model changes
  useEffect(() => {
    setJinjaOverride(null);
  }, [modelId]);

  const openServeModal = useCallback(() => {
    setJinjaOverride(null);
    setShowServeModal(true);
  }, []);

  const closeServeModal = useCallback(() => {
    setShowServeModal(false);
  }, []);

  const resetServeModal = useCallback(() => {
    setShowServeModal(false);
    setCustomContext('');
    setCustomPort('');
    setJinjaOverride(null);
    setIsServing(false);
  }, []);

  return {
    showServeModal,
    customContext,
    customPort,
    jinjaOverride,
    isServing,
    setShowServeModal,
    setCustomContext,
    setCustomPort,
    setJinjaOverride,
    setIsServing,
    openServeModal,
    closeServeModal,
    resetServeModal,
  };
}
