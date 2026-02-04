import { FC, useCallback } from 'react';
import { X } from 'lucide-react';
import type { InferenceConfig } from '../../types';
import { Input } from '../ui/Input';
import { Icon } from '../ui/Icon';
import './InferenceParametersForm.css';

interface InferenceParametersFormProps {
  value: InferenceConfig | undefined | null;
  onChange: (newValue: InferenceConfig) => void;
  disabled?: boolean;
}

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

  const renderNumberInput = (
    field: keyof InferenceConfig,
    label: string,
    min: number,
    max: number,
    step: number,
    defaultHint: string
  ) => {
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;

    return (
      <div className="inference-param-row">
        <label className="inference-param-label">{label}</label>
        <div className="inference-param-input-group">
          <Input
            type="number"
            value={isSet ? currentValue : ''}
            onChange={(e) => {
              const val = e.target.value;
              updateField(field, val === '' ? undefined : Number(val));
            }}
            placeholder={defaultHint}
            min={min}
            max={max}
            step={step}
            disabled={disabled}
            size="sm"
            className="inference-param-input"
          />
          {isSet && !disabled && (
            <button
              type="button"
              className="inference-param-reset"
              onClick={() => updateField(field, undefined)}
              title="Reset to default"
              aria-label={`Reset ${label} to default`}
            >
              <Icon icon={X} size={14} />
            </button>
          )}
        </div>
        {!isSet && (
          <span className="inference-param-hint">
            Using default ({defaultHint})
          </span>
        )}
      </div>
    );
  };

  const renderSlider = (
    field: keyof InferenceConfig,
    label: string,
    min: number,
    max: number,
    step: number,
    defaultHint: string
  ) => {
    const currentValue = config[field];
    const isSet = currentValue !== undefined && currentValue !== null;
    const displayValue = isSet ? currentValue : parseFloat(defaultHint);

    return (
      <div className="inference-param-row">
        <label className="inference-param-label">{label}</label>
        <div className="inference-param-slider-group">
          <input
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
          />
          <span className="inference-param-value">
            {isSet ? currentValue.toFixed(2) : `${displayValue.toFixed(2)} (default)`}
          </span>
          {isSet && !disabled && (
            <button
              type="button"
              className="inference-param-reset"
              onClick={() => updateField(field, undefined)}
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

  return (
    <div className="inference-parameters-form">
      <h4 className="inference-parameters-title">Inference Parameters</h4>
      <p className="inference-parameters-description">
        Configure default sampling parameters. Leave blank to inherit from global defaults.
      </p>

      <div className="inference-parameters-grid">
        {renderSlider('temperature', 'Temperature', 0, 2, 0.05, '0.7')}
        {renderSlider('topP', 'Top P', 0, 1, 0.05, '0.95')}
        {renderNumberInput('topK', 'Top K', 1, 200, 1, '40')}
        {renderNumberInput('maxTokens', 'Max Tokens', 1, 8192, 1, '2048')}
        {renderSlider('repeatPenalty', 'Repeat Penalty', 0, 2, 0.05, '1.0')}
      </div>
    </div>
  );
};
