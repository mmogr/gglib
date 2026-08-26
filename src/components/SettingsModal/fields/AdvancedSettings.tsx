import { FC } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { Textarea } from '../../ui/Textarea';
import { InferenceParametersForm } from '../../InferenceParametersForm';
import type { SparseInferenceConfig } from '../../../types';
import type { AgentGuardSettingsValues } from '../useAgentGuardSettings';
import { Label } from '../../primitives';
import { MAX_STAGNATION_STEPS, MAX_TOOL_ITERATIONS } from '../../../constants/settingsDefaults';
import { NumberSettingField } from './NumberSettingField';
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
  inferenceDefaultsInput: SparseInferenceConfig | undefined;
  setInferenceDefaultsInput: (value: SparseInferenceConfig | undefined) => void;
  trustClientSampling: boolean;
  setTrustClientSampling: (value: boolean) => void;
  proxyLoopDetection: boolean;
  setProxyLoopDetection: (value: boolean) => void;
  agentGuards: AgentGuardSettingsValues;
  setAgentGuardSetting: <K extends keyof AgentGuardSettingsValues>(
    key: K,
    value: AgentGuardSettingsValues[K],
  ) => void;
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
  agentGuards,
  setAgentGuardSetting,
  saving,
}) => (
  <>
    <Button
      type="button"
      variant="ghost"
      size="sm"
      className="px-0 font-semibold text-text hover:bg-transparent hover:text-primary"
      onClick={onToggle}
      aria-expanded={isOpen}
      leftIcon={<Icon icon={isOpen ? ChevronDown : ChevronRight} size={14} />}
    >
      Advanced Settings
    </Button>

    {isOpen && (
      <div className="flex flex-col gap-md pl-md border-l-2 border-l-border mt-sm animate-slide-down">
        <NumberSettingField
          id="max-tool-iterations-input"
          label="Max Tool Iterations"
          spec={MAX_TOOL_ITERATIONS}
          value={maxToolIterationsInput}
          onChange={setMaxToolIterationsInput}
          description="Maximum iterations for tool calling in agentic loop"
          disabled={saving}
        />

        <SettingField
          id="title-prompt-input"
          label="Chat Title Generation Prompt"
          description="Prompt used when AI generates chat titles. Leave empty to use the default."
          action={
            <Button type="button" variant="link" size="sm" onClick={() => setTitlePromptInput('')}>
              Reset to default
            </Button>
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
          // The bottom of the ladder: nothing sits between these and the
          // hardcoded floor, so the floor is what an empty field falls to.
          fallback={{ kind: 'floor' }}
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
          top-k, presence-penalty, repeat-penalty, or min-p. The client&apos;s own budgets are
          always honoured — max tokens and reasoning budget tokens — because they say what the
          request is, not how it should sample. Turn this on only for a client that exposes real
          sampling controls to its user (e.g. OpenWebUI).
        </ToggleField>

        <ToggleField
          id="proxy-loop-detection-input"
          label="Loop detection on the proxy endpoint"
          checked={proxyLoopDetection}
          onChange={setProxyLoopDetection}
          disabled={saving}
        >
          On by default: a conversation whose history repeats the same tool-call batch back
          to back, or the same assistant response anywhere in the session, beyond the
          agent-path thresholds is rejected with a clean 400 before any model work, instead
          of burning a model swap and a full generation per stuck turn. Turn this off only
          for a client that legitimately repeats identical batches with nothing in between.
        </ToggleField>

        <ToggleField
          id="agentic-sampling-input"
          label="Agentic sampling cap"
          checked={agentGuards.agenticSampling}
          onChange={(value) => setAgentGuardSetting('agenticSampling', value)}
          disabled={saving}
        >
          On by default: a turn that may emit structured output has its temperature
          capped to 0.3 — but only over a value nobody deliberately chose, and never
          on a reasoning-tagged model, whose thinking block shares one sampler with
          its tool call. Anything set by a person stands.
        </ToggleField>

        <ToggleField
          id="tool-call-repair-input"
          label="Tool call repair"
          checked={agentGuards.toolCallRepair}
          onChange={(value) => setAgentGuardSetting('toolCallRepair', value)}
          disabled={saving}
        >
          On by default: a tool call that fails its schema is re-issued once with
          <code className="font-mono"> tool_choice: &quot;required&quot;</code>, which makes
          llama.cpp&apos;s own grammar non-lazy from the first token — so a malformed call is
          usually repaired rather than forwarded to the client as a broken turn. Turn this off
          when you are measuring what a model actually produces, rather than using it.
        </ToggleField>

        <NumberSettingField
          id="max-stagnation-steps-input"
          label="Max Stagnation Steps"
          spec={MAX_STAGNATION_STEPS}
          value={agentGuards.maxStagnationSteps}
          onChange={(value) => setAgentGuardSetting('maxStagnationSteps', value)}
          description="Repeats of one assistant response before an agent loop stops. Counted across the whole session, not just consecutively, so A-B-A-B oscillation is caught. Shared by the built-in agent loop and the proxy's turn-level guard, so the two paths cannot drift."
          disabled={saving}
        />
      </div>
    )}
  </>
);
