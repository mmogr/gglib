import { FC, useState } from 'react';
import AddModel from '../AddModel';
import { HuggingFaceBrowser } from '../HuggingFaceBrowser';
import { HfModelSummary } from '../../types';
import { cn } from '../../utils/cn';

export type AddDownloadSubTab = 'add' | 'browse';

interface AddDownloadContentProps {
  onModelAdded: (filePath: string) => Promise<void>;
  activeSubTab?: AddDownloadSubTab;
  onSubTabChange?: (subtab: AddDownloadSubTab) => void;
  /** Optional error message if the backend download system failed to initialize */
  downloadSystemError?: string | null;
  /** Callback when an HF model is selected for preview */
  onSelectHfModel?: (model: HfModelSummary | null) => void;
  /** Currently selected HF model ID */
  selectedHfModelId?: string | null;
}

const AddDownloadContent: FC<AddDownloadContentProps> = ({
  onModelAdded,
  activeSubTab: externalActiveSubTab,
  onSubTabChange,
  downloadSystemError,
  onSelectHfModel,
  selectedHfModelId,
}) => {
  const [internalActiveSubTab, setInternalActiveSubTab] = useState<AddDownloadSubTab>('browse');
  const activeSubTab = externalActiveSubTab ?? internalActiveSubTab;
  
  const handleSubTabChange = (subtab: AddDownloadSubTab) => {
    if (onSubTabChange) {
      onSubTabChange(subtab);
    } else {
      setInternalActiveSubTab(subtab);
    }
  };

  const handleModelAdded = async () => {
    await onModelAdded('');
  };

  return (
    <div className="flex flex-col h-full min-h-0">
      {downloadSystemError && (
        <div className="px-2.5 py-2 border border-border rounded-md mb-2.5">
          <strong>Downloads unavailable.</strong>
          <div className="mt-1 whitespace-pre-wrap">{downloadSystemError}</div>
        </div>
      )}
      <div className="flex flex-wrap gap-sm py-sm border-b border-border shrink-0 max-mobile:flex-col max-mobile:flex-nowrap">
        <button
          className={cn(
            'flex-auto min-w-0 bg-background border border-border rounded-md text-text cursor-pointer text-sm font-medium transition-all overflow-hidden text-ellipsis whitespace-nowrap px-xs py-sm hover:bg-background-hover max-mobile:w-full max-mobile:text-center max-mobile:whitespace-normal',
            activeSubTab === 'browse' && 'bg-primary text-white border-primary',
          )}
          onClick={() => handleSubTabChange('browse')}
        >
          🔍 Browse HF
        </button>
        <button
          className={cn(
            'flex-auto min-w-0 bg-background border border-border rounded-md text-text cursor-pointer text-sm font-medium transition-all overflow-hidden text-ellipsis whitespace-nowrap px-xs py-sm hover:bg-background-hover max-mobile:w-full max-mobile:text-center max-mobile:whitespace-normal',
            activeSubTab === 'add' && 'bg-primary text-white border-primary',
          )}
          onClick={() => handleSubTabChange('add')}
        >
          📁 Local File
        </button>
      </div>

      <div className="flex-1 overflow-y-auto py-base min-h-0">
        {activeSubTab === 'browse' && (
          <HuggingFaceBrowser 
            onSelectModel={onSelectHfModel}
            selectedModelId={selectedHfModelId}
          />
        )}
        {activeSubTab === 'add' && (
          <AddModel onModelAdded={handleModelAdded} />
        )}
      </div>
    </div>
  );
};

export default AddDownloadContent;
