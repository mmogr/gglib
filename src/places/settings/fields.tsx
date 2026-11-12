/**
 * The settings place's fields, each saving itself on change: a text or a
 * number written when the person leaves the box or presses Enter (a text
 * area only when it is left; Escape puts back the stored value in either),
 * held while its write is on its way, a checkbox or a choice the moment it moves.
 *
 * A field reads its stored value from the settings the provider holds and
 * writes `{ [key]: value }` under the binding's own name (ST-7), through
 * `useSettingSave`. Until a write succeeds the box shows what was typed, so
 * a refused write keeps it, with the refusal under it; after one it shows
 * what the daemon stored.
 *
 * @module places/settings/fields
 */

import { useState, type KeyboardEvent, type ReactNode } from 'react';
import { Button } from '../../components/ui/Button';
import { Checkbox } from '../../components/ui/Checkbox';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { Textarea } from '../../components/ui/Textarea';
import type { NumericSettingSpec } from '../../constants/settingsDefaults';
import { useSettings } from '../../hooks/useSettings';
import type { AppSettings, UpdateSettingsRequest } from '../../types';
import { SaveLine } from './SaveLine';
import { SettingField, settingDescriptionId } from './SettingField';
import { useSettingSave, type Wire } from './useFieldSave';

/** A setting the place reads back under the name it writes. */
type Key = keyof UpdateSettingsRequest & keyof AppSettings;

/** The stored value of `key`, `null` while nothing is stored or nothing has loaded. */
export function useStored<K extends Key>(key: K): Wire<K> {
  const { settings } = useSettings();
  return (settings?.[key] ?? null) as Wire<K>;
}

interface TextProps<K extends Key> {
  k: K;
  id: string;
  label: string;
  description?: ReactNode;
  defaultHint?: string;
  unsetHint?: string;
  note?: string;
  placeholder?: string;
  /** The stored value as the box shows it; a blank for nothing stored. */
  toText?: (stored: Wire<K>) => string;
  /** What the box writes; by default its text trimmed, and a blank clears the setting. */
  fromText?: (text: string) => Wire<K>;
  type?: 'text' | 'number';
  min?: string;
  max?: string;
  mono?: boolean;
  width?: 'xs' | 'sm' | 'full';
  multiline?: boolean;
  /** Offer Reset, which clears the setting back to its default. */
  resettable?: boolean;
}

const plainText = (stored: unknown) => (stored === null || stored === undefined ? '' : String(stored));
const trimmedOrNull = (text: string) => text.trim() || null;

/** A box saved when it is left or Enter is pressed. */
export function TextSetting<K extends Key>(props: TextProps<K>) {
  const { k, id, label, description, defaultHint, unsetHint, note, placeholder, type = 'text', min, max, mono, width, multiline, resettable } = props;
  const toText = props.toText ?? (plainText as (stored: Wire<K>) => string);
  const fromText = props.fromText ?? (trimmedOrNull as (text: string) => Wire<K>);
  const stored = useStored(k);
  const save = useSettingSave(k, stored);
  // What was typed and not yet stored; `null` shows the stored value.
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? toText(stored);

  const commit = async () => {
    if (draft === null) return;
    if (await save.commit(fromText(draft))) setDraft(null);
  };
  const reset = async () => {
    if (await save.commit(null as Wire<K>)) setDraft(null);
  };
  const common = {
    id,
    value: shown,
    placeholder,
    'aria-describedby': settingDescriptionId(id),
    'aria-invalid': save.error !== null || undefined,
    // Held while its write is on its way, as the checkboxes and choices are:
    // an edit made then would be compared with the value before the write.
    // Read-only, not disabled: disabling a focused box drops the keyboard's
    // focus to the page, and it does not come back.
    readOnly: save.saving,
    'aria-busy': save.saving || undefined,
    onBlur: () => void commit(),
    onKeyDown: (event: KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      if (save.saving) return;
      if (event.key === 'Enter' && !multiline) void commit();
      if (event.key === 'Escape') setDraft(null);
    },
  };

  return (
    <SettingField
      id={id}
      label={label}
      controlWidth={width}
      defaultHint={defaultHint}
      unsetHint={unsetHint}
      description={description}
      note={note}
      action={
        resettable && stored !== null ? (
          <Button variant="link" size="sm" onClick={() => void reset()}>
            Reset
          </Button>
        ) : undefined
      }
      save={save}
    >
      {multiline ? (
        <Textarea {...common} rows={3} onChange={(event) => setDraft(event.target.value)} />
      ) : (
        <Input
          {...common}
          type={type}
          min={min}
          max={max}
          className={mono ? 'font-mono' : undefined}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => setDraft(event.target.value)}
        />
      )}
    </SettingField>
  );
}

/** A number, or `null` for an empty box: what the daemon is sent as typed, so it says what it refuses. */
export const numberOrNull = (text: string): number | null => (text.trim() === '' ? null : Number(text.trim()));

/**
 * A number with its default and range from `constants/settingsDefaults`:
 * the default in the box as its placeholder and under it as "Default: N",
 * announced with the field (ST-2), and the range the backend accepts (ST-3).
 */
export function NumberSetting<K extends Key>({ spec, ...props }: Omit<TextProps<K>, 'type' | 'min' | 'max' | 'defaultHint' | 'unsetHint' | 'placeholder' | 'fromText'> & { spec: NumericSettingSpec }) {
  return (
    <TextSetting
      {...props}
      type="number"
      width="xs"
      min={spec.min}
      max={spec.max}
      placeholder={spec.default ?? spec.unset.placeholder}
      defaultHint={spec.default ?? undefined}
      unsetHint={spec.default === null ? spec.unset.hint : undefined}
      fromText={numberOrNull as (text: string) => Wire<K>}
    />
  );
}

interface ToggleProps<K extends Key> {
  k: K;
  id: string;
  label: string;
  children?: ReactNode;
  note?: string;
  /** Whether the stored value reads as on; by default only a stored `true`. */
  on?: (stored: Wire<K>) => boolean;
}

/** A checkbox saved the moment it is ticked or unticked. */
export function ToggleSetting<K extends Key>({ k, id, label, children, note, on }: ToggleProps<K>) {
  const stored = useStored(k);
  const save = useSettingSave(k, stored);
  const checked = on ? on(stored) : stored === true;
  return (
    <div className="flex flex-col gap-xs">
      <Checkbox
        id={id}
        checked={checked}
        // Held by ignoring a change while its write is on its way, not by
        // disabling, which would drop the keyboard's focus.
        aria-busy={save.saving || undefined}
        onChange={(event) => {
          if (save.saving) return;
          void save.commit(event.target.checked as Wire<K>);
        }}
        label={<span className="font-semibold">{label}</span>}
        description={children}
      />
      {note && <p className="m-0 text-xs text-text-muted">{note}</p>}
      <SaveLine save={save} />
    </div>
  );
}

export interface Choice {
  value: string;
  label: string;
}

interface ChoiceProps<K extends Key> {
  k: K;
  id: string;
  label: string;
  description?: ReactNode;
  choices: Choice[];
  /** The choice the stored value is. */
  toText: (stored: Wire<K>) => string;
  /** What picking a choice writes. */
  fromText: (value: string) => Wire<K>;
  disabled?: boolean;
}

/** A choice saved the moment it is picked. */
export function ChoiceSetting<K extends Key>({ k, id, label, description, choices, toText, fromText, disabled }: ChoiceProps<K>) {
  const stored = useStored(k);
  const save = useSettingSave(k, stored);
  return (
    <SettingField id={id} label={label} description={description} save={save}>
      <Select
        id={id}
        value={toText(stored)}
        aria-describedby={settingDescriptionId(id)}
        disabled={disabled}
        aria-busy={save.saving || undefined}
        onChange={(event) => {
          if (save.saving) return;
          void save.commit(fromText(event.target.value));
        }}
      >
        {choices.map((choice) => (
          <option key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </Select>
    </SettingField>
  );
}
