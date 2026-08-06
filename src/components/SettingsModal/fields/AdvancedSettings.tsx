import { FC } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { Textarea } from '../../ui/Textarea';
import { InferenceParametersForm } from '../../InferenceParametersForm';
import type { InferenceConfig } from '../../../types';
import { Label } from '../../primitives';
import { SettingField } from './SettingField';
import { ToggleField } from './ToggleField';
import { DEFAULT_TITLE_GENERATION_PROMPT } from '../../../services/transport';

interface AdvancedSettingsProps {
  isOpen: boolean;
  onToggle: () => void;
  maxToolIterationsInput: string;
  setMaxToolIterationsInput: (value: string) => void;
  titlePromptInput: string;
  setTitlePromptInput: (value: string) => void;
  inferenceDefaultsInput: InferenceConfig | undefined;
  setInferenceDefaultsInput: (value: InferenceConfig | undefined) => void;
  trustClientSampling: boolean;
  setTrustClientSampling: (value: boolean) => void;
  proxyLoopDetection: boolean;
  setProxyLoopDetection: (value: boolean) => void;
  saving: boolean;
}

/**
 * Collapsible advanced section: tool-iteration cap, title-generation prompt,
 * and global inference parameter defaults.
 */
export const AdvancedSettings: FC<AdvancedSettingsProps> = ({
  isOpen,
  onToggle,
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
  saving,
}) => (
  <>
    <button
      type="button"
      className="flex items-center gap-sm bg-none border-none text-text text-sm font-semibold cursor-pointer py-xs px-0 transition-colors duration-200 hover:text-primary"
      onClick={onToggle}
      aria-expanded={isOpen}
    >
      <Icon icon={isOpen ? ChevronDown : ChevronRight} size={14} />
      <span>Advanced Settings</span>
    </button>

    {isOpen && (
      <div className="flex flex-col gap-md pl-md border-l-2 border-l-border mt-sm animate-slide-down">
        <SettingField
          id="max-tool-iterations-input"
          label="Max Tool Iterations"
          defaultHint="25"
          description="Maximum iterations for tool calling in agentic loop"
        >
          <Input
            id="max-tool-iterations-input"
            type="number"
            value={maxToolIterationsInput}
            onChange={(event) => setMaxToolIterationsInput(event.target.value)}
            placeholder="25"
            min="1"
            max="50"
            disabled={saving}
          />
        </SettingField>

        <SettingField
          id="title-prompt-input"
          label="Chat Title Generation Prompt"
          description="Prompt used when AI generates chat titles. Leave empty to use the default."
          action={
            <button
              type="button"
              className="bg-none border-none text-primary cursor-pointer text-sm underline p-0"
              onClick={() => setTitlePromptInput('')}
            >
              Reset to default
            </button>
          }
        >
          <Textarea
            id="title-prompt-input"
            value={titlePromptInput}
            onChange={(event) => setTitlePromptInput(event.target.value)}
            placeholder={DEFAULT_TITLE_GENERATION_PROMPT}
            rows={3}
            disabled={saving}
          />
        </SettingField>

        <div className="border-t border-border my-md" />
        <Label>Global Inference Parameter Defaults</Label>
        <InferenceParametersForm
          value={inferenceDefaultsInput}
          onChange={setInferenceDefaultsInput}
          disabled={saving}
        />
        <p className="text-text-secondary text-sm">
          Default inference parameters for all models. Can be overridden per-model in the model
          inspector.
        </p>

        <div className="border-t border-border my-md" />
        <ToggleField
          id="trust-client-sampling-input"
          label="Trust client sampling parameters"
          checked={trustClientSampling}
          onChange={setTrustClientSampling}
          disabled={saving}
        >
          Off by default: most clients (VS Code Copilot&apos;s LLM Gateway, for one) send fixed
          sampling values with no user-facing control behind them, so this server&apos;s own
          per-model and global defaults apply instead of a request&apos;s temperature, top-p,
          top-k, presence-penalty, repeat-penalty, or min-p. Max tokens is always honoured. Turn
          this on only for a client that exposes real sampling controls to its user (e.g.
          OpenWebUI).
        </ToggleField>

        <ToggleField
          id="proxy-loop-detection-input"
          label="Loop detection on the proxy endpoint"
          checked={proxyLoopDetection}
          onChange={setProxyLoopDetection}
          disabled={saving}
        >
          On by default: a conversation whose history repeats the same tool-call batch or
          assistant response beyond the agent-path thresholds is rejected with a clean 400
          before any model work, instead of burning a model swap and a full generation per
          stuck turn. Turn this off only for a client that legitimately replays identical
          batches.
        </ToggleField>
      </div>
    )}
  </>
);
