/**
 * How a field's last write went, under it: the daemon's refusal, or
 * "Saved." with Undo while Undo is offered. Nothing otherwise.
 *
 * @module places/settings/SaveLine
 */

import type { FC } from 'react';
import { Button } from '../../components/ui/Button';
import type { FieldSave } from './useFieldSave';

export const SaveLine: FC<{ save: Pick<FieldSave<unknown>, 'error' | 'undo'> }> = ({ save }) => {
  if (save.error !== null) {
    return (
      <p role="alert" className="m-0 text-xs text-danger">
        {save.error}
      </p>
    );
  }
  if (save.undo === null) return null;
  return (
    <p role="status" className="m-0 flex items-center gap-xs text-xs text-text-secondary">
      Saved.
      <Button variant="link" size="sm" className="text-xs" onClick={save.undo}>
        Undo
      </Button>
    </p>
  );
};
