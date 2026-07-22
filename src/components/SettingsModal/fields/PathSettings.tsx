import { FC } from 'react';
import { cn } from '../../../utils/cn';
import { Input } from '../../ui/Input';
import type { ModelsDirectoryInfo } from '../../../types';
import { SettingField } from './SettingField';

interface PathSettingsProps {
  pathInput: string;
  setPathInput: (value: string) => void;
  info: ModelsDirectoryInfo | null;
  sourceDescription: string | null;
  onReset: () => void;
  saving: boolean;
}

/**
 * Models directory field plus its live "exists" / "writable" status pills.
 */
export const PathSettings: FC<PathSettingsProps> = ({
  pathInput,
  setPathInput,
  info,
  sourceDescription,
  onReset,
  saving,
}) => (
  <SettingField
    id="models-dir-input"
    label="Default Download Path"
    description={sourceDescription}
    action={
      info?.defaultPath && (
        <button
          type="button"
          className="bg-none border-none text-primary cursor-pointer text-sm underline p-0"
          onClick={onReset}
        >
          Reset to defaults
        </button>
      )
    }
  >
    <Input
      id="models-dir-input"
      value={pathInput}
      onChange={(event) => setPathInput(event.target.value)}
      placeholder="/path/to/models"
      disabled={saving}
    />
    {info && (
      <div className="flex gap-sm flex-wrap" role="status" aria-live="polite">
        <span
          className={cn(
            'px-2 py-[2px] rounded-base text-sm',
            info.exists ? 'bg-success-subtle text-success' : 'bg-warning-subtle text-warning',
          )}
          aria-label={info.exists ? 'Directory exists' : 'Directory will be created (warning)'}
        >
          {info.exists ? 'Directory exists' : 'Directory will be created'}
        </span>
        <span
          className={cn(
            'px-2 py-[2px] rounded-base text-sm',
            info.writable ? 'bg-success-subtle text-success' : 'bg-danger-subtle text-danger',
          )}
          aria-label={info.writable ? 'Writable' : 'Not writable (error)'}
        >
          {info.writable ? 'Writable' : 'Not writable'}
        </span>
      </div>
    )}
  </SettingField>
);
