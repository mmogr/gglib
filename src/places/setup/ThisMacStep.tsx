/**
 * Setup's step 1, This Mac: what gglib found, and where models go.
 *
 * The machine is said in the daemon's words: the download its llama.cpp
 * install would take (`macOS ARM64 (Metal)`), and the memory models are
 * sized against, as the recommendation names it (`43.2 GiB unified
 * memory`), or with no recommendation the machine's RAM (`48.0 GiB system
 * RAM`); either is left out when the daemon has none. Where the folder was
 * set is not said: the route names a stored folder and `GGLIB_MODELS_DIR`
 * alike as `environment`. The models folder
 * is the resolved one, "(will be created)" while it is not there, with
 * Change: the folder typed, saved through the models-directory route, then
 * read back, so what is shown is what the daemon resolved.
 *
 * @module places/setup/ThisMacStep
 */

import { useState, type FC } from 'react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { getTransport } from '../../services/transport';
import type { ModelsDirectoryInfo } from '../../types';
import type { RecommendationDto, SetupStatus } from '../../types/setup';
import { formatError } from '../../utils/errors';
import { formatBudget } from '../../utils/format';
import { Item, Problem, Step } from './Step';

interface ThisMacStepProps {
  status: SetupStatus;
  rec: RecommendationDto | null;
  folder: ModelsDirectoryInfo;
  onFolder: (folder: ModelsDirectoryInfo) => void;
  onNext: () => void;
}

/** The folder's state beside its path, when there is one to say. */
function folderNote(folder: ModelsDirectoryInfo): string | null {
  if (!folder.exists) return '(will be created)';
  if (!folder.writable) return '(not writable)';
  return null;
}

export const ThisMacStep: FC<ThisMacStepProps> = ({ status, rec, folder, onFolder, onNext }) => {
  const [typed, setTyped] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The budget is the recommendation's, with Rust's own label. With none
  // there is no budget to name, and the GUI does not work one out.
  const facts = [status.llamaPlatformDescription, rec ? formatBudget(rec) : null].filter(Boolean).join(' · ');
  const note = folderNote(folder);

  const saveFolder = async (path: string) => {
    setSaving(true);
    setError(null);
    try {
      const transport = getTransport();
      await transport.setModelsDirectory(path);
      onFolder(await transport.getModelsDirectory());
      setTyped(null);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Step
      n={1}
      title="This Mac"
      actions={
        <Button onClick={onNext} disabled={typed !== null}>
          Continue
        </Button>
      }
    >
      {facts && <p className="m-0 text-sm text-text">{facts}</p>}
      {typed === null ? (
        <Item
          name="Models folder"
          state={
            <>
              <code className="min-w-0 break-all font-mono text-text">{folder.path}</code>
              {note && <span>{note}</span>}
            </>
          }
          action={
            <Button variant="ghost" size="sm" onClick={() => setTyped(folder.path)}>
              Change
            </Button>
          }
        >
          {error !== null && <Problem>{error}</Problem>}
        </Item>
      ) : (
        <form
          className="flex flex-col gap-xs"
          onSubmit={(event) => {
            event.preventDefault();
            if (typed.trim()) void saveFolder(typed.trim());
          }}
        >
          <div className="flex items-center gap-sm">
            <Input
              aria-label="Models folder"
              className="min-w-0 flex-1 font-mono"
              value={typed}
              onChange={(event) => {
                setTyped(event.target.value);
                setError(null);
              }}
            />
            <Button type="submit" variant="secondary" size="sm" isLoading={saving} disabled={!typed.trim()}>
              Save
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setTyped(null)} disabled={saving}>
              Cancel
            </Button>
          </div>
          {error !== null && <Problem>{error}</Problem>}
        </form>
      )}
    </Step>
  );
};
