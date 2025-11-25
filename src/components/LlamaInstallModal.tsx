import { FC } from 'react';
import { LlamaInstallProgress } from '../hooks/useLlamaStatus';
import styles from './LlamaInstallModal.module.css';

interface LlamaInstallModalProps {
  isOpen: boolean;
  canDownload: boolean;
  installing: boolean;
  progress: LlamaInstallProgress | null;
  error: string | null;
  onInstall: () => void;
  onSkip?: () => void;
}

const formatBytes = (bytes: number, decimals = 1) => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

export const LlamaInstallModal: FC<LlamaInstallModalProps> = ({
  isOpen,
  canDownload,
  installing,
  progress,
  error,
  onInstall,
  onSkip,
}) => {
  if (!isOpen) return null;

  const isCompleted = progress?.status === 'completed';
  const isError = progress?.status === 'error';

  return (
    <div className={styles.overlay}>
      <div className={styles.modal}>
        <div className={styles.icon}>
          {isCompleted ? '✅' : isError ? '❌' : '🦙'}
        </div>
        
        <h2 className={styles.title}>
          {isCompleted ? 'Installation Complete!' : 'llama.cpp Required'}
        </h2>
        
        {!installing && !isCompleted && (
          <p className={styles.description}>
            {canDownload 
              ? 'gglib needs llama.cpp to run models. Pre-built binaries are available for your platform and will be downloaded automatically (~15 MB).'
              : 'gglib needs llama.cpp to run models. Please build from source using the CLI: gglib llama install'}
          </p>
        )}

        {error && !installing && (
          <div className={styles.error}>{error}</div>
        )}

        {installing && progress && (
          <div className={styles.progressContainer}>
            <div className={styles.progressBar}>
              <div 
                className={`${styles.progressBarFill} ${progress.status === 'started' ? styles.indeterminate : ''}`}
                style={progress.status !== 'started' ? { width: `${progress.percentage}%` } : undefined}
              />
            </div>
            <div className={styles.progressDetails}>
              <span>{progress.percentage.toFixed(1)}%</span>
              {progress.total > 0 && (
                <span>{formatBytes(progress.downloaded)} / {formatBytes(progress.total)}</span>
              )}
            </div>
            <div className={styles.progressMessage}>{progress.message}</div>
          </div>
        )}

        {isCompleted && (
          <p className={styles.success}>
            llama.cpp is ready! You can now serve models.
          </p>
        )}

        {!installing && !isCompleted && canDownload && (
          <div className={styles.actions}>
            <button 
              className={styles.installButton}
              onClick={onInstall}
              disabled={installing}
            >
              Install llama.cpp
            </button>
            {onSkip && (
              <button 
                className={styles.skipButton}
                onClick={onSkip}
              >
                Skip for now
              </button>
            )}
          </div>
        )}

        {!installing && !isCompleted && !canDownload && onSkip && (
          <div className={styles.actions}>
            <button 
              className={styles.skipButton}
              onClick={onSkip}
            >
              I understand
            </button>
          </div>
        )}
      </div>
    </div>
  );
};

export default LlamaInstallModal;
