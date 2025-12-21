import { FC, useState } from 'react';
import { LlamaInstallProgress } from '../hooks/useLlamaStatus';
import { formatBytes } from '../utils/format';
import styles from './LlamaInstallModal.module.css';
import { invoke } from '@tauri-apps/api/core';

interface LlamaInstallModalProps {
  isOpen?: boolean;
  canDownload?: boolean;
  installing?: boolean;
  progress?: LlamaInstallProgress | null;
  error?: string | null;
  onInstall?: () => void;
  onSkip?: () => void;
  // New props for error-triggered mode
  metadata?: {
    expectedPath: string;
    legacyPath?: string;
    suggestedCommand: string;
    reason: string;
  };
  onClose?: () => void;
  onInstalled?: () => void;
}

export const LlamaInstallModal: FC<LlamaInstallModalProps> = ({
  isOpen = true,
  canDownload = true,
  installing: propInstalling = false,
  progress: propProgress = null,
  error: propError = null,
  onInstall,
  onSkip,
  metadata,
  onClose,
  onInstalled,
}) => {
  // Local state for error-triggered mode
  const [localInstalling, setLocalInstalling] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const installing = metadata ? localInstalling : propInstalling;
  const progress = propProgress;
  const error = metadata ? localError : propError;

  const isCompleted = progress?.status === 'completed';
  const isError = progress?.status === 'error';

  // Error-triggered mode: handle installation
  const handleErrorModeInstall = async () => {
    setLocalInstalling(true);
    setLocalError(null);
    
    try {
      await invoke('install_llama');
      // Installation successful
      if (onInstalled) {
        onInstalled();
      }
      if (onClose) {
        onClose();
      }
    } catch (err) {
      console.error('Installation failed:', err);
      setLocalError(String(err));
    } finally {
      setLocalInstalling(false);
    }
  };

  if (!isOpen) return null;

  // Error-triggered mode UI
  if (metadata) {
    return (
      <div className={styles.overlay}>
        <div className={styles.modal}>
          <div className={styles.icon}>
            {installing ? '⏳' : '🦙'}
          </div>
          
          <h2 className={styles.title}>
            llama-server Not Installed
          </h2>
          
          <div className={styles.description}>
            <p>
              The llama-server binary was not found at:
            </p>
            <code className={styles.path}>{metadata.expectedPath}</code>
            
            {metadata.legacyPath && (
              <>
                <p style={{ marginTop: '1rem' }}>
                  Found an older installation at:
                </p>
                <code className={styles.path}>{metadata.legacyPath}</code>
                <p style={{ marginTop: '0.5rem', fontSize: '0.875rem', color: 'var(--color-text-secondary)' }}>
                  Consider moving or symlinking it to the new location.
                </p>
              </>
            )}
            
            <p style={{ marginTop: '1rem' }}>
              Reason: <strong>{metadata.reason}</strong>
            </p>
          </div>

          {error && (
            <div className={styles.error}>{error}</div>
          )}

          {!installing && (
            <div className={styles.actions}>
              <button 
                className={styles.installButton}
                onClick={handleErrorModeInstall}
              >
                Install Now
              </button>
              <button 
                className={styles.skipButton}
                onClick={onClose}
              >
                Cancel
              </button>
            </div>
          )}

          {installing && (
            <div className={styles.progressContainer}>
              <div className={styles.progressBar}>
                <div className={`${styles.progressBarFill} ${styles.indeterminate}`} />
              </div>
              <div className={styles.progressMessage}>Installing llama.cpp...</div>
            </div>
          )}
        </div>
      </div>
    );
  }

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
