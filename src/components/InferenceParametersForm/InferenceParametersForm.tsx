import { FC, useCallback, useMemo } from 'react';
import { X } from 'lucide-react';
import type { InferenceConfig, SamplingParamKey } from '../../types';
import { INFERENCE_PARAMS } from '../../constants/inferenceDefaults';
import { PARAM_LABELS, formatParamValue } from '../../utils/samplingProvenance';
import { Input } from '../ui/Input';
import { Icon } from '../ui/Icon';
import { IconButton } from '../ui/IconButton';
import './InferenceParametersForm.css';

interface InferenceParametersFormProps {
  value: InferenceConfig | undefined | null;
  onChange: (newValue: InferenceConfig) => void;
  disabled?: boolean;
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
 * When a field is undefined/null, it shows placeholder text indicating the default.
 * A reset button appears when a value is explicitly set, allowing users to clear it.
 */
export const InferenceParametersForm: FC<InferenceParametersFormProps> = ({
  value,
  onChange,
  disabled = false,
}) => {
  const config = useMemo(() => value || {}, [value]);

  const updateField = useCallback(<K extends keyof InferenceConfig>(
    field: K,
    newValue: number | undefined
  ) => {
    const updated = { ...config, [field]: newValue };
    // Remove undefined values from the object
    if (newValue === undefined) {
      delete updated[field];
    }
    onChange(updated);
  }, [config, onChange]);

  const renderNumberInput = (field: SamplingParamKey) => {
    const { default: fallback, min, max, step } = INFERENCE_PARAMS[field];
    const label = PARAM_LABELS[field];
    // A parameter the floor leaves unset has no number to offer — no
    // placeholder to type over, and nothing to name in the caption.
    const defaultHint = fallback === null ? null : formatParamValue(field, fallback);
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;
    const inputId = paramId(field);

    return (
      <div className="flex flex-col gap-[0.4rem]">
        <label htmlFor={inputId} className="text-sm font-medium text-text">{label}</label>
        <div className="flex items-center gap-[0.5rem]">
          <Input
            id={inputId}
            type="number"
            value={isSet ? currentValue : ''}
            onChange={(e) => {
              const val = e.target.value;
              updateField(field, val === '' ? undefined : Number(val));
            }}
            placeholder={defaultHint ?? undefined}
            min={min}
            max={max}
            step={step}
            disabled={disabled}
            size="sm"
            className="flex-1 max-w-[150px]"
            aria-describedby={isSet ? undefined : paramCaptionId(field)}
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
        {!isSet && (
          <span id={paramCaptionId(field)} className="text-xs text-text-muted italic">
            {defaultHint === null
              ? 'No limit — generates until the context is full'
              : `Using default (${defaultHint})`}
          </span>
        )}
      </div>
    );
  };

  const renderSlider = (field: SamplingParamKey) => {
    const { default: fallback, min, max, step } = INFERENCE_PARAMS[field];
    const label = PARAM_LABELS[field];
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;
    // Every slider parameter has a floor; `?? min` only satisfies the type,
    // which is nullable for Max Tokens' sake — and that one is a number input.
    const displayValue = isSet ? currentValue : (fallback ?? min);
    const inputId = paramId(field);

    return (
      <div className="flex flex-col gap-[0.4rem]">
        <label htmlFor={inputId} className="text-sm font-medium text-text">{label}</label>
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
            aria-describedby={paramCaptionId(field)}
          />
          <span id={paramCaptionId(field)} className="min-w-[100px] text-sm text-text tabular-nums">
            {isSet ? currentValue.toFixed(2) : `${displayValue.toFixed(2)} (default)`}
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
      </div>
    );
  };

  return (
    <div className="my-[1.5rem] p-[1rem] border border-border rounded-base bg-background-secondary">
      <h4 className="m-0 mb-[0.5rem] text-base font-semibold text-text">Inference Parameters</h4>
      <p className="m-0 mb-[1rem] text-sm text-text-muted leading-[1.4]">
        Configure default sampling parameters. Leave blank to inherit from global defaults.
      </p>

      <div className="flex flex-col gap-[1rem]">
        {renderSlider('temperature')}
        {renderSlider('topP')}
        {renderNumberInput('topK')}
        {renderNumberInput('maxTokens')}
        {renderSlider('repeatPenalty')}
        {renderSlider('presencePenalty')}
        {renderSlider('minP')}
      </div>
    </div>
  );
};
