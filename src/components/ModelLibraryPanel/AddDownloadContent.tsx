import { FC, useState } from 'react';
import AddModel from '../AddModel';
import DownloadModel from '../DownloadModel';
import { HuggingFaceBrowser } from '../HuggingFaceBrowser';
import { HfModelSummary } from '../../types';
import './AddDownloadContent.css';

export type AddDownloadSubTab = 'add' | 'download' | 'browse';

interface AddDownloadContentProps {
  onModelAdded: (filePath: string) => Promise<void>;
  onModelDownloaded: () => Promise<void>;
  activeSubTab?: AddDownloadSubTab;
  onSubTabChange?: (subtab: AddDownloadSubTab) => void;
  /** Callback when an HF model is selected for preview */
  onSelectHfModel?: (model: HfModelSummary | null) => void;
  /** Currently selected HF model ID */
  selectedHfModelId?: string | null;
}

const AddDownloadContent: FC<AddDownloadContentProps> = ({
  onModelAdded,
  onModelDownloaded,
  activeSubTab: externalActiveSubTab,
  onSubTabChange,
  onSelectHfModel,
  selectedHfModelId,
}) => {
  const [internalActiveSubTab, setInternalActiveSubTab] = useState<AddDownloadSubTab>('download');
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

  const handleModelDownloaded = async () => {
    await onModelDownloaded();
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
          className={`add-download-subtab ${activeSubTab === 'download' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('download')}
        >
          ⬇️ Direct URL
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
            onDownloadStarted={handleModelDownloaded} 
            onDownloadCompleted={handleModelDownloaded}
            onSelectModel={onSelectHfModel}
            selectedModelId={selectedHfModelId}
          />
        )}
        {activeSubTab === 'download' && (
          <DownloadModel onModelDownloaded={handleModelDownloaded} />
        )}
        {activeSubTab === 'add' && (
          <AddModel onModelAdded={handleModelAdded} />
        )}
      </div>
    </div>
  );
};

export default AddDownloadContent;
