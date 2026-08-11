/**
 * Setup Wizard - First-run system configuration.
 *
 * A full-screen wizard that guides users through:
 * 1. Welcome & system overview
 * 2. Models directory configuration
 * 3. llama.cpp installation
 * 4. Python fast-download helper setup
 * 5. Completion
 */

import { FC, useState, useEffect, useCallback } from 'react';
import {
  CheckCircle2,
  ChevronRight,
  Download,
  FolderOpen,
  Loader2,
  AlertCircle,
  RefreshCw,
  Cpu,
  HardDrive,
  Sparkles,
  SkipForward,
  ArrowRight,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { Button } from '../ui/Button';
import { Banner } from '../ui/Banner';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';
import { formatBytes } from '../../utils/format';
import type { SetupStatus, LlamaInstallProgress } from '../../types/setup';
import {
  getSetupStatus,
  streamLlamaInstall,
  setupPython,
} from '../../services/transport/api/setup';
import { updateSettings } from '../../services/transport/api/settings';

// ============================================================================
// Types
// ============================================================================

type WizardStep = 'welcome' | 'models' | 'llama' | 'python' | 'complete';

/** Ordered wizard steps. Module-level so hook dependencies stay stable. */
const WIZARD_STEPS: readonly WizardStep[] = ['welcome', 'models', 'llama', 'python', 'complete'];

interface SetupWizardProps {
  /** Called when the wizard completes setup. */
  onComplete: () => void;
}

// ============================================================================
// Main Wizard Component
// ============================================================================

export const SetupWizard: FC<SetupWizardProps> = ({ onComplete }) => {
  const [step, setStep] = useState<WizardStep>('welcome');
  const [status, setStatus] = useState<SetupStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadStatus = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const s = await getSetupStatus();
      setStatus(s);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to check system status');
    } finally {
      setLoading(false);
    }
  }, []);

  // Load initial setup status
  useEffect(() => {
    loadStatus();
  }, [loadStatus]);

  // Refresh status (for steps that change system state)
  const refreshStatus = useCallback(async () => {
    try {
      const s = await getSetupStatus();
      setStatus(s);
    } catch {
      // Silently ignore refresh errors
    }
  }, []);

  const handleComplete = useCallback(async () => {
    try {
      await updateSettings({ setupCompleted: true });
      onComplete();
    } catch {
      // Even if saving fails, let user through
      onComplete();
    }
  }, [onComplete]);

  const steps: readonly WizardStep[] = WIZARD_STEPS;
  const currentIndex = steps.indexOf(step);

  const goNext = useCallback(() => {
    const nextIndex = currentIndex + 1;
    if (nextIndex < steps.length) {
      setStep(steps[nextIndex]);
    }
  }, [currentIndex, steps]);

  const goBack = useCallback(() => {
    const prevIndex = currentIndex - 1;
    if (prevIndex >= 0) {
      setStep(steps[prevIndex]);
    }
  }, [currentIndex, steps]);

  if (loading) {
    return (
      <WizardShell>
        <div className="flex flex-col items-center justify-center gap-4 py-20">
          <Icon icon={Loader2} className="animate-spin text-primary" size={32} />
          <p className="text-text-secondary">Checking system status...</p>
        </div>
      </WizardShell>
    );
  }

  if (error && !status) {
    return (
      <WizardShell>
        <div className="flex flex-col items-center justify-center gap-4 py-20">
          <Icon icon={AlertCircle} className="text-danger" size={32} />
          <p className="text-danger">{error}</p>
          <Button variant="primary" onClick={loadStatus} leftIcon={<Icon icon={RefreshCw} size={16} />}>
            Retry
          </Button>
        </div>
      </WizardShell>
    );
  }

  if (!status) return null;

  return (
    <WizardShell>
      {/* Progress indicator */}
      <StepIndicator steps={steps} current={step} />

      {/* Step content */}
      <div className="flex-1 min-h-0 overflow-y-auto px-8 py-6">
        {step === 'welcome' && (
          <WelcomeStep status={status} onNext={goNext} />
        )}
        {step === 'models' && (
          <ModelsDirectoryStep status={status} onNext={goNext} onBack={goBack} />
        )}
        {step === 'llama' && (
          <LlamaInstallStep
            status={status}
            onNext={() => { refreshStatus(); goNext(); }}
            onBack={goBack}
          />
        )}
        {step === 'python' && (
          <PythonSetupStep
            status={status}
            onNext={() => { refreshStatus(); goNext(); }}
            onBack={goBack}
          />
        )}
        {step === 'complete' && (
          <CompleteStep status={status} onFinish={handleComplete} />
        )}
      </div>
    </WizardShell>
  );
};

// ============================================================================
// Shell & Layout
// ============================================================================

const WizardShell: FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="fixed inset-0 bg-background z-50 flex items-center justify-center">
    <div className="w-full max-w-[42rem] max-h-[90vh] bg-surface-elevated rounded-lg shadow-2xl flex flex-col overflow-hidden">
      {/* Header */}
      <div className="px-8 pt-6 pb-4 border-b border-border-light">
        <div className="flex items-center gap-3">
          <div className="h-8 w-8 rounded-lg bg-primary-subtle flex items-center justify-center">
            <Icon icon={Sparkles} size={18} className="text-primary" />
          </div>
          <div>
            <h1 className="text-lg font-semibold text-text">gglib Setup</h1>
            <p className="text-xs text-text-secondary">First-run system configuration</p>
          </div>
        </div>
      </div>
      {children}
    </div>
  </div>
);

// ============================================================================
// Step Indicator
// ============================================================================

const stepLabels: Record<WizardStep, string> = {
  welcome: 'Welcome',
  models: 'Models',
  llama: 'Engine',
  python: 'Downloads',
  complete: 'Done',
};

const StepIndicator: FC<{ steps: readonly WizardStep[]; current: WizardStep }> = ({ steps, current }) => {
  const currentIndex = steps.indexOf(current);

  return (
    <div className="px-8 py-3 flex items-center gap-2">
      {steps.map((s, i) => (
        <div key={s} className="flex items-center gap-2">
          <div
            className={cn(
              'flex items-center gap-1.5 text-xs font-medium transition-colors',
              i < currentIndex && 'text-success',
              i === currentIndex && 'text-primary',
              i > currentIndex && 'text-text-muted',
            )}
          >
            {i < currentIndex ? (
              <Icon icon={CheckCircle2} size={14} className="text-success" />
            ) : (
              <span
                className={cn(
                  'w-5 h-5 rounded-full flex items-center justify-center text-2xs font-bold border',
                  i === currentIndex
                    ? 'border-primary bg-primary/20 text-primary'
                    : 'border-text-muted text-text-muted',
                )}
              >
                {i + 1}
              </span>
            )}
            <span className="hidden sm:inline">{stepLabels[s]}</span>
          </div>
          {i < steps.length - 1 && (
            <Icon icon={ChevronRight} size={12} className="text-text-muted" />
          )}
        </div>
      ))}
    </div>
  );
};

// ============================================================================
// Step: Welcome
// ============================================================================

const WelcomeStep: FC<{ status: SetupStatus; onNext: () => void }> = ({ status, onNext }) => (
  <div className="flex flex-col gap-6">
    <div>
      <h2 className="text-xl font-semibold text-text mb-2">Welcome to gglib</h2>
      <p className="text-text-secondary leading-relaxed">
        Let&apos;s get your system ready to run local AI models. This wizard will check
        your setup and help you configure the essentials.
      </p>
    </div>

    {/* System summary */}
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <InfoCard
        icon={Cpu}
        label="GPU"
        value={
          status.gpuInfo.hasMetal
            ? 'Apple Metal'
            : status.gpuInfo.hasNvidia
              ? `NVIDIA${status.gpuInfo.cudaVersion ? ` (CUDA ${status.gpuInfo.cudaVersion})` : ''}`
              : status.gpuInfo.hasVulkan
                ? 'Vulkan'
                : 'CPU only'
        }
        variant={status.gpuInfo.hasMetal || status.gpuInfo.hasNvidia || status.gpuInfo.hasVulkan ? 'success' : 'neutral'}
      />
      <InfoCard
        icon={HardDrive}
        label="Memory"
        value={
          status.systemMemory
            ? formatBytes(status.systemMemory.totalRamBytes)
            : 'Unknown'
        }
        variant={
          status.systemMemory && status.systemMemory.totalRamBytes >= 8 * 1024 * 1024 * 1024
            ? 'success'
            : 'neutral'
        }
      />
    </div>

    <div className="flex justify-end pt-2">
      <Button variant="primary" onClick={onNext} rightIcon={<Icon icon={ArrowRight} size={16} />}>
        Get Started
      </Button>
    </div>
  </div>
);

// ============================================================================
// Step: Models Directory
// ============================================================================

const ModelsDirectoryStep: FC<{
  status: SetupStatus;
  onNext: () => void;
  onBack: () => void;
}> = ({ status, onNext, onBack }) => {
  const { modelsDirectory } = status;

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-xl font-semibold text-text mb-2">Models Directory</h2>
        <p className="text-text-secondary leading-relaxed">
          This is where your GGUF model files will be stored. The default location
          works for most users.
        </p>
      </div>

      <div className="bg-background-secondary rounded-lg p-4 border border-border">
        <div className="flex items-start gap-3">
          <Icon icon={FolderOpen} size={20} className="text-primary mt-0.5 shrink-0" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-text mb-1">Current path</p>
            <p className="text-sm text-text-secondary font-mono break-all">
              {modelsDirectory.path || '(not set)'}
            </p>
            <div className="flex items-center gap-3 mt-2 text-xs">
              <StatusBadge
                ok={modelsDirectory.exists}
                label={modelsDirectory.exists ? 'Exists' : 'Will be created'}
              />
              {modelsDirectory.exists && (
                <StatusBadge
                  ok={modelsDirectory.writable}
                  label={modelsDirectory.writable ? 'Writable' : 'Read-only'}
                />
              )}
            </div>
          </div>
        </div>
      </div>

      <p className="text-xs text-text-muted">
        You can change this later in Settings. Models directory can also be set via the{' '}
        <code className="text-text-secondary">GGLIB_MODELS_DIR</code> environment variable.
      </p>

      <StepNavigation onBack={onBack} onNext={onNext} />
    </div>
  );
};

// ============================================================================
// Step: Llama Install
// ============================================================================

const LlamaInstallStep: FC<{
  status: SetupStatus;
  onNext: () => void;
  onBack: () => void;
}> = ({ status, onNext, onBack }) => {
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<LlamaInstallProgress | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const [installed, setInstalled] = useState(status.llamaInstalled);

  const handleInstall = useCallback(() => {
    setInstalling(true);
    setInstallError(null);
    setProgress(null);

    const abort = streamLlamaInstall(
      (p) => setProgress(p),
      () => {
        setInstalling(false);
        setInstalled(true);
      },
      (err) => {
        setInstalling(false);
        setInstallError(err);
      },
    );

    // Cleanup on unmount
    return () => abort();
  }, []);

  if (installed) {
    return (
      <div className="flex flex-col gap-6">
        <div>
          <h2 className="text-xl font-semibold text-text mb-2">Inference Engine</h2>
          <p className="text-text-secondary leading-relaxed">
            llama.cpp is already installed and ready to use.
          </p>
        </div>
        <Banner variant="success">llama.cpp binaries are installed</Banner>
        <StepNavigation onBack={onBack} onNext={onNext} />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-xl font-semibold text-text mb-2">Inference Engine</h2>
        <p className="text-text-secondary leading-relaxed">
          gglib uses{' '}
          <span className="text-text font-medium">llama.cpp</span>{' '}
          to run AI models locally. We need to install its binaries.
        </p>
      </div>

      {status.llamaCanDownload && status.llamaPlatformDescription && (
        <p className="text-xs text-text-muted">
          Platform detected: <span className="text-text-secondary">{status.llamaPlatformDescription}</span>
        </p>
      )}

      {/* Install error */}
      {installError && (
        <Banner variant="danger" title="Installation failed">
          <span className="break-all">{installError}</span>
        </Banner>
      )}

      {/* Progress bar */}
      {installing && progress && (
        <div className="flex flex-col gap-2">
          <div className="h-2 bg-background-tertiary rounded overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-primary to-primary-light rounded transition-[width] duration-300"
              style={{ width: progress.total > 0 ? `${(progress.downloaded / progress.total) * 100}%` : '0%' }}
            />
          </div>
          <div className="flex justify-between text-xs text-text-secondary font-mono tabular-nums">
            <span>{progress.total > 0 ? `${((progress.downloaded / progress.total) * 100).toFixed(1)}%` : 'Starting…'}</span>
            {progress.total > 0 && (
              <span>{formatBytes(progress.downloaded)} / {formatBytes(progress.total)}</span>
            )}
          </div>
        </div>
      )}

      {installing && !progress && (
        <div className="flex items-center gap-2 text-sm text-text-secondary">
          <Icon icon={Loader2} className="animate-spin" size={16} />
          <span>Preparing download...</span>
        </div>
      )}

      <div className="flex items-center justify-between pt-2">
        <div className="flex items-center gap-2">
          {!installing && (
            <Button variant="ghost" onClick={onBack} size="sm">
              Back
            </Button>
          )}
        </div>
        <div className="flex items-center gap-2">
          {!installing && (
            <>
              <Button
                variant="ghost"
                onClick={onNext}
                size="sm"
                leftIcon={<Icon icon={SkipForward} size={14} />}
              >
                Skip
              </Button>
              {status.llamaCanDownload && (
                <Button
                  variant="primary"
                  onClick={handleInstall}
                  leftIcon={<Icon icon={Download} size={16} />}
                >
                  {installError ? 'Retry' : 'Install'}
                </Button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
};

// ============================================================================
// Step: Python Setup
// ============================================================================

const PythonSetupStep: FC<{
  status: SetupStatus;
  onNext: () => void;
  onBack: () => void;
}> = ({ status, onNext, onBack }) => {
  const [setting, setSetting] = useState(false);
  const [setupError, setSetupError] = useState<string | null>(null);
  const [ready, setReady] = useState(status.fastDownloadReady);

  const handleSetup = useCallback(async () => {
    setSetting(true);
    setSetupError(null);
    try {
      await setupPython();
      setReady(true);
    } catch (err) {
      setSetupError(err instanceof Error ? err.message : 'Setup failed');
    } finally {
      setSetting(false);
    }
  }, []);

  if (ready) {
    return (
      <div className="flex flex-col gap-6">
        <div>
          <h2 className="text-xl font-semibold text-text mb-2">Fast Downloads</h2>
          <p className="text-text-secondary leading-relaxed">
            The hf_xet accelerator is ready. Large model downloads will use
            optimized transfer for maximum speed.
          </p>
        </div>
        <Banner variant="success">Download accelerator is ready</Banner>
        <StepNavigation onBack={onBack} onNext={onNext} />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-xl font-semibold text-text mb-2">Fast Downloads</h2>
        <p className="text-text-secondary leading-relaxed">
          Downloads already work &mdash; gglib fetches models directly. If you have
          Python 3, it can also install a helper for{' '}
          <span className="text-text font-medium">significantly faster</span>{' '}
          transfers from Hugging Face (via hf_xet). Entirely optional.
        </p>
      </div>

      {!status.pythonAvailable && (
        <Banner variant="warning" title="Python not found">
          The accelerator needs Python 3.9 or newer. Skip this step and downloads
          will run directly, which needs nothing installed. To use it later,
          install Python and either re-run this wizard from Settings or run{' '}
          <code>gglib config fast-downloads enable</code>.
        </Banner>
      )}

      {/* Setup error with retry */}
      {setupError && (
        <Banner
          variant="danger"
          title="Setup failed"
          action={
            <Button
              variant="secondary"
              size="sm"
              onClick={handleSetup}
              leftIcon={<Icon icon={RefreshCw} size={14} />}
            >
              Retry
            </Button>
          }
        >
          <span className="break-all">{setupError}</span>
        </Banner>
      )}

      {/* Loading */}
      {setting && (
        <div className="flex items-center gap-2 text-sm text-text-secondary">
          <Icon icon={Loader2} className="animate-spin" size={16} />
          <span>Installing the download accelerator... This may take a minute.</span>
        </div>
      )}

      <div className="flex items-center justify-between pt-2">
        <Button variant="ghost" onClick={onBack} size="sm" disabled={setting}>
          Back
        </Button>
        <div className="flex items-center gap-2">
          {!setting && (
            <>
              <Button
                variant="ghost"
                onClick={onNext}
                size="sm"
                leftIcon={<Icon icon={SkipForward} size={14} />}
              >
                Skip
              </Button>
              {status.pythonAvailable && !setupError && (
                <Button variant="primary" onClick={handleSetup} leftIcon={<Icon icon={Download} size={16} />}>
                  Setup
                </Button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
};

// ============================================================================
// Step: Complete
// ============================================================================

const CompleteStep: FC<{ status: SetupStatus; onFinish: () => void }> = ({ status, onFinish }) => (
  <div className="flex flex-col gap-6">
    <div className="text-center py-4">
      <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-success-subtle mb-4">
        <Icon icon={CheckCircle2} size={32} className="text-success" />
      </div>
      <h2 className="text-xl font-semibold text-text mb-2">You&apos;re all set!</h2>
      <p className="text-text-secondary leading-relaxed max-w-[28rem] mx-auto">
        Your system is configured and ready to run local AI models. You can
        re-run this wizard anytime from Settings.
      </p>
    </div>

    {/* Summary */}
    <div className="grid grid-cols-1 gap-2">
      <SummaryRow label="Models directory" ok={status.modelsDirectory.exists} value={status.modelsDirectory.path} />
      <SummaryRow label="llama.cpp" ok={status.llamaInstalled} value={status.llamaInstalled ? 'Installed' : 'Not installed'} />
      {/* Always ok: downloads work either way, so an absent accelerator is a
          choice of transport, not a failure to flag. */}
      <SummaryRow label="Downloads" ok value={status.fastDownloadReady ? 'Accelerated (hf_xet)' : 'Direct'} />
    </div>

    <div className="flex justify-center pt-4">
      <Button variant="primary" size="lg" onClick={onFinish} rightIcon={<Icon icon={ArrowRight} size={16} />}>
        Start Using gglib
      </Button>
    </div>
  </div>
);

// ============================================================================
// Shared UI Helpers
// ============================================================================

const InfoCard: FC<{
  icon: LucideIcon;
  label: string;
  value: string;
  variant: 'success' | 'neutral';
}> = ({ icon: CardIcon, label, value, variant }) => (
  <div className="bg-background-secondary rounded-md p-3 flex items-center gap-3">
    <div className={cn(
      'w-8 h-8 rounded-lg flex items-center justify-center shrink-0',
      variant === 'success' ? 'bg-success-subtle' : 'bg-background-tertiary',
    )}>
      <Icon icon={CardIcon} size={16} className={variant === 'success' ? 'text-success' : 'text-text-secondary'} />
    </div>
    <div className="min-w-0">
      <p className="text-xs text-text-muted">{label}</p>
      <p className="text-sm font-medium text-text truncate">{value}</p>
    </div>
  </div>
);

const StatusBadge: FC<{ ok: boolean; label: string }> = ({ ok, label }) => (
  <span className="inline-flex items-center gap-xs text-xs text-text-muted">
    <span aria-hidden className={cn('w-1.5 h-1.5 rounded-full', ok ? 'bg-success' : 'bg-warning')} />
    {label}
  </span>
);

const StepNavigation: FC<{
  onBack: () => void;
  onNext: () => void;
  nextLabel?: string;
}> = ({ onBack, onNext, nextLabel = 'Continue' }) => (
  <div className="flex items-center justify-between pt-2">
    <Button variant="ghost" onClick={onBack} size="sm">
      Back
    </Button>
    <Button variant="primary" onClick={onNext} rightIcon={<Icon icon={ArrowRight} size={16} />}>
      {nextLabel}
    </Button>
  </div>
);

const SummaryRow: FC<{ label: string; ok: boolean; value: string }> = ({ label, ok, value }) => (
  <div className="flex items-center justify-between py-2 px-3 bg-background-secondary rounded-lg border border-border">
    <span className="text-sm text-text-secondary">{label}</span>
    <div className="flex items-center gap-2">
      <span className="text-sm text-text font-mono truncate max-w-[250px]">{value}</span>
      <Icon
        icon={ok ? CheckCircle2 : AlertCircle}
        size={14}
        className={ok ? 'text-success' : 'text-warning'}
      />
    </div>
  </div>
);

export default SetupWizard;
