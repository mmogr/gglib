import { FC } from 'react';
import { AlertTriangle } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { cn } from '../../../utils/cn';
import { Input } from '../../ui/Input';
import { Button } from '../../ui/Button';
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
        <Button type="button" variant="link" size="sm" onClick={onReset}>
          Reset to defaults
        </Button>
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
      <div className="flex gap-sm flex-wrap items-center" role="status" aria-live="polite">
        {info.exists && info.writable ? (
          <span className="text-xs text-text-muted">Directory exists · writable</span>
        ) : (
          <>
            <span
              className={cn(
                'inline-flex items-center gap-xs text-xs',
                info.exists ? 'text-text-muted' : 'text-warning',
              )}
            >
              {!info.exists && <Icon icon={AlertTriangle} size={12} />}
              {info.exists ? 'Directory exists' : 'Directory will be created'}
            </span>
            {!info.writable && (
              <span className="inline-flex items-center gap-xs text-xs text-danger">
                <Icon icon={AlertTriangle} size={12} />
                Not writable
              </span>
            )}
          </>
        )}
      </div>
    )}
  </SettingField>
);
