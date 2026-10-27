/**
 * A stored secret in a form: held or not, shown only when asked for
 * (ADR 0027).
 *
 * A read never carries a secret, only whether one is stored, so a field
 * holding one shows `MASK`. Show and Copy ask the daemon's one reveal each
 * time they are used. Replace types a new value; Clear, where allowed,
 * removes it on save. A field with nothing stored is the input alone, and
 * leaving it empty keeps things as they are.
 *
 * @module components/ui/SecretField
 */

import { FC, useState } from 'react';
import { Copy, Eye, EyeOff } from 'lucide-react';
import { getTransport } from '../../services/transport';
import type { SecretRef } from '../../types/generated/SecretRef';
import { formatError } from '../../utils/errors';
import { Button } from './Button';
import { Icon } from './Icon';
import { IconButton } from './IconButton';
import { Input } from './Input';
import { SecretInput } from './SecretInput';

/** What a held secret is shown as. */
export const MASK = '••••••••';

/** What the form does with a secret on save. */
export type SecretEdit =
  | { kind: 'keep' }
  | { kind: 'replace'; value: string }
  | { kind: 'clear' };

/** Leave the stored secret as it is. */
export const KEEP: SecretEdit = { kind: 'keep' };

/**
 * What an edit writes: `undefined` to keep the stored value (so the request
 * leaves the field out), the new value, or `null` to clear it. A replace
 * left blank keeps, never `""`.
 */
export function editedValue(edit: SecretEdit): string | null | undefined {
  if (edit.kind === 'clear') return null;
  if (edit.kind === 'replace' && edit.value.trim()) return edit.value;
  return undefined;
}

export interface SecretFieldProps {
  /** The control's id, for a label's `htmlFor`. */
  id: string;
  /** What the secret is, for the controls' accessible names. */
  label: string;
  /** Whether a value is stored. */
  isSet: boolean;
  /** Which stored secret Show and Copy reveal. */
  secret: SecretRef;
  /** What the form will do with it on save. */
  edit: SecretEdit;
  onEdit: (edit: SecretEdit) => void;
  /** Whether a stored value may be cleared. */
  clearable?: boolean;
  /** What clearing does, said once it is chosen. */
  clearNote?: string;
  /** What the input says while nothing is stored. */
  placeholder?: string;
  disabled?: boolean;
  /** How a Show or Copy went, for the caller to raise as a toast. */
  onNotice?: (message: string, kind: 'success' | 'error') => void;
}

/** The value `secret` names, refusing when nothing is stored. */
async function revealed(secret: SecretRef): Promise<string> {
  const value = await getTransport().revealSecret(secret);
  if (value === null) throw new Error('nothing is stored');
  return value;
}

/**
 * Write the revealed value to the clipboard. The reveal goes in as a promise
 * where `ClipboardItem` takes one, so the write starts inside the click and
 * WebKit keeps the user gesture.
 */
function copySecret(secret: SecretRef): Promise<void> {
  if (typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
    const text = revealed(secret).then((value) => new Blob([value], { type: 'text/plain' }));
    return navigator.clipboard.write([new ClipboardItem({ 'text/plain': text })]);
  }
  return revealed(secret).then((value) => navigator.clipboard.writeText(value));
}

export const SecretField: FC<SecretFieldProps> = ({
  id,
  label,
  isSet,
  secret,
  edit,
  onEdit,
  clearable = false,
  clearNote,
  placeholder,
  disabled = false,
  onNotice,
}) => {
  // A revealed value, with the secret it is the value of. Shown only while
  // that is still the secret here and it is kept: a field handed another
  // secret, or cleared and restored, shows the mask again.
  const which = JSON.stringify(secret);
  const [shown, setShown] = useState<{ of: string; value: string } | null>(null);
  const showing = shown?.of === which ? shown.value : null;

  if (edit.kind === 'clear') {
    return (
      <div className="flex flex-wrap items-center gap-sm">
        <span className="text-sm text-text-secondary">Cleared when you save.</span>
        {clearNote && <span className="text-xs text-text-muted">{clearNote}</span>}
        <Button type="button" variant="ghost" size="sm" onClick={() => onEdit(KEEP)} disabled={disabled}>
          Undo
        </Button>
      </div>
    );
  }

  if (edit.kind === 'replace' || !isSet) {
    return (
      <div className="flex items-center gap-sm">
        <SecretInput
          id={id}
          label={label}
          value={edit.kind === 'replace' ? edit.value : ''}
          onChange={(value) => onEdit({ kind: 'replace', value })}
          placeholder={isSet ? `Leave empty to keep the current ${label}` : placeholder}
          disabled={disabled}
        />
        {isSet && (
          <Button type="button" variant="ghost" size="sm" onClick={() => onEdit(KEEP)} disabled={disabled}>
            Cancel
          </Button>
        )}
      </div>
    );
  }

  /** Change the edit, and put the mask back first. */
  const editTo = (next: SecretEdit) => {
    setShown(null);
    onEdit(next);
  };
  const toggleShown = () => {
    if (showing !== null) {
      setShown(null);
      return;
    }
    revealed(secret)
      .then((value) => setShown({ of: which, value }))
      .catch((err) => onNotice?.(`Could not show the ${label}: ${formatError(err)}`, 'error'));
  };
  const copy = () => {
    copySecret(secret)
      .then(() => onNotice?.(`Copied the ${label}.`, 'success'))
      .catch((err) => onNotice?.(`Could not copy the ${label}: ${formatError(err)}`, 'error'));
  };

  return (
    <div className="flex items-center gap-sm">
      <Input
        id={id}
        className="flex-1 min-w-0 font-mono"
        value={showing ?? MASK}
        aria-label={label}
        readOnly
        disabled={disabled}
      />
      <IconButton
        type="button"
        label={showing === null ? `Show the ${label}` : `Hide the ${label}`}
        size="sm"
        onClick={toggleShown}
        disabled={disabled}
      >
        <Icon icon={showing === null ? Eye : EyeOff} size={14} />
      </IconButton>
      <IconButton type="button" label={`Copy the ${label}`} size="sm" onClick={copy} disabled={disabled}>
        <Icon icon={Copy} size={14} />
      </IconButton>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() => editTo({ kind: 'replace', value: '' })}
        disabled={disabled}
      >
        Replace
      </Button>
      {clearable && (
        <Button
          type="button"
          variant="dangerGhost"
          size="sm"
          onClick={() => editTo({ kind: 'clear' })}
          disabled={disabled}
        >
          Clear
        </Button>
      )}
    </div>
  );
};
