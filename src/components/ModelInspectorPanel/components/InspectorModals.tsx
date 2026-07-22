import { FC } from 'react';
import type { GgufModel } from '../../../types';
import type { LlamaServerNotInstalledMetadata } from '../../../services/transport/errors';
import { LlamaInstallModal } from '../../LlamaInstallModal';
import { VerificationModal } from '../../VerificationModal';
import type { UseInspectorModalsResult } from '../hooks/useInspectorModals';

interface InspectorModalsProps {
  model: GgufModel;
  modals: UseInspectorModalsResult;
}

/**
 * The inspector's three secondary modals: llama-server install prompt,
 * shard verification, and HuggingFace update check.
 *
 * Serve and delete modals stay in the panel — they are wired into the
 * server-action flow rather than being standalone.
 */
export const InspectorModals: FC<InspectorModalsProps> = ({ model, modals }) => (
  <>
    {modals.showInstallModal && modals.installMetadata && (
      <LlamaInstallModal
        isOpen={modals.showInstallModal}
        onClose={modals.closeInstallModal}
        metadata={modals.installMetadata as LlamaServerNotInstalledMetadata}
      />
    )}

    {model.id != null && (
      <>
        <VerificationModal
          modelId={model.id}
          modelName={model.name}
          open={modals.showVerifyModal}
          onClose={modals.closeVerifyModal}
          mode="verify"
        />
        <VerificationModal
          modelId={model.id}
          modelName={model.name}
          open={modals.showUpdateModal}
          onClose={modals.closeUpdateModal}
          mode="update"
        />
      </>
    )}
  </>
);
