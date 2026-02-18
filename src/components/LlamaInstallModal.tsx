import { FC, useState } from 'react';
import { AlertCircle, AlertTriangle, CheckCircle2, Download, Loader2, XCircle } from 'lucide-react';
import { appLogger } from '../services/platform';
import { LlamaInstallProgress } from '../hooks/useLlamaStatus';
import { formatBytes } from '../utils/format';
import { installLlama } from '../services/platform/llamaInstall';
import { Button } from './ui/Button';
import { Icon } from './ui/Icon';
import { Modal } from './ui/Modal';
import { cn } from '../utils/cn';

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
      appLogger.error('component.settings', 'Installation failed', { error: err });
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
      <div className="flex flex-col gap-[0.35rem]">
        <div className="h-2 bg-background-tertiary rounded overflow-hidden">
          <div
            className={cn('h-full bg-gradient-to-r from-primary to-[#74c7ec] rounded transition-[width] duration-300', isIndeterminate && 'w-[30%] animate-indeterminate')}
            style={!isIndeterminate ? { width: `${progress.percentage}%` } : undefined}
          />
        </div>
        <div className="flex justify-between text-text-secondary text-[0.9rem]">
          <span>{progress.percentage.toFixed(1)}%</span>
          {progress.total > 0 && (
            <span>{formatBytes(progress.downloaded)} / {formatBytes(progress.total)}</span>
          )}
        </div>
        <div className="text-text text-[0.95rem]">{progress.message}</div>
      </div>
    );
  };

  const renderMetadataContent = () => (
    <>
      <div className="flex gap-3 items-center">
        <div className="w-10 h-10 rounded-full inline-flex items-center justify-center bg-background-secondary border border-border text-primary">
          <Icon icon={installing ? Loader2 : AlertTriangle} size={28} className={installing ? 'animate-spin' : ''} />
        </div>
        <div>
          <h2 className="m-0 text-[1.1rem] font-semibold text-text">llama-server Not Installed</h2>
          <p className="mt-1 mb-0 text-text-secondary text-[0.95rem]">{metadata?.reason}</p>
        </div>
      </div>

      <div className="flex flex-col gap-2">
        <p className="m-0 text-text-secondary leading-normal">The llama-server binary was not found at:</p>
        <code className="block bg-background-tertiary border border-border rounded-md py-2 px-3 font-mono text-[0.9rem] text-text break-all">{metadata?.expectedPath}</code>
        {metadata?.legacyPath && (
          <div className="p-3 rounded-lg border border-border bg-background-secondary flex flex-col gap-[0.35rem]">
            <p className="m-0 font-semibold text-text">Older installation detected</p>
            <code className="block bg-background-tertiary border border-border rounded-md py-2 px-3 font-mono text-[0.9rem] text-text break-all">{metadata.legacyPath}</code>
            <p className="m-0 text-[0.9rem] text-text-secondary">Move or symlink it to the expected path to reuse.</p>
          </div>
        )}
      </div>

      {error ? <div className="bg-[rgba(239,68,68,0.1)] border border-danger rounded-lg py-3 px-4 text-danger text-[0.95rem]">{error}</div> : null}

      <div className="flex gap-2 flex-wrap justify-end">
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
      <div className="flex gap-3 items-center">
        <div className="w-10 h-10 rounded-full inline-flex items-center justify-center bg-background-secondary border border-border text-primary">
          <Icon
            icon={isCompleted ? CheckCircle2 : isError ? XCircle : AlertCircle}
            size={28}
            className={installing ? 'animate-pulse' : ''}
          />
        </div>
        <div>
          <h2 className="m-0 text-[1.1rem] font-semibold text-text">{isCompleted ? 'Installation complete' : 'llama.cpp required'}</h2>
          {!installing && !isCompleted && (
            <p className="mt-1 mb-0 text-text-secondary text-[0.95rem]">
              {canDownload
                ? 'We will download a prebuilt binary for your platform (~15 MB).'
                : 'Please build llama.cpp via the CLI: gglib llama install'}
            </p>
          )}
        </div>
      </div>

      {error && !installing ? <div className="bg-[rgba(239,68,68,0.1)] border border-danger rounded-lg py-3 px-4 text-danger text-[0.95rem]">{error}</div> : null}

      {renderProgress()}

      {isCompleted ? (
        <p className="text-success font-semibold">llama.cpp is ready! You can now serve models.</p>
      ) : null}

      {!installing && !isCompleted && canDownload ? (
        <div className="flex gap-2 flex-wrap justify-end">
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
        <div className="flex gap-2 flex-wrap justify-end">
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
      <div className="flex flex-col gap-4">{metadata ? renderMetadataContent() : renderStandardContent()}</div>
    </Modal>
  );
};

export default LlamaInstallModal;
