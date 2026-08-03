import { FC } from 'react';
import { ToggleField } from './ToggleField';

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
  <ToggleField
    id="show-fit-indicators-input"
    label="Show memory fit indicators"
    checked={showFitIndicators}
    onChange={setShowFitIndicators}
    disabled={saving}
  >
    Display fit status indicators in the HuggingFace browser showing if models fit in your system
    memory
  </ToggleField>
);
