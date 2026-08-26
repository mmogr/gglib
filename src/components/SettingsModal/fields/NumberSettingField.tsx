import { FC, ReactNode } from 'react';
import { Input } from '../../ui/Input';
import type { NumericSettingSpec } from '../../../constants/settingsDefaults';
import { SettingField, settingDescriptionId } from './SettingField';

interface NumberSettingFieldProps {
  /** Field id, wired to the label via htmlFor and forwarded to the input. */
  id: string;
  label: string;
  /**
   * Default value and accepted range for this field, from
   * `src/constants/settingsDefaults.ts`. Taking the whole spec rather than
   * three loose props keeps a field's bounds and its fallback from being
   * sourced independently.
   */
  spec: NumericSettingSpec;
  value: string;
  onChange: (value: string) => void;
  description?: ReactNode;
  disabled?: boolean;
}

/**
 * A numeric settings input with its label, description, and default hint.
 *
 * Ports, sizes, and caps all want the same shape — a narrow number input,
 * min/max bounds, and a visible statement of what happens when the field is
 * left empty — and were repeating it call site by call site. Owning the
 * `Input` here (rather than leaving it to each caller, as plain `SettingField`
 * does) is what lets that statement reach the control as well as the hint,
 * without `SettingField` having to clone its children — both as the
 * placeholder and, via `aria-describedby`, as something a screen reader will
 * actually read out.
 *
 * A field with no fixed default renders its `unset.hint` as a plain sentence
 * rather than behind the "Default:" label, and its shorter `unset.placeholder`
 * in the box, which is only seven rem wide.
 */
export const NumberSettingField: FC<NumberSettingFieldProps> = ({
  id,
  label,
  spec,
  value,
  onChange,
  description,
  disabled,
}) => (
  <SettingField
    id={id}
    label={label}
    controlWidth="xs"
    defaultHint={spec.default ?? undefined}
    unsetHint={spec.default === null ? spec.unset.hint : undefined}
    description={description}
  >
    <Input
      id={id}
      type="number"
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder={spec.default ?? spec.unset.placeholder}
      min={spec.min}
      max={spec.max}
      disabled={disabled}
      aria-describedby={settingDescriptionId(id)}
    />
  </SettingField>
);
