import { FC, useCallback, useMemo, useState } from 'react';
import { X } from 'lucide-react';
import type { SparseInferenceConfig, SamplingParamKey, TemplateSupport } from '../../types';
import { INFERENCE_PARAMS } from '../../constants/inferenceDefaults';
import { PARAM_LABELS, formatParamValue } from '../../utils/samplingProvenance';
import { Input } from '../ui/Input';
import { Icon } from '../ui/Icon';
import { IconButton } from '../ui/IconButton';
import { Select } from '../ui/Select';
import { type InferenceFallback, fallbackCaption, fallbackValue } from './fallbackCaption';
import { ReasoningEffortField } from './ReasoningEffortField';
import './SamplingForm.css';

interface SamplingFormProps {
  value: SparseInferenceConfig | undefined | null;
  onChange: (newValue: SparseInferenceConfig) => void;
  disabled?: boolean;
  /**
   * What an empty field on this surface falls through to. Required: a surface
   * that does not say which rung it edits would otherwise inherit a caption
   * describing somebody else's.
   */
  fallback: InferenceFallback;
  /**
   * What this surface knows about the model whose defaults it is editing.
   *
   * Optional, and omitting it means *there is no model here* rather than "no
   * capabilities" — the global settings form is the surface with no model, and
   * it must still offer every control, because the defaults it edits will meet
   * models on both sides of every capability. Nothing is hidden when this is
   * absent; only the captions change.
   */
  capabilities?: FormCapabilities;
  /**
   * The inspector's in-place form: only the fields in `keys` that are set,
   * plus "Add a value" for the rest, under one caption that says where an
   * empty field's value comes from (in place of a caption per field).
   * Omitted, the whole form is drawn.
   */
  compact?: { keys: readonly CompactKey[]; caption: string };
}

/** A field the compact form can show: a sampling number, or the effort level. */
export type CompactKey = SamplingParamKey | 'reasoningEffort';

/** The fields typed as a number rather than slid. */
const NUMBER_FIELDS: ReadonlySet<SamplingParamKey> = new Set([
  'topK',
  'maxTokens',
  'dryAllowedLength',
  'dryPenaltyLastN',
  'reasoningBudgetTokens',
]);

/** The model facts that decide whether a control here applies at all. */
export interface FormCapabilities {
  /** Whether the model's chat template reads `reasoning_effort`. */
  reasoningEffort: TemplateSupport;
}

/**
 * DOM id for one parameter's control, so its label can point at it.
 *
 * Same `-description` convention as `SettingField`'s `settingDescriptionId`,
 * spelled out here rather than imported: this form is a top-level component
 * and has no business depending on the settings modal's internals.
 */
const paramId = (field: SamplingParamKey) => `inference-param-${field}`;

/** DOM id for the caption below a parameter, referenced by `aria-describedby`. */
const paramCaptionId = (field: SamplingParamKey) => `${paramId(field)}-description`;

/**
 * Tristate inference parameters form.
 * 
 * Each parameter can be:
 * - undefined (inherited from hierarchy)
 * - null (explicitly cleared)
 * - number (explicitly set)
 * 
 * An empty field is captioned with what it will actually fall through to on
 * this surface — see `fallbackCaption`, which is where the difference between
 * the three surfaces lives. A reset button appears when a value is explicitly
 * set, allowing users to clear it.
 */
export const SamplingForm: FC<SamplingFormProps> = ({
  value,
  onChange,
  disabled = false,
  fallback,
  capabilities,
  compact,
}) => {
  const config = useMemo(() => value || {}, [value]);
  // The compact form's fields on screen: those set when it opened, and any added since.
  const [shown, setShown] = useState<ReadonlySet<CompactKey>>(
    () => new Set(compact?.keys.filter((key) => config[key] != null) ?? []),
  );
  const quiet = compact !== undefined;

  const updateField = useCallback(<K extends keyof SparseInferenceConfig>(
    field: K,
    newValue: SparseInferenceConfig[K] | undefined
  ) => {
    const updated = { ...config, [field]: newValue };
    // Remove undefined values from the object
    if (newValue === undefined) {
      delete updated[field];
    }
    onChange(updated);
  }, [config, onChange]);

  const renderNumberInput = (field: SamplingParamKey) => {
    const { min, max, step } = INFERENCE_PARAMS[field];
    const label = PARAM_LABELS[field];
    // What the field will actually take if left empty. Null when gglib sends
    // nothing (llama.cpp's own default applies) or nothing is known yet.
    const inherited = fallbackValue(field, fallback);
    const placeholder = inherited === null ? undefined : formatParamValue(field, inherited);
    const caption = quiet ? null : fallbackCaption(field, fallback);
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;
    const inputId = paramId(field);

    return (
      <div className="flex flex-col gap-[0.4rem]">
        <label htmlFor={inputId} className="text-sm font-semibold text-text">{label}</label>
        <div className="flex items-center gap-[0.5rem]">
          <Input
            id={inputId}
            type="number"
            value={isSet ? currentValue : ''}
            onChange={(e) => {
              const val = e.target.value;
              updateField(field, val === '' ? undefined : Number(val));
            }}
            placeholder={placeholder}
            min={min}
            max={max}
            step={step}
            disabled={disabled}
            size="sm"
            className="flex-1 max-w-[150px]"
            aria-describedby={!isSet && caption ? paramCaptionId(field) : undefined}
          />
          {isSet && !disabled && (
            <IconButton
              type="button"
              label={`Reset ${label} to default`}
              title="Reset to default"
              size="sm"
              className="h-6 w-6"
              onClick={() => updateField(field, undefined)}
            >
              <Icon icon={X} size={14} />
            </IconButton>
          )}
        </div>
        {!isSet && caption && (
          <span id={paramCaptionId(field)} className="text-xs text-text-muted italic">
            {caption}
          </span>
        )}
      </div>
    );
  };

  const renderSlider = (field: SamplingParamKey) => {
    const { min, max, step } = INFERENCE_PARAMS[field];
    const label = PARAM_LABELS[field];
    const caption = quiet ? null : fallbackCaption(field, fallback);
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;
    // An unset thumb sits on the value that will actually apply, so it agrees
    // with the caption beside it. `?? min` covers the one case neither knows:
    // a value cleared from this very layer, not yet re-resolved.
    const displayValue = isSet ? currentValue : (fallbackValue(field, fallback) ?? min);
    const inputId = paramId(field);

    return (
      <div className="flex flex-col gap-[0.4rem]">
        <label htmlFor={inputId} className="text-sm font-semibold text-text">{label}</label>
        <div className="flex items-center gap-[0.75rem]">
          <input
            id={inputId}
            type="range"
            value={displayValue}
            onChange={(e) => {
              updateField(field, Number(e.target.value));
            }}
            min={min}
            max={max}
            step={step}
            disabled={disabled}
            className={`inference-param-slider ${!isSet ? 'is-default' : ''}`}
            aria-describedby={!isSet && caption ? paramCaptionId(field) : undefined}
          />
          {/* Only a set value has a number: an unset one's caption names what it inherits. */}
          <span className={`min-w-[100px] text-sm tabular-nums ${isSet ? 'text-text' : 'text-text-muted'}`}>
            {isSet ? currentValue.toFixed(2) : '—'}
          </span>
          {isSet && !disabled && (
            <IconButton
              type="button"
              label={`Reset ${label} to default`}
              title="Reset to default"
              size="sm"
              className="h-6 w-6"
              onClick={() => updateField(field, undefined)}
            >
              <Icon icon={X} size={14} />
            </IconButton>
          )}
        </div>
        {!isSet && caption && (
          <span id={paramCaptionId(field)} className="text-xs text-text-muted italic">
            {caption}
          </span>
        )}
      </div>
    );
  };

  const effortField = (
    <ReasoningEffortField
      value={config.reasoningEffort ?? undefined}
      onChange={(level) => updateField('reasoningEffort', level)}
      disabled={disabled}
      support={capabilities?.reasoningEffort}
    />
  );

  if (compact) {
    const field = (key: CompactKey) =>
      key === 'reasoningEffort' ? effortField : NUMBER_FIELDS.has(key) ? renderNumberInput(key) : renderSlider(key);
    const rest = compact.keys.filter((key) => !shown.has(key));
    return (
      <div className="flex flex-col gap-md">
        <p className="m-0 text-xs text-text-secondary">{compact.caption}</p>
        {compact.keys.filter((key) => shown.has(key)).map((key) => (
          <div key={key}>{field(key)}</div>
        ))}
        {rest.length > 0 && !disabled && (
          <Select
            size="sm"
            aria-label="Add a value"
            value=""
            onChange={(e) => setShown(new Set([...shown, e.target.value as CompactKey]))}
            className="max-w-[12rem]"
          >
            <option value="">Add a value</option>
            {rest.map((key) => (
              <option key={key} value={key}>
                {PARAM_LABELS[key]}
              </option>
            ))}
          </Select>
        )}
      </div>
    );
  }

  return (
    <div className="my-[1.5rem] p-[1rem] border border-border rounded-base bg-background-secondary">
      {/* No intro: what a blank field inherits differs by surface, and each caption says it. */}
      <h4 className="m-0 mb-[1rem] text-sm font-semibold text-text">Inference Parameters</h4>

      <div className="flex flex-col gap-[1rem]">
        {renderSlider('temperature')}
        {renderSlider('topP')}
        {renderNumberInput('topK')}
        {renderNumberInput('maxTokens')}
        {renderSlider('repeatPenalty')}
        {renderSlider('presencePenalty')}
        {renderSlider('minP')}
        {renderSlider('frequencyPenalty')}
        {renderSlider('dynatempRange')}
        {renderSlider('dynatempExponent')}
        {renderSlider('topNSigma')}
        {renderSlider('dryMultiplier')}
        {renderSlider('dryBase')}
        {renderNumberInput('dryAllowedLength')}
        {renderNumberInput('dryPenaltyLastN')}
      </div>

      {/*
        Kept apart from the sampling block above, and not merged into it: these
        two are not sampler settings. The budget is enforced by llama.cpp's own
        thinking cap and the effort is a variable handed to a chat template that
        may not read it, so grouping them with `temperature` would imply a
        uniformity of effect that only one of them has.
      */}
      <h5 className="m-0 mt-[1.5rem] mb-[0.5rem] text-sm font-semibold text-text">Reasoning</h5>
      <p className="m-0 mb-[1rem] text-xs text-text-muted leading-[1.4]">
        The budget is a hard cap llama.cpp enforces on every model — −1 defers to whatever the
        launch chose, 0 stops the model thinking altogether. The effort level is only a request to
        the chat template, which is why the two are separate fields.
      </p>
      <div className="flex flex-col gap-[1rem]">
        {effortField}
        {/*
          Never capability-gated: nothing about a template can stop the sampler
          honouring a budget, so this field shows for every model.
        */}
        {renderNumberInput('reasoningBudgetTokens')}
      </div>
    </div>
  );
};
