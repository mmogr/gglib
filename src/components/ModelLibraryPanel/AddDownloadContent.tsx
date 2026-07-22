import { FC, useState } from 'react';
import { FolderOpen, Search } from 'lucide-react';
import AddModel from '../AddModel';
import { HuggingFaceBrowser } from '../HuggingFaceBrowser';
import { HfModelSummary } from '../../types';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';

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
      {/* px-base matches the gutter on the search row and list rows. Without
          it this control bled to the panel's left edge and clipped its icon. */}
      <div className="flex gap-sm px-base py-sm border-b border-border shrink-0">
        <Button
          variant={activeSubTab === 'browse' ? 'primary' : 'secondary'}
          size="sm"
          className="flex-1 min-w-0"
          onClick={() => handleSubTabChange('browse')}
          leftIcon={<Icon icon={Search} size={14} />}
          aria-pressed={activeSubTab === 'browse'}
        >
          Browse HF
        </Button>
        <Button
          variant={activeSubTab === 'add' ? 'primary' : 'secondary'}
          size="sm"
          className="flex-1 min-w-0"
          onClick={() => handleSubTabChange('add')}
          leftIcon={<Icon icon={FolderOpen} size={14} />}
          aria-pressed={activeSubTab === 'add'}
        >
          Local File
        </Button>
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
