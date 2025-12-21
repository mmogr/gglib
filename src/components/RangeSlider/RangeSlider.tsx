import { FC, useCallback, useRef } from 'react';
import './RangeSlider.css';

interface RangeSliderProps {
  /** Minimum possible value */
  min: number;
  /** Maximum possible value */
  max: number;
  /** Current minimum selected value */
  minValue: number;
  /** Current maximum selected value */
  maxValue: number;
  /** Step size for the slider */
  step?: number;
  /** Callback when range changes */
  onChange: (min: number, max: number) => void;
  /** Format function for displaying values */
  formatValue?: (value: number) => string;
  /** Whether the slider is disabled */
  disabled?: boolean;
  /** Label for the slider */
  label?: string;
}

/**
 * Dual-handle range slider component for filtering by numeric ranges.
 * Dependency-free implementation using two overlapping range inputs.
 */
const RangeSlider: FC<RangeSliderProps> = ({
  min,
  max,
  minValue,
  maxValue,
  step = 1,
  onChange,
  formatValue = (v) => v.toString(),
  disabled = false,
  label,
}) => {
  const trackRef = useRef<HTMLDivElement>(null);
  
  // Calculate the percentage position of a value
  const getPercent = useCallback(
    (value: number) => {
      if (max === min) return 50;
      return ((value - min) / (max - min)) * 100;
    },
    [min, max]
  );

  const minPercent = getPercent(minValue);
  const maxPercent = getPercent(maxValue);

  const handleMinChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = Math.min(Number(e.target.value), maxValue - step);
    onChange(value, maxValue);
  };

  const handleMaxChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = Math.max(Number(e.target.value), minValue + step);
    onChange(minValue, value);
  };

  return (
    <div className={`range-slider ${disabled ? 'range-slider-disabled' : ''}`}>
      {label && (
        <div className="range-slider-header">
          <span className="range-slider-label">{label}</span>
          <span className="range-slider-values">
            {formatValue(minValue)} — {formatValue(maxValue)}
          </span>
        </div>
      )}
      
      <div className="range-slider-container">
        {/* Background track */}
        <div className="range-slider-track" ref={trackRef}>
          {/* Highlighted range */}
          <div
            className="range-slider-range"
            style={{
              left: `${minPercent}%`,
              width: `${maxPercent - minPercent}%`,
            }}
          />
        </div>

        {/* Min thumb input */}
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={minValue}
          onChange={handleMinChange}
          className="range-slider-thumb range-slider-thumb-min"
          disabled={disabled}
          aria-label={`${label} minimum`}
        />

        {/* Max thumb input */}
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={maxValue}
          onChange={handleMaxChange}
          className="range-slider-thumb range-slider-thumb-max"
          disabled={disabled}
          aria-label={`${label} maximum`}
        />
      </div>

      {/* Min/Max labels */}
      <div className="range-slider-bounds">
        <span className="range-slider-bound-min">{formatValue(min)}</span>
        <span className="range-slider-bound-max">{formatValue(max)}</span>
      </div>
    </div>
  );
};

export default RangeSlider;
