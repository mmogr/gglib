import { FC, useState } from 'react';
import AddModel from '../AddModel';
import DownloadModel from '../DownloadModel';
import './AddDownloadTab.css';

interface AddDownloadTabProps {
  onModelAdded: (filePath: string) => Promise<void>;
  onModelDownloaded: () => Promise<void>;
  activeSubTab?: 'add' | 'download';
  onSubTabChange?: (subtab: 'add' | 'download') => void;
}

const AddDownloadTab: FC<AddDownloadTabProps> = ({
  onModelAdded,
  onModelDownloaded,
  activeSubTab: externalActiveSubTab,
  onSubTabChange,
}) => {
  const [internalActiveSubTab, setInternalActiveSubTab] = useState<'add' | 'download'>('add');
  const activeSubTab = externalActiveSubTab ?? internalActiveSubTab;
  
  const handleSubTabChange = (subtab: 'add' | 'download') => {
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
          className={`subtab-button btn ${activeSubTab === 'add' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('add')}
        >
          📁 Add from file
        </button>
        <button
          className={`subtab-button btn ${activeSubTab === 'download' ? 'active' : ''}`}
          onClick={() => handleSubTabChange('download')}
        >
          ⬇️ Download from HF
        </button>
      </div>

      <div className="subtab-content">
        {activeSubTab === 'add' && (
          <AddModel onModelAdded={handleModelAdded} />
        )}
        {activeSubTab === 'download' && (
          <DownloadModel onModelDownloaded={handleModelDownloaded} />
        )}
      </div>
    </div>
  );
};

export default AddDownloadTab;
