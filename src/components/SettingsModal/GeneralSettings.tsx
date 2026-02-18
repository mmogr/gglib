import { FC, FormEvent } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";
import { Select } from "../ui/Select";
import { InferenceParametersForm } from "../InferenceParametersForm";
import { DEFAULT_TITLE_GENERATION_PROMPT } from "../../services/clients/chat";
import type { ModelsDirectoryInfo, GgufModel, InferenceConfig } from "../../types";
import { cn } from '../../utils/cn';

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
  showFitIndicators: boolean;
  setShowFitIndicators: (value: boolean) => void;
  
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
  maxStagnationStepsInput: string;
  setMaxStagnationStepsInput: (value: string) => void;
  titlePromptInput: string;
  setTitlePromptInput: (value: string) => void;
  
  // Inference defaults
  inferenceDefaultsInput: InferenceConfig | undefined;
  setInferenceDefaultsInput: (value: InferenceConfig | undefined) => void;
  
  // Actions
  onSubmit: (event: FormEvent) => Promise<void>;
  onReset: () => void;
  onRefresh: () => void;
  onClose: () => void;
  
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
  showFitIndicators,
  setShowFitIndicators,
  defaultModelInput,
  setDefaultModelInput,
  models,
  loadingModels,
  isAdvancedOpen,
  setIsAdvancedOpen,
  maxToolIterationsInput,
  setMaxToolIterationsInput,
  maxStagnationStepsInput,
  setMaxStagnationStepsInput,
  titlePromptInput,
  setTitlePromptInput,
  inferenceDefaultsInput,
  setInferenceDefaultsInput,
  onSubmit,
  onReset,
  onRefresh,
  onClose,
  loading,
  saving,
  error,
  successMessage,
}) => {
  if (loading) {
    return (
      <div className="modal-loading">
        <div className="modal-spinner" aria-hidden />
        <p className="modal-loading-text">Loading current settings…</p>
      </div>
    );
  }

  return (
    <form className="flex flex-col gap-md" onSubmit={onSubmit}>
      <label className="font-semibold text-text" htmlFor="models-dir-input">
        Default Download Path
      </label>
      <Input
        id="models-dir-input"
        value={pathInput}
        onChange={(event) => setPathInput(event.target.value)}
        placeholder="/path/to/models"
        disabled={saving}
      />
      <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
        {sourceDescription && <span>{sourceDescription}</span>}
        {info?.defaultPath && (
          <button type="button" className="bg-none border-none text-primary cursor-pointer text-sm underline p-0" onClick={onReset}>
            Reset to defaults
          </button>
        )}
      </div>

      {info && (
        <div className="flex gap-sm flex-wrap" role="status" aria-live="polite">
          <span
            className={cn(
              'px-2 py-[2px] rounded-base text-sm',
              info.exists ? 'bg-[rgba(16,185,129,0.15)] text-[#10b981]' : 'bg-[rgba(245,158,11,0.15)] text-[#f59e0b]',
            )}
            aria-label={info.exists ? "Directory exists" : "Directory will be created (warning)"}
          >
            {info.exists ? "Directory exists" : "Directory will be created"}
          </span>
          <span
            className={cn(
              'px-2 py-[2px] rounded-base text-sm',
              info.writable ? 'bg-[rgba(16,185,129,0.15)] text-[#10b981]' : 'bg-[rgba(239,68,68,0.15)] text-[#ef4444]',
            )}
            aria-label={info.writable ? "Writable" : "Not writable (error)"}
          >
            {info.writable ? "Writable" : "Not writable"}
          </span>
        </div>
      )}

      <div className="border-t border-border my-md" />

      <label className="font-semibold text-text" htmlFor="context-size-input">
        Default Context Size
      </label>
      <Input
        id="context-size-input"
        type="number"
        value={contextSizeInput}
        onChange={(event) => setContextSizeInput(event.target.value)}
        placeholder="4096"
        min="512"
        max="1000000"
        disabled={saving}
      />
      <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
        <span>Default context size for models (e.g., 4096, 8192, 16384)</span>
      </div>

      <label className="font-semibold text-text" htmlFor="default-model-select">
        Default Model
      </label>
      <Select
        id="default-model-select"
        value={defaultModelInput}
        onChange={(event) => setDefaultModelInput(event.target.value)}
        disabled={saving || loadingModels}
      >
        <option value="">No default model</option>
        {models.map((model) => (
          <option key={model.id} value={model.id?.toString() ?? ""}>
            {model.name}{model.quantization ? ` (${model.quantization})` : ""}
          </option>
        ))}
      </Select>
      <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
        <span>Model to use for quick commands like <code>gglib question</code></span>
      </div>

      <label className="font-semibold text-text" htmlFor="proxy-port-input">
        Proxy Server Port
      </label>
      <Input
        id="proxy-port-input"
        type="number"
        value={proxyPortInput}
        onChange={(event) => setProxyPortInput(event.target.value)}
        placeholder="8080"
        min="1024"
        max="65535"
        disabled={saving}
      />
      <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
        <span>Port for the OpenAI-compatible proxy server</span>
      </div>

      <label className="font-semibold text-text" htmlFor="server-port-input">
        Base Server Port
      </label>
      <Input
        id="server-port-input"
        type="number"
        value={serverPortInput}
        onChange={(event) => setServerPortInput(event.target.value)}
        placeholder="9000"
        min="1024"
        max="65535"
        disabled={saving}
      />
      <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
        <span>Starting port for llama-server instances</span>
      </div>

      <label className="font-semibold text-text" htmlFor="max-queue-size-input">
        Max Download Queue Size
      </label>
      <Input
        id="max-queue-size-input"
        type="number"
        value={maxQueueSizeInput}
        onChange={(event) => setMaxQueueSizeInput(event.target.value)}
        placeholder="10"
        min="1"
        max="50"
        disabled={saving}
      />
      <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
        <span>Maximum number of models that can be queued for download (1-50)</span>
      </div>

      <div className="border-t border-border my-md" />

      <div>
        <label className="flex items-center gap-sm cursor-pointer select-none">
          <input
            type="checkbox"
            className="w-[18px] h-[18px] accent-primary cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed"
            checked={showFitIndicators}
            onChange={(e) => setShowFitIndicators(e.target.checked)}
            disabled={saving}
          />
          <span className="font-semibold text-text">Show memory fit indicators</span>
        </label>
        <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
          <span>Display fit status indicators in the HuggingFace browser showing if models fit in your system memory</span>
        </div>
      </div>

      {/* Advanced Settings Section */}
      <div className="border-t border-border my-md" />
      <button
        type="button"
        className="flex items-center gap-sm bg-none border-none text-text text-sm font-semibold cursor-pointer py-xs px-0 transition-colors duration-200 hover:text-primary"
        onClick={() => setIsAdvancedOpen(!isAdvancedOpen)}
        aria-expanded={isAdvancedOpen}
      >
        <span className="text-xs transition-transform duration-200">{isAdvancedOpen ? '▼' : '▶'}</span>
        <span>Advanced Settings</span>
      </button>

      {isAdvancedOpen && (
        <div className="flex flex-col gap-md pl-md border-l-2 border-l-border mt-sm animate-slide-down">
          <label className="font-semibold text-text" htmlFor="max-tool-iterations-input">
            Max Tool Iterations
          </label>
          <Input
            id="max-tool-iterations-input"
            type="number"
            value={maxToolIterationsInput}
            onChange={(event) => setMaxToolIterationsInput(event.target.value)}
            placeholder="25"
            min="1"
            max="100"
            disabled={saving}
          />
          <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
            <span>Maximum iterations for tool calling in agentic loop (default: 25)</span>
          </div>

          <label className="font-semibold text-text" htmlFor="max-stagnation-steps-input">
            Max Stagnation Steps
          </label>
          <Input
            id="max-stagnation-steps-input"
            type="number"
            value={maxStagnationStepsInput}
            onChange={(event) => setMaxStagnationStepsInput(event.target.value)}
            placeholder="5"
            min="1"
            max="20"
            disabled={saving}
          />
          <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
            <span>Maximum repeated outputs before stopping (prevents infinite loops, default: 5)</span>
          </div>

          <label className="font-semibold text-text" htmlFor="title-prompt-input">
            Chat Title Generation Prompt
          </label>
          <Textarea
            id="title-prompt-input"
            value={titlePromptInput}
            onChange={(event) => setTitlePromptInput(event.target.value)}
            placeholder={DEFAULT_TITLE_GENERATION_PROMPT}
            rows={3}
            disabled={saving}
          />
          <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
            <span>Prompt used when AI generates chat titles. Leave empty to use the default.</span>
            <button
              type="button"
              className="bg-none border-none text-primary cursor-pointer text-sm underline p-0"
              onClick={() => setTitlePromptInput("")}
            >
              Reset to default
            </button>
          </div>

          <div className="border-t border-border my-md" />
          <label className="font-semibold text-text">
            Global Inference Parameter Defaults
          </label>
          <InferenceParametersForm
            value={inferenceDefaultsInput}
            onChange={setInferenceDefaultsInput}
            disabled={saving}
          />
          <div className="flex justify-between items-center gap-sm text-text-secondary text-sm">
            <span>Default inference parameters for all models. Can be overridden per-model in the model inspector.</span>
          </div>
        </div>
      )}

      {error && <p className="text-[#ef4444] text-sm" role="alert">{error}</p>}
      {successMessage && <p className="text-[#10b981] text-sm" role="status" aria-live="polite">{successMessage}</p>}

      <div className="modal-footer modal-footer-between">
        <Button type="button" variant="secondary" onClick={onRefresh} disabled={loading || saving}>
          Refresh
        </Button>
        <div className="flex gap-sm">
          <Button type="button" variant="secondary" onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={saving}>
            {saving ? "Saving…" : "Save changes"}
          </Button>
        </div>
      </div>
    </form>
  );
};
