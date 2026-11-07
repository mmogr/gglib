import { FC, KeyboardEvent, useState } from 'react';
import { X } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { IconButton } from '../../ui/IconButton';
import { Input } from '../../ui/Input';
import { cn } from '../../../utils/cn';
import type { ChatFolder } from '../../../hooks/useChatFolder';

/** Where a chat's folder is: this machine on desktop; on web, whose browser may be another device's, the machine gglib runs on. */
const whereOf = (native: boolean) => (native ? 'on this machine' : 'on the machine gglib runs on');

/** What a title adds for a chat on the paired machine's model: what the tools read goes there. */
const sentOf = (sentTo: string | null) => (sentTo ? ` What is read is sent to ${sentTo}'s model.` : '');

/** "Use a folder…"'s title: what choosing one does. */
export function folderOffer({ native, sentTo }: Pick<ChatFolder, 'native' | 'sentTo'>): string {
  const offer = `Choose a folder ${whereOf(native)} for this chat's file tools to read, read-only.`;
  return `${offer} Nothing is read until you do.${sentOf(sentTo)}`;
}

/** The chip's title with the folder at `path` chosen: what reads it, or that nothing does for a model that calls no tools. */
export function folderTitle(path: string, { sentTo, unread }: Pick<ChatFolder, 'sentTo' | 'unread'>): string {
  if (unread) return `This model calls no tools, so ${path} is not read. × forgets it.`;
  return `This chat's file tools read ${path}, read-only. Click to choose another.${sentOf(sentTo)}`;
}

/** The last name in `path`, which is what the chip shows; the whole path where it has none. */
export function folderName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '');
  return trimmed.split(/[\\/]/).pop() || path;
}

/** Choose another folder: the native dialog on desktop; on web, the field the path is typed in (`typing`). */
export function changeFolder(folder: ChatFolder, typing: (on: boolean) => void): void {
  if (folder.native) void folder.pick();
  else typing(true);
}

interface FolderChipProps {
  folder: ChatFolder;
  /** Whether the field for a typed path is open (web only). */
  typing: boolean;
  setTyping: (on: boolean) => void;
}

/**
 * The chat's folder in the composer (`useChatFolder`), drawn only once one
 * is set: its name, the whole path on hover, a click to choose another, and
 * × to clear it. Where a send was refused for it, the chip is a warning,
 * its title the reason. On web the path is typed, in a field that Enter
 * uses and Escape leaves; the `[+]` menu's "Use a folder…" opens it too.
 */
export const FolderChip: FC<FolderChipProps> = ({ folder, typing, setTyping }) => {
  const [draft, setDraft] = useState(folder.path ?? '');
  const { path, error } = folder;
  // The field opens on the folder as it stands, whichever control opened it.
  const [wasTyping, setWasTyping] = useState(typing);
  if (typing !== wasTyping) {
    setWasTyping(typing);
    if (typing) setDraft(path ?? '');
  }

  // No form of its own: the field sits in the composer's form, and a form
  // may not hold another. Enter uses the path and Escape leaves it, each
  // kept from the composer, so neither sends the message drafted there.
  const onKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter' && event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Enter' && draft.trim() !== '') folder.choose(draft);
    setTyping(false);
  };

  if (typing) {
    return (
      <span className="inline-block w-[220px]">
        <Input
          size="sm"
          aria-label="Folder path"
          placeholder="/absolute/path/to/folder"
          title="The absolute path of a folder on the machine gglib runs on. Enter uses it, Escape leaves it as it was."
          className="font-mono"
          value={draft}
          autoFocus
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKey}
          onBlur={() => setTyping(false)}
        />
      </span>
    );
  }

  if (path === null && error === null) return null;

  return (
    <span
      className={cn(
        'inline-flex max-w-[240px] items-center rounded-base text-xs',
        error ? 'bg-warning-subtle text-warning' : 'bg-surface-elevated text-text-secondary',
      )}
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="min-w-0 pr-xs font-mono text-xs font-normal text-current"
        title={error ? `${path ?? ''}${path ? ': ' : ''}${error}` : folderTitle(path ?? '', folder)}
        onClick={() => changeFolder(folder, setTyping)}
      >
        <span className="truncate">{path === null ? 'Folder' : folderName(path)}</span>
        {error && <span className="sr-only">: {error}</span>}
      </Button>
      <IconButton type="button" label="Clear the folder" size="sm" variant="ghost" className="text-current" onClick={folder.clear}>
        <Icon icon={X} size={12} />
      </IconButton>
    </span>
  );
};
