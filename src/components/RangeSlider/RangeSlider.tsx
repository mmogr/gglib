import { FC, useCallback, useRef } from 'react';
import { cn } from '../../utils/cn';
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
    <div className={cn("w-full py-xs", disabled && "opacity-50 pointer-events-none")}>
      {label && (
        <div className="flex justify-between items-center mb-sm">
          <span className="text-sm font-medium text-text">{label}</span>
          <span className="text-xs font-semibold text-primary bg-surface-elevated py-[2px] px-[6px] rounded-sm">
            {formatValue(minValue)} — {formatValue(maxValue)}
          </span>
        </div>
      )}
      
      <div className="relative h-[20px] flex items-center">
        {/* Background track */}
        <div className="absolute w-full h-1 bg-border rounded-[2px]" ref={trackRef}>
          {/* Highlighted range */}
          <div
            className={cn("absolute h-full rounded-[2px]", disabled ? "bg-border" : "bg-primary")}
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
          className="range-slider-thumb range-slider-thumb-min absolute w-full h-0 pointer-events-none appearance-none bg-transparent z-[3]"
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
          className="range-slider-thumb range-slider-thumb-max absolute w-full h-0 pointer-events-none appearance-none bg-transparent z-[4]"
          disabled={disabled}
          aria-label={`${label} maximum`}
        />
      </div>

      {/* Min/Max labels */}
      <div className="flex justify-between mt-xs">
        <span className="text-xs text-text-muted">{formatValue(min)}</span>
        <span className="text-xs text-text-muted">{formatValue(max)}</span>
      </div>
    </div>
  );
};

export default RangeSlider;
