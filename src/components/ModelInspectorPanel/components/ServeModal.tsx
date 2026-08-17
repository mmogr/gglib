import { FC, useState } from 'react';
import { Checkbox } from '../../ui/Checkbox';
import { Play, ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Banner } from '../../ui/Banner';
import { Input } from '../../ui/Input';
import { Modal } from '../../ui/Modal';
import { InferenceParametersForm } from '../../InferenceParametersForm';
import { JinjaModeField } from './JinjaModeField';
import { useSamplingExplanation } from '../hooks/useSamplingExplanation';
import type { GgufModel, AppSettings, InferenceConfig, TemplateSupport } from '../../../types';
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
  hasMtpTag: boolean;
  /** null = auto from tag; 0 = disable; >0 = explicit count */
  mtpNMaxOverride: number | null;
  /** null = use default 0.75 */
  mtpPMinOverride: number | null;
  inferenceParams: InferenceConfig | undefined;
  /**
   * Whether this model's template reads `reasoning_effort`, from the model
   * detail. Absent reads as `unknown` — the launch this modal configures is
   * often the very launch that will answer the question.
   */
  reasoningEffortSupport?: TemplateSupport;
  /** Serve as a pinned proxy (`gglib serve`) instead of a bare model start. */
  pinProxy: boolean;
  // Handlers
  onContextChange: (value: string) => void;
  onPortChange: (value: string) => void;
  onJinjaChange: (value: boolean) => void;
  onJinjaReset: () => void;
  onMtpNMaxChange: (value: number | null) => void;
  onMtpPMinChange: (value: number | null) => void;
  onInferenceParamsChange: (params: InferenceConfig) => void;
  onPinProxyChange: (pin: boolean) => void;
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
  hasMtpTag,
  mtpNMaxOverride,
  mtpPMinOverride,
  inferenceParams,
  reasoningEffortSupport,
  pinProxy,
  onPinProxyChange,
  onContextChange,
  onPortChange,
  onJinjaChange,
  onJinjaReset,
  onMtpNMaxChange,
  onMtpPMinChange,
  onInferenceParamsChange,
  onClose,
  onStart,
}) => {
  // MTP: auto-enabled when tag present and no explicit override
  const effectiveMtpEnabled = mtpNMaxOverride !== null ? mtpNMaxOverride > 0 : hasMtpTag;
  const isAutoMtp = mtpNMaxOverride === null && hasMtpTag;
  const [showAdvanced, setShowAdvanced] = useState(false);

  // What this model's parameters resolve to before this session overrides
  // anything, so an empty field can name the value and layer it will inherit.
  // Session overrides live only in this modal, so nothing here invalidates it.
  const resolution = useSamplingExplanation(model.id, null);

  // Check if any inference params are set (for visual indicator)
  const hasInferenceOverrides = inferenceParams && Object.values(inferenceParams).some(v => v != null);

  return (
    <Modal
      open={true}
      onClose={onClose}
      title="Start model server"
      size="md"
      preventClose={isServing}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={isServing}>
            Cancel
          </Button>
          <Button
            variant="primary"
            onClick={onStart}
            isLoading={isServing}
            leftIcon={!isServing ? <Icon icon={Play} size={14} /> : undefined}
          >
            {isServing
              ? pinProxy
                ? 'Starting pinned proxy…'
                : 'Loading model…'
              : pinProxy
                ? 'Start Pinned Proxy'
                : 'Start Server'}
          </Button>
        </>
      }
    >
        <div className="flex justify-between items-center mb-lg p-base bg-background rounded-md">
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
            className="font-mono tabular-nums"
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
            className="font-mono tabular-nums"
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

        <div className="mb-lg">
          <Checkbox
            id="pin-proxy-toggle"
            checked={pinProxy}
            onChange={(e) => onPinProxyChange(e.target.checked)}
            disabled={isServing}
            label="Pin the proxy to this model"
            description="The OpenAI endpoint serves only this model and refuses requests naming any other — the GUI equivalent of gglib serve, for clients that cannot switch models."
          />
        </div>

        <JinjaModeField
          value={jinjaOverride}
          hasAgentTag={hasAgentTag}
          disabled={isServing}
          onChange={onJinjaChange}
          onReset={onJinjaReset}
        />

        {/* MTP Speculative Decoding section (shown for all models; auto-banner when tagged) */}
        {hasMtpTag && (
          <Banner
            variant="info"
            title="MTP speculative decoding detected"
            className="mb-md"
            action={
              mtpNMaxOverride !== null && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => { onMtpNMaxChange(null); onMtpPMinChange(null); }}
                  disabled={isServing}
                >
                  Reset to auto-detect
                </Button>
              )
            }
          >
            {mtpNMaxOverride === 0
              ? 'Speculative decoding would normally be auto-enabled for this model, but you have disabled it for this launch.'
              : 'This model contains embedded MTP draft heads. Speculative decoding will be enabled automatically (n-max=2, p-min=0.75).'}
          </Banner>
        )}

        <div className="mb-lg">
          <div className="flex items-center justify-between gap-sm">
            <label className="block mb-0 font-medium text-text">MTP Speculative Decoding</label>
            <span className="text-sm text-text-muted">
              {isAutoMtp
                ? 'Auto (mtp tag)'
                : (mtpNMaxOverride === null
                  ? 'Disabled'
                  : (mtpNMaxOverride === 0 ? 'Disabled manually' : `Enabled (n=${mtpNMaxOverride})`))}
            </span>
          </div>
          <Checkbox
            id="mtp-toggle"
            wrapperClassName="mt-sm"
            checked={effectiveMtpEnabled}
            onChange={(e) => {
              if (!e.target.checked) {
                onMtpNMaxChange(0);
              } else {
                // Restore to auto (tag) or default explicit n=2
                onMtpNMaxChange(hasMtpTag ? null : 2);
              }
            }}
            disabled={isServing}
            description={
              <>
                Enable <code>--spec-type draft-mtp</code> speculative decoding for MTP models.
                Requires bundled draft heads in the GGUF file.
              </>
            }
          />
          {effectiveMtpEnabled && (
            <div className="flex gap-md mt-md">
              <div className="flex-1">
                <label htmlFor="mtp-n-max" className="block mb-sm text-sm font-medium text-text">
                  Draft tokens (n-max)
                </label>
                <Input
                  id="mtp-n-max"
                  type="number"
                  className="font-mono tabular-nums"
                  placeholder={isAutoMtp ? 'Auto (2)' : '2'}
                  value={mtpNMaxOverride !== null && mtpNMaxOverride > 0 ? String(mtpNMaxOverride) : ''}
                  onChange={(e) => {
                    const v = e.target.value.trim();
                    if (v === '') {
                      onMtpNMaxChange(null);
                    } else {
                      const parsed = parseInt(v, 10);
                      onMtpNMaxChange(Number.isFinite(parsed) ? Math.min(8, Math.max(1, parsed)) : 1);
                    }
                  }}
                  disabled={isServing}
                  min="1"
                  max="8"
                />
              </div>
              <div className="flex-1">
                <label htmlFor="mtp-p-min" className="block mb-sm text-sm font-medium text-text">
                  Min probability (p-min)
                </label>
                <Input
                  id="mtp-p-min"
                  type="number"
                  className="font-mono tabular-nums"
                  placeholder={isAutoMtp ? 'Auto (0.75)' : '0.75'}
                  value={mtpPMinOverride !== null ? String(mtpPMinOverride) : ''}
                  onChange={(e) => {
                    const v = e.target.value.trim();
                    if (v === '') {
                      onMtpPMinChange(null);
                    } else {
                      const parsed = parseFloat(v);
                      onMtpPMinChange(Number.isFinite(parsed) ? Math.min(1, Math.max(0, parsed)) : null);
                    }
                  }}
                  disabled={isServing}
                  min="0"
                  max="1"
                  step="0.05"
                />
              </div>
            </div>
          )}
        </div>

        {/* Advanced: Inference Parameters */}
        <div className="mb-lg">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="px-0 hover:bg-transparent"
            onClick={() => setShowAdvanced(!showAdvanced)}
            disabled={isServing}
            aria-expanded={showAdvanced}
            leftIcon={<Icon icon={showAdvanced ? ChevronDown : ChevronRight} size={16} />}
          >
            Inference Parameters
            {hasInferenceOverrides && <span className="ml-1 text-primary-light leading-none">•</span>}
          </Button>
          {showAdvanced && (
            <div className="mt-sm">
              <p className="mt-sm mb-md text-sm text-text-secondary">
                Override sampling parameters for this session.
              </p>
              <InferenceParametersForm
                value={inferenceParams}
                onChange={onInferenceParamsChange}
                disabled={isServing}
                fallback={{ kind: 'resolved', ownLayer: 'request', resolution }}
                capabilities={{ reasoningEffort: reasoningEffortSupport ?? 'unknown' }}
              />
            </div>
          )}
        </div>
    </Modal>
  );
};
