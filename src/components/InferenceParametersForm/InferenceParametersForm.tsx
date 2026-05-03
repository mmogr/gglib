import { FC, useCallback } from 'react';
import { X } from 'lucide-react';
import type { InferenceConfig } from '../../types';
import { Input } from '../ui/Input';
import { Textarea } from '../ui/Textarea';
import { Icon } from '../ui/Icon';
import './InferenceParametersForm.css';

interface InferenceParametersFormProps {
  value: InferenceConfig | undefined | null;
  onChange: (newValue: InferenceConfig) => void;
  disabled?: boolean;
}

type NumericInferenceField =
  | 'temperature'
  | 'topP'
  | 'topK'
  | 'maxTokens'
  | 'repeatPenalty';

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
  const config = value || {};

  const updateNumberField = useCallback((
    field: NumericInferenceField,
    newValue: number | undefined
  ) => {
    const updated = { ...config, [field]: newValue };
    // Remove undefined values from the object
    if (newValue === undefined) {
      delete updated[field];
    }
    onChange(updated);
  }, [config, onChange]);

  const updateStopField = useCallback((newValue: string[] | undefined) => {
    const updated = { ...config, stop: newValue };
    if (newValue === undefined) {
      delete updated.stop;
    }
    onChange(updated);
  }, [config, onChange]);

  const renderNumberInput = (
    field: NumericInferenceField,
    label: string,
    min: number,
    max: number,
    step: number,
    defaultHint: string
  ) => {
    const inputId = `inference-${field}`;
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;

    return (
      <div className="flex flex-col gap-[0.4rem]">
        <label htmlFor={inputId} className="text-[0.85rem] font-medium text-text">{label}</label>
        <div className="flex items-center gap-[0.5rem]">
          <Input
            id={inputId}
            type="number"
            value={isSet ? currentValue : ''}
            onChange={(e) => {
              const val = e.target.value;
              updateNumberField(field, val === '' ? undefined : Number(val));
            }}
            placeholder={defaultHint}
            min={min}
            max={max}
            step={step}
            disabled={disabled}
            size="sm"
            className="flex-1 max-w-[150px]"
          />
          {isSet && !disabled && (
            <button
              type="button"
              className="flex items-center justify-center w-[24px] h-[24px] p-0 border-0 rounded-[4px] bg-transparent text-text-muted cursor-pointer transition-all duration-150 hover:bg-background-hover hover:text-text active:scale-95"
              onClick={() => updateNumberField(field, undefined)}
              title="Reset to default"
              aria-label={`Reset ${label} to default`}
            >
              <Icon icon={X} size={14} />
            </button>
          )}
        </div>
        {!isSet && (
          <span className="text-[0.75rem] text-text-muted italic">
            Using default ({defaultHint})
          </span>
        )}
      </div>
    );
  };

  const renderSlider = (
    field: NumericInferenceField,
    label: string,
    min: number,
    max: number,
    step: number,
    defaultHint: string
  ) => {
    const inputId = `inference-${field}`;
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;
    const displayValue = isSet ? currentValue : parseFloat(defaultHint);

    return (
      <div className="flex flex-col gap-[0.4rem]">
        <label htmlFor={inputId} className="text-[0.85rem] font-medium text-text">{label}</label>
        <div className="flex items-center gap-[0.75rem]">
          <input
            id={inputId}
            type="range"
            value={displayValue}
            onChange={(e) => {
              updateNumberField(field, Number(e.target.value));
            }}
            min={min}
            max={max}
            step={step}
            disabled={disabled}
            className={`inference-param-slider ${!isSet ? 'is-default' : ''}`}
          />
          <span className="min-w-[100px] text-[0.85rem] text-text tabular-nums">
            {isSet ? currentValue.toFixed(2) : `${displayValue.toFixed(2)} (default)`}
          </span>
          {isSet && !disabled && (
            <button
              type="button"
              className="flex items-center justify-center w-[24px] h-[24px] p-0 border-0 rounded-[4px] bg-transparent text-text-muted cursor-pointer transition-all duration-150 hover:bg-background-hover hover:text-text active:scale-95"
              onClick={() => updateNumberField(field, undefined)}
              title="Reset to default"
              aria-label={`Reset ${label} to default`}
            >
              <Icon icon={X} size={14} />
            </button>
          )}
        </div>
      </div>
    );
  };

  const renderStopInput = (defaultHint: string) => {
    const inputId = 'inference-stop';
    const currentValue = config.stop;
    const isSet = currentValue !== undefined && currentValue !== null;

    return (
      <div className="flex flex-col gap-[0.4rem]">
        <label htmlFor={inputId} className="text-[0.85rem] font-medium text-text">Stop Sequences</label>
        <div className="flex items-start gap-[0.5rem]">
          <Textarea
            id={inputId}
            value={isSet ? currentValue.join('\n') : ''}
            onChange={(e) => {
              const parsed = e.target.value
                .split(/[\n,]/)
                .map((token) => token.trim())
                .filter((token) => token.length > 0);
              updateStopField(parsed.length > 0 ? parsed : undefined);
            }}
            placeholder={defaultHint}
            disabled={disabled}
            size="sm"
            className="flex-1 font-mono text-xs"
          />
          {isSet && !disabled && (
            <button
              type="button"
              className="flex items-center justify-center w-[24px] h-[24px] p-0 border-0 rounded-[4px] bg-transparent text-text-muted cursor-pointer transition-all duration-150 hover:bg-background-hover hover:text-text active:scale-95"
              onClick={() => updateStopField(undefined)}
              title="Reset to default"
              aria-label="Reset Stop Sequences to default"
            >
              <Icon icon={X} size={14} />
            </button>
          )}
        </div>
        {!isSet ? (
          <span className="text-[0.75rem] text-text-muted italic">
            Using default ({defaultHint})
          </span>
        ) : (
          <span className="text-[0.75rem] text-text-muted">
            One sequence per line (commas also supported).
          </span>
        )}
      </div>
    );
  };

  return (
    <div className="my-[1.5rem] p-[1rem] border border-border rounded-[6px] bg-background-secondary">
      <h4 className="m-0 mb-[0.5rem] text-[0.95rem] font-semibold text-text">Inference Parameters</h4>
      <p className="m-0 mb-[1rem] text-[0.85rem] text-text-muted leading-[1.4]">
        Configure default sampling parameters. Leave blank to inherit from global defaults.
      </p>

      <div className="flex flex-col gap-[1rem]">
        {renderSlider('temperature', 'Temperature', 0, 2, 0.05, '0.7')}
        {renderSlider('topP', 'Top P', 0, 1, 0.05, '0.95')}
        {renderNumberInput('topK', 'Top K', 1, 200, 1, '40')}
        {renderNumberInput('maxTokens', 'Max Tokens', 1, 8192, 1, '2048')}
        {renderSlider('repeatPenalty', 'Repeat Penalty', 0, 2, 0.05, '1.0')}
        {renderStopInput('<|im_end|>\n</s>')}
      </div>
    </div>
  );
};
