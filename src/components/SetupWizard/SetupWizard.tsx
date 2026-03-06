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
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';
import { formatBytes } from '../../utils/format';
import type { SetupStatus, LlamaInstallProgress } from '../../types/setup';
import {
  getSetupStatus,
  installLlama,
  setupPython,
} from '../../services/transport/api/setup';
import { updateSettings } from '../../services/transport/api/settings';

// ============================================================================
// Types
// ============================================================================

type WizardStep = 'welcome' | 'models' | 'llama' | 'python' | 'complete';

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

  // Load initial setup status
  useEffect(() => {
    loadStatus();
  }, []);

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

  const steps: WizardStep[] = ['welcome', 'models', 'llama', 'python', 'complete'];
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
    <div className="w-full max-w-[42rem] max-h-[90vh] bg-surface rounded-xl border border-border shadow-2xl flex flex-col overflow-hidden">
      {/* Header */}
      <div className="px-8 pt-6 pb-4 border-b border-border">
        <div className="flex items-center gap-3">
          <div className="h-8 w-8 rounded-lg bg-primary/20 flex items-center justify-center">
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

const StepIndicator: FC<{ steps: WizardStep[]; current: WizardStep }> = ({ steps, current }) => {
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
                  'w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold border',
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
              : 'CPU only'
        }
        variant={status.gpuInfo.hasMetal || status.gpuInfo.hasNvidia ? 'success' : 'neutral'}
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

    const abort = installLlama(
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
        <div className="bg-success/10 border border-success/30 rounded-lg p-4 flex items-center gap-3">
          <Icon icon={CheckCircle2} size={20} className="text-success shrink-0" />
          <span className="text-sm text-text">llama.cpp binaries are installed</span>
        </div>
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
        <div className="bg-danger/10 border border-danger/30 rounded-lg p-4">
          <div className="flex items-start gap-3">
            <Icon icon={AlertCircle} size={18} className="text-danger shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-danger mb-1">Installation failed</p>
              <p className="text-xs text-text-secondary break-all">{installError}</p>
            </div>
          </div>
        </div>
      )}

      {/* Progress bar */}
      {installing && progress && (
        <div className="flex flex-col gap-2">
          <div className="h-2 bg-background-tertiary rounded overflow-hidden">
            <div
              className="h-full bg-gradient-to-r from-primary to-[#74c7ec] rounded transition-[width] duration-300"
              style={{ width: progress.total > 0 ? `${(progress.downloaded / progress.total) * 100}%` : '0%' }}
            />
          </div>
          <div className="flex justify-between text-xs text-text-secondary">
            <span>{progress.total > 0 ? `${((progress.downloaded / progress.total) * 100).toFixed(1)}%` : 'Starting...'}</span>
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
            The Python fast-download helper is ready. Large model downloads will
            use optimized transfer for maximum speed.
          </p>
        </div>
        <div className="bg-success/10 border border-success/30 rounded-lg p-4 flex items-center gap-3">
          <Icon icon={CheckCircle2} size={20} className="text-success shrink-0" />
          <span className="text-sm text-text">Fast download helper is ready</span>
        </div>
        <StepNavigation onBack={onBack} onNext={onNext} />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-xl font-semibold text-text mb-2">Fast Downloads</h2>
        <p className="text-text-secondary leading-relaxed">
          gglib can use a Python helper for{' '}
          <span className="text-text font-medium">significantly faster</span>{' '}
          model downloads from Hugging Face (via hf_xet). This is optional but recommended.
        </p>
      </div>

      {!status.pythonAvailable && (
        <div className="bg-warning/10 border border-warning/30 rounded-lg p-4 flex items-start gap-3">
          <Icon icon={AlertCircle} size={18} className="text-warning shrink-0 mt-0.5" />
          <div>
            <p className="text-sm font-medium text-warning mb-1">Python not found</p>
            <p className="text-xs text-text-secondary">
              Python 3 is required for fast downloads. Install Python 3 and restart the wizard,
              or skip this step to use standard downloads.
            </p>
          </div>
        </div>
      )}

      {/* Setup error with retry */}
      {setupError && (
        <div className="bg-danger/10 border border-danger/30 rounded-lg p-4">
          <div className="flex items-start gap-3">
            <Icon icon={AlertCircle} size={18} className="text-danger shrink-0 mt-0.5" />
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-danger mb-1">Setup failed</p>
              <p className="text-xs text-text-secondary break-all mb-3">{setupError}</p>
              <Button
                variant="danger"
                size="sm"
                onClick={handleSetup}
                leftIcon={<Icon icon={RefreshCw} size={14} />}
              >
                Retry
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Loading */}
      {setting && (
        <div className="flex items-center gap-2 text-sm text-text-secondary">
          <Icon icon={Loader2} className="animate-spin" size={16} />
          <span>Setting up Python environment... This may take a minute.</span>
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
      <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-success/20 mb-4">
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
      <SummaryRow label="Fast downloads" ok={status.fastDownloadReady} value={status.fastDownloadReady ? 'Ready' : 'Not configured'} />
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
  <div className="bg-background-secondary rounded-lg p-3 border border-border flex items-center gap-3">
    <div className={cn(
      'w-8 h-8 rounded-lg flex items-center justify-center shrink-0',
      variant === 'success' ? 'bg-success/20' : 'bg-background-tertiary',
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
  <span className={cn(
    'inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium',
    ok ? 'bg-success/20 text-success' : 'bg-warning/20 text-warning',
  )}>
    <span className={cn('w-1.5 h-1.5 rounded-full', ok ? 'bg-success' : 'bg-warning')} />
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
