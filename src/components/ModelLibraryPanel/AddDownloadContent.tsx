import { FC, useState } from 'react';
import { FolderOpen, Search } from 'lucide-react';
import AddModel from '../AddModel';
import { HuggingFaceBrowser } from '../HuggingFaceBrowser';
import { HfModelSummary } from '../../types';
import { Icon } from '../ui/Icon';
import { Tabs, type TabItem } from '../ui/Tabs';

export type AddDownloadSubTab = 'add' | 'browse';

const ADD_SUBTABS: TabItem<AddDownloadSubTab>[] = [
  { id: 'browse', label: 'Browse HF', icon: <Icon icon={Search} size={14} /> },
  { id: 'add', label: 'Local File', icon: <Icon icon={FolderOpen} size={14} /> },
];

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
      <Tabs
        tabs={ADD_SUBTABS}
        activeId={activeSubTab}
        onChange={handleSubTabChange}
        aria-label="Add model source"
        fill
        className="px-base shrink-0"
      />

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
