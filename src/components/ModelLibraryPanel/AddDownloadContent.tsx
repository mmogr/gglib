import { FC, useState } from 'react';
import AddModel from '../AddModel';
import { HuggingFaceBrowser } from '../HuggingFaceBrowser';
import { HfModelSummary } from '../../types';
import './AddDownloadContent.css';

export type AddDownloadSubTab = 'add' | 'browse';

interface AddDownloadContentProps {
  onModelAdded: (filePath: string) => Promise<void>;
  activeSubTab?: AddDownloadSubTab;
  onSubTabChange?: (subtab: AddDownloadSubTab) => void;
  /** Callback when an HF model is selected for preview */
  onSelectHfModel?: (model: HfModelSummary | null) => void;
  /** Currently selected HF model ID */
  selectedHfModelId?: string | null;
}

const AddDownloadContent: FC<AddDownloadContentProps> = ({
  onModelAdded,
  activeSubTab: externalActiveSubTab,
  onSubTabChange,
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
    <div className="add-download-content">
      <div className="add-download-subtabs">
        <button
          className={`add-download-subtab ${activeSubTab === 'browse' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('browse')}
        >
          🔍 Browse HF
        </button>
        <button
          className={`add-download-subtab ${activeSubTab === 'add' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('add')}
        >
          📁 Local File
        </button>
      </div>

      <div className="add-download-panel">
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
