import { FC, FormEvent, useState } from 'react';
import { FolderOpen, FolderX } from 'lucide-react';
import { Chip } from '../../ui/Chip';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import type { ChatFolder } from '../../../hooks/useChatFolder';

/** Where a chat's folder is: this machine on desktop; on web, whose browser may be another device's, the machine gglib runs on. */
const whereOf = (native: boolean) => (native ? 'on this machine' : 'on the machine gglib runs on');

/** What a title adds for a chat on the paired machine's model: what the tools read goes there. */
const sentOf = (sentTo: string | null) => (sentTo ? ` What is read is sent to ${sentTo}'s model.` : '');

/** The chip's title with no folder chosen. */
export function folderOffer({ native, sentTo }: Pick<ChatFolder, 'native' | 'sentTo'>): string {
  const offer = `Choose a folder ${whereOf(native)} for this chat's file tools to read, read-only.`;
  return `${offer} Nothing is read until you do.${sentOf(sentTo)}`;
}

/** The chip's title with the folder at `path` chosen: what reads it, or that nothing does for a model that calls no tools. */
export function folderTitle(path: string, { sentTo, unread }: Pick<ChatFolder, 'sentTo' | 'unread'>): string {
  if (unread) return `This model calls no tools, so ${path} is not read. Clear forgets it.`;
  return `This chat's file tools read ${path}, read-only. Click Change to choose another.${sentOf(sentTo)}`;
}

/** The last name in `path`, which is what the chip shows; the whole path where it has none. */
export function folderName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '');
  return trimmed.split(/[\\/]/).pop() || path;
}

/**
 * The chat's folder in the composer's margin (`useChatFolder`). With none,
 * a chip named Folder that chooses one. With one, its name, the whole path
 * on hover, Clear on the chip and Change beside it. Where a send was refused
 * for it, the chip is a warning, its title the reason. Change opens the
 * native dialog on desktop, and on web a field for the path typed, which
 * Enter uses and Escape leaves.
 */
export const FolderChip: FC<{ folder: ChatFolder }> = ({ folder }) => {
  const [typing, setTyping] = useState(false);
  const [draft, setDraft] = useState('');
  const { path, error } = folder;

  const change = () => {
    if (folder.native) {
      void folder.pick();
      return;
    }
    setDraft(path ?? '');
    setTyping(true);
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (draft.trim() !== '') folder.choose(draft);
    setTyping(false);
  };

  if (typing) {
    return (
      <form onSubmit={submit} className="w-[220px]">
        <Input
          size="sm"
          aria-label="Folder path"
          placeholder="/absolute/path/to/folder"
          title="The absolute path of a folder on the machine gglib runs on. Enter uses it, Escape leaves it as it was."
          value={draft}
          autoFocus
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => event.key === 'Escape' && setTyping(false)}
          onBlur={() => setTyping(false)}
        />
      </form>
    );
  }

  if (path === null) {
    return (
      <Chip
        leftIcon={<Icon icon={error ? FolderX : FolderOpen} size={12} />}
        variant={error ? 'warning' : 'neutral'}
        onClick={change}
        title={error ?? folderOffer(folder)}
      >
        Folder
      </Chip>
    );
  }

  return (
    <span className="inline-flex items-center gap-xs max-w-[240px]">
      <Chip
        leftIcon={<Icon icon={error ? FolderX : FolderOpen} size={12} />}
        variant={error ? 'warning' : 'primary'}
        title={error ? `${path}: ${error}` : folderTitle(path, folder)}
        onRemove={folder.clear}
        removeLabel="Clear the folder"
      >
        {folderName(path)}
        {error && <span className="sr-only">: {error}</span>}
      </Chip>
      <Chip onClick={change} title="Choose another folder for this chat">
        Change
      </Chip>
    </span>
  );
};
