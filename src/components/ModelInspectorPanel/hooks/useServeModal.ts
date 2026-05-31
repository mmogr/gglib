import { useState, useEffect, useCallback } from 'react';
import type { InferenceConfig } from '../../../types';

export interface ServeModalState {
  showServeModal: boolean;
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  /** null = auto-detect from 'mtp' tag; 0 = explicitly disabled; >0 = explicitly enabled */
  mtpNMaxOverride: number | null;
  /** null = use default 0.75; only meaningful when mtpNMaxOverride is set */
  mtpPMinOverride: number | null;
  isServing: boolean;
  inferenceParams: InferenceConfig | undefined;
  setShowServeModal: (show: boolean) => void;
  setCustomContext: (context: string) => void;
  setCustomPort: (port: string) => void;
  setJinjaOverride: (override: boolean | null) => void;
  setMtpNMaxOverride: (override: number | null) => void;
  setMtpPMinOverride: (override: number | null) => void;
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
  const [mtpNMaxOverride, setMtpNMaxOverride] = useState<number | null>(null);
  const [mtpPMinOverride, setMtpPMinOverride] = useState<number | null>(null);
  const [isServing, setIsServing] = useState(false);
  const [inferenceParams, setInferenceParams] = useState<InferenceConfig | undefined>(undefined);

  // Reset overrides and inference params when model changes
  useEffect(() => {
    setJinjaOverride(null);
    setMtpNMaxOverride(null);
    setMtpPMinOverride(null);
    setInferenceParams(undefined);
  }, [modelId]);

  const openServeModal = useCallback(() => {
    setJinjaOverride(null);
    setMtpNMaxOverride(null);
    setMtpPMinOverride(null);
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
    setMtpNMaxOverride(null);
    setMtpPMinOverride(null);
    setIsServing(false);
    setInferenceParams(undefined);
  }, []);

  return {
    showServeModal,
    customContext,
    customPort,
    jinjaOverride,
    mtpNMaxOverride,
    mtpPMinOverride,
    isServing,
    inferenceParams,
    setShowServeModal,
    setCustomContext,
    setCustomPort,
    setJinjaOverride,
    setMtpNMaxOverride,
    setMtpPMinOverride,
    setIsServing,
    setInferenceParams,
    openServeModal,
    closeServeModal,
    resetServeModal,
  };
}
