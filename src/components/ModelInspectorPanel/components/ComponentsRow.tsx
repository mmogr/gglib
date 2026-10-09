import { FC, useEffect, useState } from 'react';
import { listComponentChoices } from '../../../services/transport/api/models/local';
import type { ComponentChanges } from '../../../services/transport/types/models';
import type { ComponentChoices, ComponentRole, ModelDetail } from '../../../types';
import type { ProjectorChoice } from '../../../types/generated/ProjectorChoice';
import { formatError } from '../../../utils/errors';
import { componentRoleLabel } from '../../../utils/imageFamily';
import { Banner } from '../../ui/Banner';
import { Select } from '../../ui/Select';
import { InfoRow } from './InfoRow';

/** Each picker's own entry: no file linked in that role. */
const NONE = '';

interface ComponentsRowProps {
  modelId: number;
  /** The model's detail, which carries each linked component and the roles still missing. */
  detail: ModelDetail | undefined;
  /** The library's update: a path links the role, `null` unlinks it. It rejects with the server's refusal. */
  onUpdateModel: (id: number, updates: { components: ComponentChanges }) => Promise<void>;
  /** Called after a change lands so the owner reads the detail again. */
  onChanged: () => void;
}

interface RolePickerProps {
  modelId: number;
  role: ComponentRole;
  /** The linked file's path, `NONE` for no link, `undefined` while this model's detail is not read. */
  linked: string | undefined;
  /** Whether a file is at the linked path, as the detail was read. */
  present: boolean;
  /** The files offered for this role; `null` until they are read. */
  offered: ProjectorChoice[] | null;
  onUpdateModel: ComponentsRowProps['onUpdateModel'];
  onChanged: () => void;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** One role's picker. Its wait and its refusal are that role's alone. */
const RolePicker: FC<RolePickerProps> = ({ modelId, role, linked, present, offered, onUpdateModel, onChanged }) => {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const label = componentRoleLabel(role);
  const known = offered ?? [];
  const options =
    linked && !known.some((c) => c.path === linked) ? [{ path: linked, name: fileName(linked) }, ...known] : known;

  const pick = async (value: string) => {
    setSaving(true);
    setError(null);
    try {
      await onUpdateModel(modelId, { components: { [role]: value === NONE ? null : value } });
      onChanged();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex flex-col gap-xs">
      <span className="text-xs text-text-secondary">{label}</span>
      <Select
        size="sm"
        aria-label={label}
        title={linked || undefined}
        value={linked ?? NONE}
        disabled={linked === undefined || saving}
        onChange={(e) => void pick(e.target.value)}
      >
        <option value={NONE}>None</option>
        {options.map((choice) => (
          <option key={choice.path} value={choice.path} title={choice.path}>
            {choice.name}
          </option>
        ))}
      </Select>
      {linked && !present && <p className="m-0 text-xs text-warning">No file is at the linked path.</p>}
      {error && <Banner variant="danger">{error}</Banner>}
      {offered?.length === 0 && !linked && (
        <p className="m-0 text-xs text-text-muted">
          No {label} file is known here. Link one from a terminal:{' '}
          <code className="font-mono">{`gglib model update ${modelId} --component ${role}=<path>`}</code>
        </p>
      )}
    </div>
  );
};

/** One model's pickers. Its choices and its refusals are that model's alone. */
const ComponentPickers: FC<ComponentsRowProps> = ({ modelId, detail, onUpdateModel, onChanged }) => {
  const [choices, setChoices] = useState<ComponentChoices[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    listComponentChoices(modelId)
      .then((read) => current && setChoices(read))
      .catch((err: unknown) => current && setError(`Could not read the component files: ${formatError(err)}`));
    return () => {
      current = false;
    };
  }, [modelId]);

  // The detail shown may still be the last model's while this one's is read.
  const own = detail?.id === modelId ? detail : undefined;
  // The roles are the daemon's answer, in the recipe's order; until it comes,
  // the detail's linked and missing roles together are the same set.
  const roles =
    choices?.map((c) => c.role) ?? [...(own?.components.map((c) => c.role) ?? []), ...(own?.missingComponents ?? [])];

  if (roles.length === 0 && !error) return null;

  return (
    <InfoRow label="Components">
      <div className="flex flex-col gap-md">
        {error && <Banner variant="danger">{error}</Banner>}
        {roles.map((role) => {
          const link = own?.components.find((c) => c.role === role);
          return (
            <RolePicker
              key={role}
              modelId={modelId}
              role={role}
              linked={own ? (link?.path ?? NONE) : undefined}
              present={link?.present ?? false}
              offered={choices ? (choices.find((c) => c.role === role)?.files ?? []) : null}
              onUpdateModel={onUpdateModel}
              onChanged={onChanged}
            />
          );
        })}
      </div>
    </InfoRow>
  );
};

/**
 * The files an image model draws with beside its weights, one picker for
 * each role its family needs — the GUI face of `gglib model update
 * --component <role>=<path>` / `--no-component <role>`. A pick is saved at
 * once; a file the server refuses is named in its own words under its role
 * and the link stays as it was.
 *
 * Nothing for a model whose family needs no separate file. The choices are
 * read once per model and kept, so a file just unlinked is still offered.
 */
export const ComponentsRow: FC<ComponentsRowProps> = (props) => <ComponentPickers key={props.modelId} {...props} />;
