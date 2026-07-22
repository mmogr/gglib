import { useCallback, useState } from 'react';
import type { LlamaServerNotInstalledMetadata } from '../../../services/transport/errors';

export interface UseInspectorModalsResult {
  /** llama-server install prompt, opened reactively when a start attempt fails. */
  showInstallModal: boolean;
  installMetadata: LlamaServerNotInstalledMetadata | null;
  closeInstallModal: () => void;
  /** Passed to useServerActions; opens the install modal with its metadata. */
  handleLlamaServerNotInstalled: (metadata: LlamaServerNotInstalledMetadata) => void;

  /** Shard integrity verification. */
  showVerifyModal: boolean;
  openVerifyModal: () => void;
  closeVerifyModal: () => void;

  /** HuggingFace update check. */
  showUpdateModal: boolean;
  openUpdateModal: () => void;
  closeUpdateModal: () => void;
}

/**
 * Owns the inspector's three secondary modals (install / verify / update).
 *
 * These are independent of the serve and delete flows, which have their own
 * hooks. Grouping them keeps ModelInspectorPanel free of raw useState pairs.
 */
export function useInspectorModals(): UseInspectorModalsResult {
  const [showInstallModal, setShowInstallModal] = useState(false);
  const [installMetadata, setInstallMetadata] = useState<LlamaServerNotInstalledMetadata | null>(null);
  const [showVerifyModal, setShowVerifyModal] = useState(false);
  const [showUpdateModal, setShowUpdateModal] = useState(false);

  const handleLlamaServerNotInstalled = useCallback((metadata: LlamaServerNotInstalledMetadata) => {
    setInstallMetadata(metadata);
    setShowInstallModal(true);
  }, []);

  return {
    showInstallModal,
    installMetadata,
    closeInstallModal: useCallback(() => setShowInstallModal(false), []),
    handleLlamaServerNotInstalled,

    showVerifyModal,
    openVerifyModal: useCallback(() => setShowVerifyModal(true), []),
    closeVerifyModal: useCallback(() => setShowVerifyModal(false), []),

    showUpdateModal,
    openUpdateModal: useCallback(() => setShowUpdateModal(true), []),
    closeUpdateModal: useCallback(() => setShowUpdateModal(false), []),
  };
}
