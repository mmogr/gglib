import { FC } from 'react';
import { Row } from '../../primitives';

interface DisplaySettingsProps {
  showFitIndicators: boolean;
  setShowFitIndicators: (value: boolean) => void;
  saving: boolean;
}

/**
 * Display-only toggles. Currently just the memory-fit indicator switch;
 * kept as its own file so future display toggles have somewhere to land
 * that isn't the ports group or the advanced section.
 */
export const DisplaySettings: FC<DisplaySettingsProps> = ({
  showFitIndicators,
  setShowFitIndicators,
  saving,
}) => (
  <div>
    <label className="flex items-center gap-sm cursor-pointer select-none">
      <input
        type="checkbox"
        className="w-[18px] h-[18px] accent-primary cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed"
        checked={showFitIndicators}
        onChange={(e) => setShowFitIndicators(e.target.checked)}
        disabled={saving}
      />
      <span className="font-semibold text-text">Show memory fit indicators</span>
    </label>
    <Row justify="between" gap="sm" className="text-text-secondary text-sm">
      <span>
        Display fit status indicators in the HuggingFace browser showing if models fit in your
        system memory
      </span>
    </Row>
  </div>
);
