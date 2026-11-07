/**
 * The inspector's Hugging Face subject as it was before the redesign: the
 * repo's preview, with the download queue's room for its Download buttons.
 * Kept until the inspector holds a Hugging Face repo itself (I4).
 *
 * @module components/ModelInspectorPanel/HfInspector
 */

import type { FC } from 'react';
import { HfModelPreview } from '../HfModelPreview';
import type { QueueSnapshot } from '../../services/transport/types/downloads';
import type { HfModelSummary } from '../../types';
import { useHfDownload } from './hooks/useHfDownload';

export const HfInspector: FC<{ model: HfModelSummary; downloadQueue: QueueSnapshot | null }> = ({ model, downloadQueue }) => {
  const { handleHfDownload, downloadsDisabled, disabledReason } = useHfDownload(downloadQueue);
  return (
    <div className="relative flex flex-1 flex-col overflow-hidden bg-surface md:h-full md:min-h-0">
      <HfModelPreview model={model} onDownload={handleHfDownload} downloadsDisabled={downloadsDisabled} disabledReason={disabledReason} />
    </div>
  );
};
