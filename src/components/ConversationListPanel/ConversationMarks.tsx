import { FC } from 'react';
import { Circle, GitBranch, Loader2 } from 'lucide-react';
import { Chip } from '../ui/Chip';
import { Icon } from '../ui/Icon';

/**
 * A conversation's marks, in words: a reply running in it, one not yet
 * seen, and that it is a branch of another chat (ADR 0017).
 */
export const ConversationMarks: FC<{ running: boolean; unread: boolean; branch?: boolean }> = ({
  running,
  unread,
  branch = false,
}) => {
  if (!running && !unread && !branch) return null;
  return (
    <span className="flex gap-xs">
      {branch && (
        <Chip size="sm" leftIcon={<Icon icon={GitBranch} size={10} />}>
          Branch
        </Chip>
      )}
      {running && (
        <Chip size="sm" variant="success" leftIcon={<Icon icon={Loader2} size={10} className="animate-spin-360" />}>
          Running
        </Chip>
      )}
      {unread && (
        <Chip size="sm" variant="primary" leftIcon={<Icon icon={Circle} size={8} className="fill-current" />}>
          New
        </Chip>
      )}
    </span>
  );
};

/** The list button's name: how many conversations are running and new. */
export function conversationsLabel(running: number, unread: number): string {
  const parts = ['Conversations'];
  if (running > 0) parts.push(`${running} running`);
  if (unread > 0) parts.push(`${unread} new`);
  return parts.join(', ');
}
