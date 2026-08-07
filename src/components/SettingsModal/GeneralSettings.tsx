import { FC, FormEvent } from "react";
import type { ModelsDirectoryInfo, GgufModel, InferenceConfig } from "../../types";
import {
  PathSettings,
  ModelDefaults,
  PortSettings,
  SecuritySettings,
  DisplaySettings,
  DesktopSettings,
  AdvancedSettings,
  SetupWizardRow,
} from "./fields";
import type { DesktopSettingsValues } from "./useDesktopSettings";

interface GeneralSettingsProps {
  // Directory state
  pathInput: string;
  setPathInput: (value: string) => void;
  info: ModelsDirectoryInfo | null;
  sourceDescription: string | null;

  // Settings state
  contextSizeInput: string;
  setContextSizeInput: (value: string) => void;
  proxyPortInput: string;
  setProxyPortInput: (value: string) => void;
  serverPortInput: string;
  setServerPortInput: (value: string) => void;
  maxQueueSizeInput: string;
  setMaxQueueSizeInput: (value: string) => void;
  proxyApiKeyInput: string;
  setProxyApiKeyInput: (value: string) => void;
  showFitIndicators: boolean;
  setShowFitIndicators: (value: boolean) => void;

  // Always-on proxy (desktop app)
  desktopSettings: DesktopSettingsValues;
  setDesktopSetting: <K extends keyof DesktopSettingsValues>(
    key: K,
    value: DesktopSettingsValues[K],
  ) => void;

  // Default model state
  defaultModelInput: string;
  setDefaultModelInput: (value: string) => void;
  models: GgufModel[];
  loadingModels: boolean;

  // Advanced settings
  isAdvancedOpen: boolean;
  setIsAdvancedOpen: (value: boolean) => void;
  maxToolIterationsInput: string;
  setMaxToolIterationsInput: (value: string) => void;
  titlePromptInput: string;
  setTitlePromptInput: (value: string) => void;

  // Inference defaults
  inferenceDefaultsInput: InferenceConfig | undefined;
  setInferenceDefaultsInput: (value: InferenceConfig | undefined) => void;
  trustClientSampling: boolean;
  setTrustClientSampling: (value: boolean) => void;
  proxyLoopDetection: boolean;
  setProxyLoopDetection: (value: boolean) => void;

  // Actions
  onSubmit: (event: FormEvent) => Promise<void>;
  onReset: () => void;

  // Status
  loading: boolean;
  saving: boolean;
  error: string | null;
  successMessage: string | null;
}

export const GeneralSettings: FC<GeneralSettingsProps> = ({
  pathInput,
  setPathInput,
  info,
  sourceDescription,
  contextSizeInput,
  setContextSizeInput,
  proxyPortInput,
  setProxyPortInput,
  serverPortInput,
  setServerPortInput,
  maxQueueSizeInput,
  setMaxQueueSizeInput,
  proxyApiKeyInput,
  setProxyApiKeyInput,
  showFitIndicators,
  setShowFitIndicators,
  desktopSettings,
  setDesktopSetting,
  defaultModelInput,
  setDefaultModelInput,
  models,
  loadingModels,
  isAdvancedOpen,
  setIsAdvancedOpen,
  maxToolIterationsInput,
  setMaxToolIterationsInput,
  titlePromptInput,
  setTitlePromptInput,
  inferenceDefaultsInput,
  setInferenceDefaultsInput,
  trustClientSampling,
  setTrustClientSampling,
  proxyLoopDetection,
  setProxyLoopDetection,
  onSubmit,
  onReset,
  loading,
  saving,
  error,
  successMessage,
}) => {
  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center p-2xl gap-base min-h-[200px]">
        <div className="w-[40px] h-[40px] border-[3px] border-border border-t-primary rounded-full animate-spin-360" aria-hidden />
        <p className="text-text-secondary text-sm">Loading current settings…</p>
      </div>
    );
  }

  return (
    <form id="settings-general-form" className="flex flex-col gap-xl" onSubmit={onSubmit}>
      <PathSettings
        pathInput={pathInput}
        setPathInput={setPathInput}
        info={info}
        sourceDescription={sourceDescription}
        onReset={onReset}
        saving={saving}
      />

      <ModelDefaults
        contextSizeInput={contextSizeInput}
        setContextSizeInput={setContextSizeInput}
        defaultModelInput={defaultModelInput}
        setDefaultModelInput={setDefaultModelInput}
        models={models}
        loadingModels={loadingModels}
        saving={saving}
      />

      <PortSettings
        proxyPortInput={proxyPortInput}
        setProxyPortInput={setProxyPortInput}
        serverPortInput={serverPortInput}
        setServerPortInput={setServerPortInput}
        maxQueueSizeInput={maxQueueSizeInput}
        setMaxQueueSizeInput={setMaxQueueSizeInput}
        saving={saving}
      />

      <SecuritySettings
        proxyApiKeyInput={proxyApiKeyInput}
        setProxyApiKeyInput={setProxyApiKeyInput}
        saving={saving}
      />

      <DisplaySettings
        showFitIndicators={showFitIndicators}
        setShowFitIndicators={setShowFitIndicators}
        saving={saving}
      />

      <DesktopSettings
        values={desktopSettings}
        onChange={setDesktopSetting}
        saving={saving}
      />

      <AdvancedSettings
        isOpen={isAdvancedOpen}
        onToggle={() => setIsAdvancedOpen(!isAdvancedOpen)}
        maxToolIterationsInput={maxToolIterationsInput}
        setMaxToolIterationsInput={setMaxToolIterationsInput}
        titlePromptInput={titlePromptInput}
        setTitlePromptInput={setTitlePromptInput}
        inferenceDefaultsInput={inferenceDefaultsInput}
        setInferenceDefaultsInput={setInferenceDefaultsInput}
        trustClientSampling={trustClientSampling}
        setTrustClientSampling={setTrustClientSampling}
        proxyLoopDetection={proxyLoopDetection}
        setProxyLoopDetection={setProxyLoopDetection}
        saving={saving}
      />

      {error && <p className="text-danger text-sm" role="alert">{error}</p>}
      {successMessage && <p className="text-success text-sm" role="status" aria-live="polite">{successMessage}</p>}

      <SetupWizardRow saving={saving} />

    </form>
  );
};
