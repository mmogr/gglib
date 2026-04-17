/**
 * Inline-editable text field for council agent properties.
 *
 * Renders as plain text by default; click to switch to an input/textarea.
 * Commits on blur or Enter. Supports single-line (input) and multi-line
 * (textarea) modes.
 *
 * @module components/Council/Setup/EditableTextField
 */

import { type FC, useState, useRef, useEffect } from 'react';
import { cn } from '../../../utils/cn';

interface EditableTextFieldProps {
  value: string;
  onChange: (value: string) => void;
  multiline?: boolean;
  disabled?: boolean;
  className?: string;
  placeholder?: string;
  'aria-label'?: string;
}

export const EditableTextField: FC<EditableTextFieldProps> = ({
  value, onChange, multiline, disabled, className, placeholder, ...props
}) => {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const ref = useRef<HTMLInputElement | HTMLTextAreaElement>(null);

  useEffect(() => { setDraft(value); }, [value]);
  useEffect(() => { if (editing) ref.current?.focus(); }, [editing]);

  const commit = () => {
    setEditing(false);
    const trimmed = draft.trim();
    if (trimmed && trimmed !== value) onChange(trimmed);
    else setDraft(value);
  };

  if (disabled || !editing) {
    return (
      <span
        className={cn(
          'cursor-pointer rounded px-xs hover:bg-background-hover transition-colors',
          disabled && 'cursor-not-allowed opacity-50',
          className,
        )}
        onClick={() => !disabled && setEditing(true)}
        role="button"
        tabIndex={disabled ? -1 : 0}
        onKeyDown={(e) => { if (e.key === 'Enter' && !disabled) setEditing(true); }}
        aria-label={props['aria-label']}
      >
        {value || placeholder}
      </span>
    );
  }

  const shared = {
    value: draft,
    onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => setDraft(e.target.value),
    onBlur: commit,
    className: cn(
      'w-full rounded border border-border bg-background-input px-xs py-0.5 text-inherit outline-none focus:border-primary',
      className,
    ),
    'aria-label': props['aria-label'],
  };

  if (multiline) {
    return (
      <textarea
        ref={ref as React.RefObject<HTMLTextAreaElement>}
        rows={3}
        onKeyDown={(e) => { if (e.key === 'Escape') { setDraft(value); setEditing(false); } }}
        {...shared}
      />
    );
  }

  return (
    <input
      ref={ref as React.RefObject<HTMLInputElement>}
      type="text"
      onKeyDown={(e) => {
        if (e.key === 'Enter') commit();
        if (e.key === 'Escape') { setDraft(value); setEditing(false); }
      }}
      {...shared}
    />
  );
};
