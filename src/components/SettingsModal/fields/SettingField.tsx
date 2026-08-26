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
   * The value this field falls back to when left empty, e.g. "8080".
   * Rendered as an explicit "Default: 8080" hint below the control. A field
   * with no fixed default uses `unsetHint` instead and renders no such label —
   * the context window is the one such field, and captioning it with a default
   * is exactly what must not happen.
   *
   * This is the half of the story that survives: it stays put when the
   * field takes focus, and it stays put once a value has been entered, so
   * the fallback is still legible to someone deciding whether to clear the
   * field. The control's own placeholder is the other half — it shows the
   * same value in the box, where it can be typed over. Numeric fields get
   * both, from one source, via NumberSettingField; a caller supplying its
   * own control is responsible for its own placeholder.
   */
  defaultHint?: string;
  /**
   * What happens when the field is left empty, for a field with no fixed
   * default. Rendered as its own sentence, *not* behind the "Default:" label —
   * a field whose whole point is that it has no default must not be captioned
   * with one. Mutually exclusive with `defaultHint` in practice; if both are
   * given, both render.
   */
  unsetHint?: string;
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
 * gives every field the same label placement, control width, and
 * hint row.
 */
const controlWidthClasses = {
  xs: 'w-28',
  sm: 'w-48',
  full: 'w-full',
} as const;

/**
 * Id of the element holding a field's description and default hint.
 *
 * A control passes this to `aria-describedby` so the hint is announced with
 * the field. `SettingField` cannot attach it itself — `children` is an opaque
 * ReactNode, and reaching into it would mean cloneElement — so the id is
 * derived from the field id and both sides compute it from here.
 */
export const settingDescriptionId = (id: string): string => `${id}-description`;

export const SettingField: FC<SettingFieldProps> = ({
  id,
  label,
  children,
  controlWidth = 'full',
  defaultHint,
  unsetHint,
  description,
  action,
}) => (
  <div className="flex flex-col gap-sm">
    <Label htmlFor={id} size="sm">
      {label}
    </Label>
    <div className={controlWidthClasses[controlWidth]}>{children}</div>
    {(description || defaultHint || unsetHint || action) && (
      <Row justify="between" gap="sm" className="text-text-secondary text-sm">
        <span id={id ? settingDescriptionId(id) : undefined}>
          {description}
          {description && (defaultHint || unsetHint) && ' '}
          {defaultHint && <span className="text-text-muted">Default: {defaultHint}</span>}
          {unsetHint && <span className="text-text-muted">{unsetHint}</span>}
        </span>
        {action}
      </Row>
    )}
  </div>
);
