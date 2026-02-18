import { FC, useState } from 'react';
import { Loader2, Play, ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { Modal } from '../../ui/Modal';
import { InferenceParametersForm } from '../../InferenceParametersForm';
import type { GgufModel, AppSettings, InferenceConfig } from '../../../types';
import { formatParamCount } from '../../../utils/format';

interface ServeModalProps {
  model: GgufModel;
  settings: AppSettings | null;
  // State
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  isServing: boolean;
  hasAgentTag: boolean;
  inferenceParams: InferenceConfig | undefined;
  // Handlers
  onContextChange: (value: string) => void;
  onPortChange: (value: string) => void;
  onJinjaChange: (value: boolean) => void;
  onJinjaReset: () => void;
  onInferenceParamsChange: (params: InferenceConfig) => void;
  onClose: () => void;
  onStart: () => void;
}

/**
 * Modal for configuring and starting a model server.
 */
export const ServeModal: FC<ServeModalProps> = ({
  model,
  settings,
  customContext,
  customPort,
  jinjaOverride,
  isServing,
  hasAgentTag,
  inferenceParams,
  onContextChange,
  onPortChange,
  onJinjaChange,
  onJinjaReset,
  onInferenceParamsChange,
  onClose,
  onStart,
}) => {
  const effectiveJinjaEnabled = jinjaOverride === null ? hasAgentTag : jinjaOverride;
  const isAutoJinja = jinjaOverride === null && hasAgentTag;
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Check if any inference params are set (for visual indicator)
  const hasInferenceOverrides = inferenceParams && Object.values(inferenceParams).some(v => v != null);

  return (
    <Modal open={true} onClose={onClose} title="Start model server" size="md" preventClose={isServing}>
      <div className="p-lg overflow-y-auto flex-1 min-h-0">
        <div className="flex justify-between items-center mb-lg p-base bg-background rounded-md border border-border">
          <strong>{model.name}</strong>
          <span className="text-text-secondary text-sm">{formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}</span>
        </div>

        <div className="mb-lg">
          <label htmlFor="context-input" className="block mb-sm font-medium text-text">
            Context Length
            <span className="font-normal text-text-secondary text-sm"> (optional)</span>
          </label>
          <Input
            id="context-input"
            type="number"
            className="w-full p-md bg-background-input border border-border rounded-base text-text text-base transition duration-200 focus:outline-none focus:border-border-focus focus:shadow-[0_0_0_3px_rgba(59,130,246,0.1)]"
            placeholder={
              settings?.defaultContextSize
                ? `Default: ${settings.defaultContextSize.toLocaleString()}`
                : model.contextLength
                  ? `Model max: ${model.contextLength.toLocaleString()}`
                  : 'Enter context length'
            }
            value={customContext}
            onChange={(e) => onContextChange(e.target.value)}
            disabled={isServing}
            min="1"
          />
          <p className="mt-sm text-sm text-text-secondary">
            {model.contextLength
              ? `Model's maximum: ${model.contextLength.toLocaleString()} tokens`
              : 'No model context metadata available'}
          </p>
        </div>

        <div className="mb-lg">
          <label htmlFor="port-input" className="block mb-sm font-medium text-text">
            Port
            <span className="font-normal text-text-secondary text-sm"> (optional)</span>
          </label>
          <Input
            id="port-input"
            type="number"
            className="w-full p-md bg-background-input border border-border rounded-base text-text text-base transition duration-200 focus:outline-none focus:border-border-focus focus:shadow-[0_0_0_3px_rgba(59,130,246,0.1)]"
            placeholder={
              settings?.llamaBasePort
                ? `Auto (from ${settings.llamaBasePort})`
                : 'Auto (from 9000)'
            }
            value={customPort}
            onChange={(e) => onPortChange(e.target.value)}
            disabled={isServing}
            min="1024"
            max="65535"
          />
          <p className="mt-sm text-sm text-text-secondary">
            Leave empty to auto-allocate from base port
          </p>
        </div>

        {hasAgentTag && (
          <div className="flex flex-col gap-sm py-sm px-md rounded-md border border-border bg-[rgba(59,130,246,0.08)] text-text text-sm mb-md" role="status">
            <div className="font-semibold">Agent tag detected</div>
            <p>
              Jinja templates {jinjaOverride === false 
                ? 'would normally be auto-enabled for agent-tagged models, but you have disabled them for this launch.' 
                : 'will be enabled automatically for agent-tagged models to support structured prompts.'}
            </p>
            {jinjaOverride !== null && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={onJinjaReset}
                disabled={isServing}
              >
                Reset to auto-detect
              </Button>
            )}
          </div>
        )}

        <div className="mb-lg">
          <div className="flex items-center justify-between gap-sm">
            <label htmlFor="jinja-toggle" className="block mb-0 font-medium text-text">Jinja Templates</label>
            <span className="text-sm text-text-muted">
              {isAutoJinja
                ? 'Auto (agent tag)'
                : (jinjaOverride === null
                  ? 'Disabled'
                  : (jinjaOverride ? 'Enabled manually' : 'Disabled manually'))}
            </span>
          </div>
          <div className="flex gap-md items-start">
            <input
              id="jinja-toggle"
              type="checkbox"
              checked={effectiveJinjaEnabled}
              onChange={(e) => onJinjaChange(e.target.checked)}
              disabled={isServing}
            />
            <div className="flex-1 text-sm text-text-secondary">
              <p className="m-0">
                Enable llama.cpp's Jinja templating for instruction/agent models. Leave off for plain chat models.
              </p>
            </div>
          </div>
        </div>

        {/* Advanced: Inference Parameters */}
        <div className="mb-lg">
          <button 
            type="button"
            className="advanced-toggle"
            onClick={() => setShowAdvanced(!showAdvanced)}
            disabled={isServing}
          >
            <Icon icon={showAdvanced ? ChevronDown : ChevronRight} size={16} />
            <span>Inference Parameters</span>
            {hasInferenceOverrides && <span className="override-indicator">•</span>}
          </button>
          {showAdvanced && (
            <div className="advanced-section">
              <p className="mt-sm text-sm text-text-secondary" style={{ marginBottom: '12px' }}>
                Override sampling parameters for this session. Leave empty to use model or global defaults.
              </p>
              <InferenceParametersForm
                value={inferenceParams}
                onChange={onInferenceParamsChange}
                disabled={isServing}
              />
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center justify-end gap-md p-lg border-t border-border shrink-0">
        <Button variant="ghost" onClick={onClose} disabled={isServing}>
          Cancel
        </Button>
        <Button
          variant="primary"
          onClick={onStart}
          disabled={isServing}
          leftIcon={!isServing ? <Icon icon={Play} size={14} /> : undefined}
        >
          {isServing ? (
            <>
              <Loader2 className="spinner" />
              Loading model...
            </>
          ) : (
            'Start Server'
          )}
        </Button>
      </div>
    </Modal>
  );
};
