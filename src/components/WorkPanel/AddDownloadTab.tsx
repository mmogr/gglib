import { FC, useState } from 'react';
import AddModel from '../AddModel';
import DownloadModel from '../DownloadModel';
import { HuggingFaceBrowser } from '../HuggingFaceBrowser';
import './AddDownloadTab.css';

interface AddDownloadTabProps {
  onModelAdded: (filePath: string) => Promise<void>;
  onModelDownloaded: () => Promise<void>;
  activeSubTab?: 'add' | 'download' | 'browse';
  onSubTabChange?: (subtab: 'add' | 'download' | 'browse') => void;
}

const AddDownloadTab: FC<AddDownloadTabProps> = ({
  onModelAdded,
  onModelDownloaded,
  activeSubTab: externalActiveSubTab,
  onSubTabChange,
}) => {
  const [internalActiveSubTab, setInternalActiveSubTab] = useState<'add' | 'download' | 'browse'>('browse');
  const activeSubTab = externalActiveSubTab ?? internalActiveSubTab;
  
  const handleSubTabChange = (subtab: 'add' | 'download' | 'browse') => {
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
    <div className="add-download-tab">
      <div className="subtab-switcher">
        <button
          className={`subtab-button btn ${activeSubTab === 'browse' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('browse')}
        >
          🔍 Browse HF
        </button>
        <button
          className={`subtab-button btn ${activeSubTab === 'download' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('download')}
        >
          ⬇️ Direct Download
        </button>
        <button
          className={`subtab-button btn ${activeSubTab === 'add' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('add')}
        >
          📁 Add from file
        </button>
      </div>

      <div className="subtab-content">
        {activeSubTab === 'browse' && (
          <HuggingFaceBrowser onDownloadStarted={handleModelDownloaded} />
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

export default AddDownloadTab;
