import { FC, useRef, useState } from 'react';
import { Button } from '../ui/Button';
import { Chip } from '../ui/Chip';
import type { TaskSuite, TuneTask } from '../../types/benchmark';

interface SuitePickerProps {
  disabled: boolean;
  /** `null` while a custom suite is selected but no valid file is loaded yet. */
  onSuiteChange: (suite: TaskSuite | null) => void;
}

/**
 * Default-vs-custom task-suite selector with the client-side JSON file parse.
 * The uploaded file is the same shape `--task-suite path.json` reads from disk.
 */
export const SuitePicker: FC<SuitePickerProps> = ({ disabled, onSuiteChange }) => {
  const [suiteMode, setSuiteMode] = useState<'default' | 'custom'>('default');
  const [taskCount, setTaskCount] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const pickMode = (mode: 'default' | 'custom') => {
    setSuiteMode(mode);
    onSuiteChange(mode === 'default' ? { source: 'default' } : null);
  };

  const handleFileChange = async (file: File | null) => {
    setError(null);
    setTaskCount(null);
    setFileName(file?.name ?? null);
    onSuiteChange(null);
    if (!file) return;
    try {
      const parsed = JSON.parse(await file.text()) as unknown;
      if (!Array.isArray(parsed) || parsed.length === 0) {
        throw new Error('Expected a non-empty JSON array of tasks');
      }
      setTaskCount(parsed.length);
      onSuiteChange({ source: 'custom', tasks: parsed as TuneTask[] });
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <div className="flex flex-col gap-xs">
      <label className="text-xs font-semibold text-text">Task Suite</label>
      <div className="flex gap-sm">
        <Chip selected={suiteMode === 'default'} disabled={disabled} onClick={() => pickMode('default')}>
          Default
        </Chip>
        <Chip selected={suiteMode === 'custom'} disabled={disabled} onClick={() => pickMode('custom')}>
          Custom
        </Chip>
      </div>
      {suiteMode === 'custom' && (
        <div className="flex flex-col gap-xs">
          <input
            ref={fileInputRef}
            type="file"
            tabIndex={-1}
            aria-hidden="true"
            accept=".json,application/json"
            disabled={disabled}
            onChange={(e) => void handleFileChange(e.target.files?.[0] ?? null)}
            className="sr-only"
          />
          <Button
            variant="secondary"
            size="sm"
            disabled={disabled}
            onClick={() => fileInputRef.current?.click()}
          >
            Choose task file…
          </Button>
          {fileName && <span className="text-xs text-text-muted">{fileName}</span>}
          {taskCount != null && <p className="text-xs text-success m-0">{taskCount} task(s) loaded</p>}
          {error && <p className="text-xs text-danger m-0">{error}</p>}
        </div>
      )}
    </div>
  );
};
