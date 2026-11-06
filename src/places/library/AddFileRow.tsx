/**
 * Add a GGUF file: a path and Add, at the top of the library. The web
 * types the path (a browser's picker gives a file name, not where the file
 * is); the desktop app fills it from the native picker and adds at once,
 * leaving the row open only to say why an add failed. The daemon reads the
 * file; its refusal is said under the path as it came.
 *
 * @module places/library/AddFileRow
 */

import { useState, type FC, type FormEvent } from 'react';
import { X } from 'lucide-react';
import { Banner } from '../../components/ui/Banner';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { IconButton } from '../../components/ui/IconButton';
import { Input } from '../../components/ui/Input';
import { refreshModels } from '../../services/modelEvents';
import { getTransport } from '../../services/transport';
import { formatError } from '../../utils/errors';

/** Add the file at `path` to the library. Throws the daemon's refusal. */
export async function addGgufFile(path: string): Promise<void> {
  await getTransport().addModel({ filePath: path });
  await refreshModels();
}

interface AddFileRowProps {
  /** The path the picker chose and the reason adding it failed, if it did. */
  initial?: { path: string; error: string };
  onClose: () => void;
}

export const AddFileRow: FC<AddFileRowProps> = ({ initial, onClose }) => {
  const [path, setPath] = useState(initial?.path ?? '');
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(initial?.error ?? null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setAdding(true);
    setError(null);
    try {
      await addGgufFile(path.trim());
      onClose();
    } catch (err) {
      setError(formatError(err));
    } finally {
      setAdding(false);
    }
  };

  return (
    <form aria-label="Add a GGUF file" onSubmit={(e) => void submit(e)} className="flex flex-col gap-xs px-md pb-sm">
      <div className="flex items-center gap-sm">
        <Input
          size="sm"
          aria-label="Path to a GGUF file"
          placeholder="/path/to/model.gguf"
          className="min-w-0 flex-1 font-mono"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          autoFocus
        />
        <Button type="submit" size="sm" disabled={adding || !path.trim()} isLoading={adding}>
          Add
        </Button>
        <IconButton label="Close" size="sm" onClick={onClose}>
          <Icon icon={X} size={14} />
        </IconButton>
      </div>
      {error && <Banner variant="danger">{error}</Banner>}
    </form>
  );
};
