import { FC } from 'react';
import { IncompleteDownload } from '../../types';
import styles from './IncompleteDownloadsModal.module.css';

interface IncompleteDownloadsModalProps {
  isOpen: boolean;
  downloads: IncompleteDownload[];
  onResume: () => void;
  onDiscard: () => void;
}

/**
 * Modal that appears on startup when there are incomplete downloads.
 * Allows user to resume or discard them.
 */
const IncompleteDownloadsModal: FC<IncompleteDownloadsModalProps> = ({
  isOpen,
  downloads,
  onResume,
  onDiscard,
}) => {
  if (!isOpen || downloads.length === 0) {
    return null;
  }

  // Group by model_id to show unique models
  const uniqueModels = new Map<string, IncompleteDownload>();
  for (const d of downloads) {
    const key = d.quantization ? `${d.model_id}:${d.quantization}` : d.model_id;
    if (!uniqueModels.has(key)) {
      uniqueModels.set(key, d);
    }
  }

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
  };

  return (
    <div className={styles.overlay}>
      <div className={styles.modal}>
        <div className={styles.header}>
          <span className={styles.icon}>⏸</span>
          <h2>Resume Downloads?</h2>
        </div>
        
        <p className={styles.description}>
          {uniqueModels.size === 1
            ? 'There is an incomplete download from a previous session:'
            : `There are ${uniqueModels.size} incomplete downloads from a previous session:`}
        </p>

        <ul className={styles.downloadList}>
          {Array.from(uniqueModels.values()).map((download, index) => (
            <li key={index} className={styles.downloadItem}>
              <span className={styles.modelName}>
                {download.model_id}
                {download.quantization && (
                  <span className={styles.quantization}>{download.quantization}</span>
                )}
              </span>
              {download.bytes_downloaded > 0 && download.total_bytes > 0 && (
                <span className={styles.progress}>
                  {formatBytes(download.bytes_downloaded)} / {formatBytes(download.total_bytes)}
                  ({Math.round((download.bytes_downloaded / download.total_bytes) * 100)}%)
                </span>
              )}
            </li>
          ))}
        </ul>

        <div className={styles.actions}>
          <button
            className={styles.resumeButton}
            onClick={onResume}
          >
            ▶ Resume Downloads
          </button>
          <button
            className={styles.discardButton}
            onClick={onDiscard}
          >
            ✕ Discard
          </button>
        </div>
      </div>
    </div>
  );
};

export default IncompleteDownloadsModal;
