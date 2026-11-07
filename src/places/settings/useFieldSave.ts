/**
 * One field's write, as the settings place makes it: on change, alone, with
 * Undo for five seconds after.
 *
 * A write sends the one field it is for and nothing else, and nothing at all
 * when the value is the one stored (ST-1): a field left alone cannot put
 * back what a terminal wrote meanwhile (#1059). Undo writes back what was
 * stored before, `null` included, which clears the setting again. A failed
 * write is the field's own: its message stays under it until the next write
 * from it, and the field keeps what was typed.
 *
 * @module places/settings/useFieldSave
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { useSettings } from '../../hooks/useSettings';
import type { UpdateSettingsRequest } from '../../types';
import { formatError } from '../../utils/errors';

/** How long Undo is offered after a write. */
export const UNDO_MS = 5_000;

/**
 * Whether two wire values say the same thing; lists and objects by their
 * contents, where a key set to `null` and a key left out both say "unset".
 */
export function sameValue(a: unknown, b: unknown): boolean {
  if (a === b || (a == null && b == null)) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  return [...keys].every((key) => sameValue(left[key], right[key]));
}

export interface FieldSave<V> {
  /**
   * Write `next`, unless it is what is stored. `before` is what Undo puts
   * back, when that is not the stored value the field read (a secret, which
   * no read carries). Resolves whether the field now holds `next`.
   */
  commit: (next: V, before?: V) => Promise<boolean>;
  saving: boolean;
  /** The last write's refusal, as the daemon gave it. */
  error: string | null;
  /** Put back what the last write replaced: offered for `UNDO_MS` after it. */
  undo: (() => void) | null;
}

/** A write of one value through `write`, against `stored`, with Undo after. */
export function useUndoableWrite<V>(stored: V, write: (value: V) => Promise<unknown>): FieldSave<V> {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [undoTo, setUndoTo] = useState<{ value: V } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const stopTimer = () => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  };
  useEffect(() => stopTimer, []);

  const run = useCallback(
    async (value: V): Promise<boolean> => {
      setSaving(true);
      setError(null);
      try {
        await write(value);
        return true;
      } catch (err) {
        setError(formatError(err));
        return false;
      } finally {
        setSaving(false);
      }
    },
    [write],
  );

  const commit = useCallback(
    async (next: V, before?: V): Promise<boolean> => {
      const previous = before === undefined ? stored : before;
      if (before === undefined && sameValue(next, stored)) {
        setError(null);
        return true;
      }
      stopTimer();
      setUndoTo(null);
      const written = await run(next);
      if (written) {
        setUndoTo({ value: previous });
        timer.current = setTimeout(() => setUndoTo(null), UNDO_MS);
      }
      return written;
    },
    [stored, run],
  );

  const undo = undoTo
    ? () => {
        stopTimer();
        setUndoTo(null);
        void run(undoTo.value);
      }
    : null;

  return { commit, saving, error, undo };
}

/** The value a settings write sends as `key`. */
export type Wire<K extends keyof UpdateSettingsRequest> = Exclude<UpdateSettingsRequest[K], undefined>;

/** One setting's write: `{ [key]: value }` and no other field. */
export function useSettingSave<K extends keyof UpdateSettingsRequest>(key: K, stored: Wire<K>): FieldSave<Wire<K>> {
  const { save } = useSettings();
  const write = useCallback(
    (value: Wire<K>) => save({ [key]: value } as UpdateSettingsRequest, { quiet: true }),
    [key, save],
  );
  return useUndoableWrite(stored, write);
}
