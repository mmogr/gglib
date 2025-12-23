import { FC, useState } from 'react';
import { AlertCircle, AlertTriangle, CheckCircle2, Download, Loader2, XCircle } from 'lucide-react';
import { LlamaInstallProgress } from '../hooks/useLlamaStatus';
import { formatBytes } from '../utils/format';
import { installLlama } from '../services/platform/llamaInstall';
import { Button } from './ui/Button';
import { Icon } from './ui/Icon';
import { Modal } from './ui/Modal';
import styles from './LlamaInstallModal.module.css';

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
      await installLlama();
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

  const renderProgress = () => {
    if (!installing || !progress) return null;

    const isIndeterminate = progress.status === 'started';

    return (
      <div className={styles.progressBlock}>
        <div className={styles.progressBar}>
          <div
            className={`${styles.progressBarFill} ${isIndeterminate ? styles.indeterminate : ''}`}
            style={!isIndeterminate ? { width: `${progress.percentage}%` } : undefined}
          />
        </div>
        <div className={styles.progressMeta}>
          <span>{progress.percentage.toFixed(1)}%</span>
          {progress.total > 0 && (
            <span>{formatBytes(progress.downloaded)} / {formatBytes(progress.total)}</span>
          )}
        </div>
        <div className={styles.progressMessage}>{progress.message}</div>
      </div>
    );
  };

  const renderMetadataContent = () => (
    <>
      <div className={styles.lede}>
        <div className={styles.iconCircle}>
          <Icon icon={installing ? Loader2 : AlertTriangle} size={28} className={installing ? styles.iconSpin : ''} />
        </div>
        <div>
          <h2 className={styles.title}>llama-server Not Installed</h2>
          <p className={styles.subtitle}>{metadata?.reason}</p>
        </div>
      </div>

      <div className={styles.block}>
        <p className={styles.description}>The llama-server binary was not found at:</p>
        <code className={styles.code}>{metadata?.expectedPath}</code>
        {metadata?.legacyPath && (
          <div className={styles.callout}>
            <p className={styles.calloutTitle}>Older installation detected</p>
            <code className={styles.code}>{metadata.legacyPath}</code>
            <p className={styles.calloutNote}>Move or symlink it to the expected path to reuse.</p>
          </div>
        )}
      </div>

      {error ? <div className={styles.error}>{error}</div> : null}

      <div className={styles.actions}>
        <Button onClick={handleErrorModeInstall} disabled={installing} leftIcon={<Icon icon={Download} size={16} />}>
          Install now
        </Button>
        <Button variant="ghost" onClick={onClose} disabled={installing}>
          Cancel
        </Button>
      </div>

      {installing ? renderProgress() : null}
    </>
  );

  const renderStandardContent = () => (
    <>
      <div className={styles.lede}>
        <div className={styles.iconCircle}>
          <Icon
            icon={isCompleted ? CheckCircle2 : isError ? XCircle : AlertCircle}
            size={28}
            className={installing ? styles.iconPulse : ''}
          />
        </div>
        <div>
          <h2 className={styles.title}>{isCompleted ? 'Installation complete' : 'llama.cpp required'}</h2>
          {!installing && !isCompleted && (
            <p className={styles.subtitle}>
              {canDownload
                ? 'We will download a prebuilt binary for your platform (~15 MB).'
                : 'Please build llama.cpp via the CLI: gglib llama install'}
            </p>
          )}
        </div>
      </div>

      {error && !installing ? <div className={styles.error}>{error}</div> : null}

      {renderProgress()}

      {isCompleted ? (
        <p className={styles.success}>llama.cpp is ready! You can now serve models.</p>
      ) : null}

      {!installing && !isCompleted && canDownload ? (
        <div className={styles.actions}>
          <Button onClick={onInstall} disabled={installing} leftIcon={<Icon icon={Download} size={16} />}>
            Install llama.cpp
          </Button>
          {onSkip ? (
            <Button variant="ghost" onClick={onSkip} disabled={installing}>
              Skip for now
            </Button>
          ) : null}
        </div>
      ) : null}

      {!installing && !isCompleted && !canDownload && onSkip ? (
        <div className={styles.actions}>
          <Button variant="ghost" onClick={onSkip}>
            I understand
          </Button>
        </div>
      ) : null}
    </>
  );

  return (
    <Modal
      open={isOpen}
      onClose={onClose ?? (() => {})}
      title="Llama installation"
      size="md"
      preventClose={installing}
    >
      <div className={styles.content}>{metadata ? renderMetadataContent() : renderStandardContent()}</div>
    </Modal>
  );
};

export default LlamaInstallModal;
