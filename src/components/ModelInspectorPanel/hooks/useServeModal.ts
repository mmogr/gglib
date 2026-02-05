import { useState, useEffect, useCallback } from 'react';
import type { InferenceConfig } from '../../../types';

export interface ServeModalState {
  showServeModal: boolean;
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  isServing: boolean;
  inferenceParams: InferenceConfig | undefined;
  setShowServeModal: (show: boolean) => void;
  setCustomContext: (context: string) => void;
  setCustomPort: (port: string) => void;
  setJinjaOverride: (override: boolean | null) => void;
  setIsServing: (serving: boolean) => void;
  setInferenceParams: (params: InferenceConfig | undefined) => void;
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
  const [inferenceParams, setInferenceParams] = useState<InferenceConfig | undefined>(undefined);

  // Reset jinja override and inference params when model changes
  useEffect(() => {
    setJinjaOverride(null);
    setInferenceParams(undefined);
  }, [modelId]);

  const openServeModal = useCallback(() => {
    setJinjaOverride(null);
    setInferenceParams(undefined);
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
    setInferenceParams(undefined);
  }, []);

  return {
    showServeModal,
    customContext,
    customPort,
    jinjaOverride,
    isServing,
    inferenceParams,
    setShowServeModal,
    setCustomContext,
    setCustomPort,
    setJinjaOverride,
    setIsServing,
    setInferenceParams,
    openServeModal,
    closeServeModal,
    resetServeModal,
  };
}
