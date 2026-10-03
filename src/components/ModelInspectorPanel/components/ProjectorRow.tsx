import { FC, useEffect, useState } from 'react';
import { listProjectorChoices } from '../../../services/transport/api/models/local';
import type { ModelDetail } from '../../../types';
import type { ProjectorChoice } from '../../../types/generated/ProjectorChoice';
import { formatError } from '../../../utils/errors';
import { Banner } from '../../ui/Banner';
import { Select } from '../../ui/Select';
import { InfoRow } from './InfoRow';

/** The picker's own entry: no projector. */
const NONE = '';

interface ProjectorRowProps {
  modelId: number;
  /** The model's detail, which carries the linked projector's path. */
  detail: ModelDetail | undefined;
  /** The library's update: a path links, `null` unlinks. It rejects with the server's refusal. */
  onUpdateModel: (id: number, updates: { projectorPath: string | null }) => Promise<void>;
  /** Called after a change lands so the owner reads the detail again. */
  onChanged: () => void;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** One model's picker. Its choices, its wait and its refusal are that model's alone. */
const ProjectorPicker: FC<ProjectorRowProps> = ({ modelId, detail, onUpdateModel, onChanged }) => {
  const [choices, setChoices] = useState<ProjectorChoice[] | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    listProjectorChoices(modelId)
      .then((read) => current && setChoices(read))
      .catch((err: unknown) => current && setError(`Could not read the projector files: ${formatError(err)}`));
    return () => {
      current = false;
    };
  }, [modelId]);

  // The detail shown may still be the last model's while this one's is read.
  const linked = detail?.id === modelId ? (detail.projectorPath ?? NONE) : undefined;
  const offered = choices ?? [];
  const options =
    linked && !offered.some((c) => c.path === linked)
      ? [{ path: linked, name: fileName(linked) }, ...offered]
      : offered;

  const pick = async (value: string) => {
    setSaving(true);
    setError(null);
    try {
      await onUpdateModel(modelId, { projectorPath: value === NONE ? null : value });
      onChanged();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <InfoRow label="Projector">
      <div className="flex flex-col gap-sm">
        <Select
          size="sm"
          aria-label="Projector"
          title={linked || undefined}
          value={linked ?? NONE}
          disabled={linked === undefined || saving}
          onChange={(e) => void pick(e.target.value)}
        >
          <option value={NONE}>None (text only)</option>
          {options.map((choice) => (
            <option key={choice.path} value={choice.path} title={choice.path}>
              {choice.name}
            </option>
          ))}
        </Select>
        {error && <Banner variant="danger">{error}</Banner>}
        {choices?.length === 0 && !linked && (
          <p className="m-0 text-xs text-text-muted">
            No projector file is known here. Link one from a terminal:{' '}
            <code className="font-mono">{`gglib model update ${modelId} --projector <path>`}</code>
          </p>
        )}
      </div>
    </InfoRow>
  );
};

/**
 * The projector a model loads beside its weights — the GUI face of
 * `gglib model update --projector` / `--no-projector`. A pick is saved at
 * once; a file the server refuses is named in its own words and the link
 * stays as it was.
 *
 * The choices are read once per model and kept, so a projector just unlinked
 * is still offered here even when no other model loads it.
 */
export const ProjectorRow: FC<ProjectorRowProps> = (props) => <ProjectorPicker key={props.modelId} {...props} />;
