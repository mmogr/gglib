/**
 * Settings › Models › Models folder: where the library lives, saved when the
 * box is left or Enter is pressed, or picked with Change… in the desktop
 * app, with Reset back to the default and the folder's state (`MODELS_DIR_WORDS`:
 * writable, will be created, not writable). Undo puts the folder before back. The folder is its own write
 * (`setModelsDirectory`), not a settings field.
 *
 * @module places/settings/ModelsFolder
 */

import { useCallback, useState, type FC } from 'react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { StatusMark } from '../../components/status/StatusMark';
import { MODELS_DIR_WORDS } from '../../components/status/words';
import { useModelsDirectory } from '../../hooks/useModelsDirectory';
import { isDesktop, pickFolder } from '../../services/platform';
import { SettingField, settingDescriptionId } from './SettingField';
import { useUndoableWrite } from './useFieldSave';

const ID = 'models-dir-input';

/** Where the folder came from, as the daemon names its source. */
const SOURCES: Record<string, string> = {
  explicit: 'Chosen here or with gglib config.',
  environment: 'From the environment (.env).',
  default: 'The default.',
};


export const ModelsFolder: FC = () => {
  const { info, save } = useModelsDirectory();
  const write = useCallback((path: string) => save(path), [save]);
  const field = useUndoableWrite(info?.path ?? '', write);
  const [draft, setDraft] = useState<string | null>(null);

  const commit = async (path: string) => {
    const next = path.trim();
    if (next === '') {
      setDraft(null);
      return;
    }
    if (await field.commit(next)) setDraft(null);
  };
  const change = async () => {
    const picked = await pickFolder('Choose the models folder');
    if (picked) await commit(picked);
  };

  if (!info) return null;
  return (
    <SettingField
      id={ID}
      label="Models folder"
      description={SOURCES[info.source] ?? info.source}
      action={
        <span className="flex gap-sm">
          {isDesktop() && (
            <Button variant="link" size="sm" onClick={() => void change()}>
              Change…
            </Button>
          )}
          {info.path !== info.default_path && (
            <Button variant="link" size="sm" onClick={() => void commit(info.default_path)}>
              Reset
            </Button>
          )}
        </span>
      }
      save={field}
    >
      <Input
        id={ID}
        className="font-mono"
        value={draft ?? info.path}
        placeholder={info.default_path}
        aria-describedby={settingDescriptionId(ID)}
        spellCheck={false}
        disabled={field.saving}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => draft !== null && void commit(draft)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && draft !== null) void commit(draft);
          if (event.key === 'Escape') setDraft(null);
        }}
      />
      <span className="text-xs text-text-secondary">
        {info.state === 'writable' ? (
          MODELS_DIR_WORDS.writable
        ) : (
          <StatusMark kind="attention" word={MODELS_DIR_WORDS[info.state]} />
        )}
      </span>
    </SettingField>
  );
};
