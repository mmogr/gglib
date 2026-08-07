import { FC, ReactNode } from 'react';
import { Label, Row } from '../../primitives';

interface SettingFieldProps {
  /** Field id, wired to the label via htmlFor and forwarded to the control. */
  id?: string;
  label: string;
  children: ReactNode;
  /**
   * Width of the control column. Numeric fields (ports, sizes) use "xs" so
   * a 4-digit value doesn't get a full-width input.
   */
  controlWidth?: 'xs' | 'sm' | 'full';
  /**
   * The value this field falls back to when left empty, e.g. "4096".
   * Rendered as an explicit "Default: 4096" hint below the control.
   *
   * Settings inputs start empty and only backfill from the server value
   * when one has been explicitly set (see SettingsModal.tsx), so an unset
   * field previously showed nothing but its HTML placeholder — visually
   * identical to a field the user just hasn't typed in yet. This makes
   * the fallback an explicit, always-visible fact instead of a value
   * that vanishes the instant the field gains focus.
   */
  defaultHint?: string;
  /** Additional description text, shown alongside the default hint. */
  description?: ReactNode;
  /** Optional trailing action (e.g. "Reset to default") on the hint row. */
  action?: ReactNode;
}

/**
 * One label / control / hint group for a settings form.
 *
 * GeneralSettings.tsx used to repeat this structure by hand for every
 * field with slightly different markup each time. Centralising it here
 * is also where the placeholder-as-default defect gets fixed once,
 * instead of once per field.
 */
const controlWidthClasses = {
  xs: 'w-28',
  sm: 'w-48',
  full: 'w-full',
} as const;

export const SettingField: FC<SettingFieldProps> = ({
  id,
  label,
  children,
  controlWidth = 'full',
  defaultHint,
  description,
  action,
}) => (
  <div className="flex flex-col gap-sm">
    <Label htmlFor={id} size="sm">
      {label}
    </Label>
    <div className={controlWidthClasses[controlWidth]}>{children}</div>
    {(description || defaultHint || action) && (
      <Row justify="between" gap="sm" className="text-text-secondary text-sm">
        <span>
          {description}
          {description && defaultHint && ' '}
          {defaultHint && <span className="text-text-muted">Default: {defaultHint}</span>}
        </span>
        {action}
      </Row>
    )}
  </div>
);
