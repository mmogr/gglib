import { FC, useState } from 'react';
import { AlertCircle, AlertTriangle, CheckCircle2, Download, Loader2, XCircle } from 'lucide-react';
import { appLogger } from '../services/platform';
import { LlamaProgressEvent } from '../hooks/useLlamaStatus';
import { INSTALL_PHASE_LABELS } from '../types/setup';
import { formatBytes, formatDuration, formatRate } from '../utils/format';
import { installLlama } from '../services/platform/llamaInstall';
import { Button } from './ui/Button';
import { Banner } from './ui/Banner';
import { Icon } from './ui/Icon';
import { Modal } from './ui/Modal';
import { cn } from '../utils/cn';

interface LlamaInstallModalProps {
  isOpen?: boolean;
  canDownload?: boolean;
  installing?: boolean;
  progress?: LlamaProgressEvent | null;
  error?: string | null;
  onInstall?: () => void;
  onSkip?: () => void;
  // New props for error-triggered mode
  metadata?: {
    expectedPath: string;
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

  const isCompleted = progress?.type === 'completed';
  const isError = progress?.type === 'failed';

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

    if (progress.type === 'progress') {
      // Percentage is a rendering detail; speed and time remaining are not —
      // they arrive measured, and deriving them here would disagree with
      // every other surface.
      const percentage = progress.total > 0 ? (progress.downloaded / progress.total) * 100 : 0;
      return (
        <div className="flex flex-col gap-[0.35rem]">
          <div className="h-2 bg-background-tertiary rounded overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-primary to-primary-light rounded transition-[width] duration-300"
              style={{ width: `${percentage}%` }}
            />
          </div>
          <div className="flex justify-between text-text-secondary text-base">
            <span>{percentage.toFixed(1)}%</span>
            {progress.total > 0 && (
              <span>{formatBytes(progress.downloaded)} / {formatBytes(progress.total)}</span>
            )}
          </div>
          <div className="text-text text-base">
            {formatRate(progress.rate_bps)} · {formatDuration(progress.eta_seconds)} remaining
          </div>
        </div>
      );
    }

    if (progress.type === 'phase_started') {
      return (
        <div className="flex flex-col gap-[0.35rem]">
          <div className="h-2 bg-background-tertiary rounded overflow-hidden">
            <div className={cn('h-full bg-gradient-to-r from-primary to-primary-light rounded', 'w-[30%] animate-indeterminate')} />
          </div>
          <div className="text-text text-base">{INSTALL_PHASE_LABELS[progress.phase]}</div>
        </div>
      );
    }

    if (progress.type === 'completed') {
      return (
        <div className="flex flex-col gap-[0.35rem]">
          <div className="h-2 bg-background-tertiary rounded overflow-hidden">
            <div className="h-full w-full bg-gradient-to-r from-primary to-primary-light rounded" />
          </div>
          <div className="text-text text-base">llama.cpp {progress.version} installed</div>
        </div>
      );
    }

    // `phase_completed` has nothing of its own to draw, and `failed` is
    // already carried by the error banner above.
    return null;
  };
  const renderFooterContent = () => {
    if (metadata) {
      return (
        <>
          <Button onClick={handleErrorModeInstall} disabled={installing} leftIcon={<Icon icon={Download} size={16} />}>
            Install now
          </Button>
          <Button variant="ghost" onClick={onClose} disabled={installing}>
            Cancel
          </Button>
        </>
      );
    }
    if (!installing && !isCompleted && canDownload) {
      return (
        <>
          <Button onClick={onInstall} disabled={installing} leftIcon={<Icon icon={Download} size={16} />}>
            Install llama.cpp
          </Button>
          {onSkip ? (
            <Button variant="ghost" onClick={onSkip} disabled={installing}>
              Skip for now
            </Button>
          ) : null}
        </>
      );
    }
    if (!installing && !isCompleted && !canDownload && onSkip) {
      return (
        <Button variant="ghost" onClick={onSkip}>
          I understand
        </Button>
      );
    }
    return null;
  };
  const renderMetadataContent = () => (
    <>
      <div className="flex gap-3 items-center">
        <div className="w-10 h-10 rounded-full inline-flex items-center justify-center bg-background-secondary border border-border text-primary">
          <Icon icon={installing ? Loader2 : AlertTriangle} size={28} className={installing ? 'animate-spin' : ''} />
        </div>
        <div>
          <h2 className="m-0 text-lg font-semibold text-text">llama-server Not Installed</h2>
          <p className="mt-1 mb-0 text-text-secondary text-base">{metadata?.reason}</p>
        </div>
      </div>

      <div className="flex flex-col gap-2">
        <p className="m-0 text-text-secondary leading-normal">The llama-server binary was not found at:</p>
        <code className="block bg-background-tertiary border border-border rounded-md py-2 px-3 font-mono text-base text-text break-all">{metadata?.expectedPath}</code>
      </div>

      {error ? <Banner variant="danger">{error}</Banner> : null}

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
          <h2 className="m-0 text-lg font-semibold text-text">{isCompleted ? 'Installation complete' : 'llama.cpp required'}</h2>
          {!installing && !isCompleted && (
            <p className="mt-1 mb-0 text-text-secondary text-base">
              {canDownload
                ? 'We will download a prebuilt binary for your platform (~15 MB).'
                : 'Please build llama.cpp via the CLI: gglib config llama install'}
            </p>
          )}
        </div>
      </div>

      {error && !installing ? <Banner variant="danger">{error}</Banner> : null}

      {renderProgress()}

      {isCompleted ? (
        <p className="text-success font-semibold">llama.cpp is ready! You can now serve models.</p>
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
      footer={renderFooterContent() ?? undefined}
    >
      <div className="flex flex-col gap-4">{metadata ? renderMetadataContent() : renderStandardContent()}</div>
    </Modal>
  );
};

export default LlamaInstallModal;
