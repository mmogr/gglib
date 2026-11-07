/**
 * The sidebar's chats: "Chats ⌕ +" over the recent conversations, title
 * only, newest first, the open one marked. A reply running adds a small arc,
 * the row's one accessory; a reply not yet seen sets the title in bold. A
 * machine this one has joined adds its own chats as an "On <machine>" group
 * at the foot, read through the tunnel and never changed here. Hovering a
 * chat of this machine's shows ⋯: Rename, Delete… (asked first). ⌕ shows a
 * filter field; + starts a chat at once.
 *
 * The list is `services/chatRegistry`'s, the store the open chat changes, so
 * a rename or a first send there is here at once; it is read again when
 * this mounts and each time a reply ends. It scrolls inside the sidebar, so
 * the foot stays put.
 *
 * @module sidebar/ChatList
 */

import { useEffect, useId, useRef, useState, type FC } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { MoreHorizontal, Plus, Search } from 'lucide-react';
import { chatAddress, paths } from '../app/paths';
import { StatusMark } from '../components/status/StatusMark';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';
import { IconButton } from '../components/ui/IconButton';
import { Input } from '../components/ui/Input';
import { useToastContext } from '../contexts/ToastContext';
import { useConfirmContext } from '../contexts/ConfirmContext';
import { confirmDeleteChat } from '../places/chat/useConversationActions';
import { Menu } from '../places/library/Menu';
import { modelOf, refreshChats, renameChat, resetChats, useChats } from '../services/chatRegistry';
import { pairedName, useRemoteState } from '../services/remoteRegistry';
import type { ConversationSummary } from '../services/transport';
import { cn } from '../utils/cn';
import { formatError } from '../utils/errors';
import { ACTIVITY_POLL_MS, useConversationActivity } from './useConversationActivity';

/** The chats whose titles hold `query`, any case; all of them for none. */
function matching(list: readonly ConversationSummary[], query: string): ConversationSummary[] {
  const q = query.trim().toLowerCase();
  return q ? list.filter((c) => c.title.toLowerCase().includes(q)) : [...list];
}

/** Read the list again each time a reply ends: its title and its place in the order change. */
function useRefreshOnEnd(running: ReadonlySet<number>, source: 'this' | 'far'): void {
  const before = useRef(running);
  useEffect(() => {
    const ended = [...before.current].some((id) => !running.has(id));
    before.current = running;
    if (ended) void refreshChats(source);
  }, [running, source]);
}

interface RowProps {
  chat: ConversationSummary;
  current: boolean;
  running: boolean;
  unread: boolean;
  onOpen: () => void;
  /** Rename and Delete…, for this machine's chats; absent for a far one. */
  onRename?: (title: string) => Promise<void>;
  onDelete?: () => void;
}

/** One chat: its title, the arc while a reply runs, and ⋯ on hover. */
const ChatRow: FC<RowProps> = ({ chat, current, running, unread, onOpen, onRename, onDelete }) => {
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(chat.title);
  const unreadId = useId();

  if (renaming && onRename) {
    const commit = () => {
      setRenaming(false);
      const title = draft.trim();
      if (title && title !== chat.title) void onRename(title);
    };
    return (
      <li className="flex">
        <Input
          aria-label="Chat title"
          size="sm"
          autoFocus
          className="w-full"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            else if (e.key === 'Escape') setRenaming(false);
          }}
        />
      </li>
    );
  }

  return (
    <li className={cn('group/row relative flex items-center rounded-base hover:bg-surface-hover', current && 'bg-surface-hover')}>
      <Button
        variant="ghost"
        fullWidth
        className={cn(
          'h-9 justify-start px-sm text-sm hover:bg-transparent [&>span]:min-w-0',
          unread ? 'font-semibold text-text' : 'font-normal text-text-secondary',
          current && 'text-text',
        )}
        aria-current={current ? 'page' : undefined}
        aria-describedby={unread ? unreadId : undefined}
        title={chat.title}
        onClick={onOpen}
      >
        <span className="truncate">{chat.title}</span>
      </Button>
      {unread && (
        <span id={unreadId} className="sr-only">
          New reply
        </span>
      )}
      {running && <StatusMark kind="changing" label="Reply running" className="absolute right-sm group-hover/row:hidden group-focus-within/row:hidden" />}
      {onRename && onDelete && (
        <div className="absolute right-xs hidden group-hover/row:block group-focus-within/row:block">
          <Menu
            iconOnly
            name={`More for ${chat.title}`}
            label={<Icon icon={MoreHorizontal} size={14} />}
            items={[
              {
                label: 'Rename',
                onPick: () => {
                  setDraft(chat.title);
                  setRenaming(true);
                },
              },
              { label: 'Delete…', onPick: onDelete, danger: true },
            ]}
          />
        </div>
      )}
    </li>
  );
};

/**
 * The joined machine's own chats, at the foot: read while joined, gone after.
 * Keyed by that machine, so another one joining reads its own.
 */
const DeskGroup: FC<{ query: string; currentId: number | null; machine: string; onOpen: (id: number) => void }> = ({
  query,
  currentId,
  machine,
  onOpen,
}) => {
  const { list, fetched } = useChats('far');
  const activity = useConversationActivity(currentId, fetched, ACTIVITY_POLL_MS, 'far');
  useRefreshOnEnd(activity.running, 'far');
  useEffect(() => {
    void refreshChats('far');
    return () => resetChats('far');
  }, []);
  const shown = matching(list ?? [], query);
  if (shown.length === 0) return null;
  return (
    <section aria-label={`On ${machine}`} className="mt-sm flex flex-col">
      <h3 className="m-0 px-sm py-xs text-xs font-semibold text-text-muted">On {machine}</h3>
      <ul className="m-0 flex list-none flex-col gap-[2px] p-0">
        {shown.map((chat) => (
          <ChatRow
            key={chat.id}
            chat={chat}
            current={chat.id === currentId}
            running={activity.running.has(chat.id)}
            unread={activity.unread.has(chat.id)}
            onOpen={() => onOpen(chat.id)}
          />
        ))}
      </ul>
    </section>
  );
};

export function ChatList() {
  const location = useLocation();
  const navigate = useNavigate();
  const address = chatAddress(location);
  const { list, fetched, failed } = useChats('this');
  const { status } = useRemoteState();
  const { showToast } = useToastContext();
  const { confirm } = useConfirmContext();
  const [filtering, setFiltering] = useState(false);
  const [query, setQuery] = useState('');
  const currentId = address?.kind === 'saved' ? address.id : null;
  const activity = useConversationActivity(currentId, fetched, ACTIVITY_POLL_MS, 'this');
  useRefreshOnEnd(activity.running, 'this');

  useEffect(() => {
    void refreshChats('this');
  }, []);

  const onError = (message: string) => showToast(message, 'error');
  const remove = async (chat: ConversationSummary) => {
    if (!(await confirmDeleteChat(confirm, chat, onError))) return;
    if (chat.id !== currentId) return;
    // The open chat is gone: an empty one on its model takes its place.
    const model = modelOf(chat);
    const on = model === null ? {} : model.kind === 'local' ? { model: model.id } : { far: model.model.id };
    navigate(paths.newChat(on), { replace: true });
  };
  const shown = matching(list ?? [], query);

  return (
    <section aria-label="Chats" className="mt-sm flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-xs px-base">
        <h2 className="m-0 flex-1 text-sm font-normal text-text">Chats</h2>
        <IconButton
          variant="ghost"
          size="sm"
          label="Filter chats"
          aria-pressed={filtering}
          onClick={() => {
            setFiltering((on) => !on);
            setQuery('');
          }}
        >
          <Icon icon={Search} size={14} />
        </IconButton>
        <IconButton variant="ghost" size="sm" label="New chat" onClick={() => navigate(paths.newChat())}>
          <Icon icon={Plus} size={14} />
        </IconButton>
      </div>
      {filtering && (
        <div className="shrink-0 px-sm pt-xs">
          <Input
            type="search"
            size="sm"
            autoFocus
            aria-label="Filter chats"
            placeholder="Filter chats"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== 'Escape') return;
              e.preventDefault();
              setFiltering(false);
              setQuery('');
            }}
          />
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto px-sm pt-xs pb-sm">
        {list === null ? (
          <div role="status" aria-label="Loading chats" className="flex flex-col gap-sm px-sm py-xs">
            {[0, 1, 2].map((i) => (
              <span key={i} className="h-3 w-3/4 rounded-sm bg-surface-elevated" />
            ))}
          </div>
        ) : shown.length === 0 ? (
          <p className="m-0 px-sm py-xs text-xs text-text-muted">
            {query.trim() ? 'No matching chats' : failed ? "The chats couldn't be read." : 'No chats yet.'}
          </p>
        ) : (
          <ul className="m-0 flex list-none flex-col gap-[2px] p-0">
            {shown.map((chat) => (
              <ChatRow
                key={chat.id}
                chat={chat}
                current={chat.id === currentId}
                running={activity.running.has(chat.id)}
                unread={activity.unread.has(chat.id)}
                onOpen={() => navigate(paths.chat(chat.id))}
                onRename={(title) => renameChat(chat.id, title).catch((error: unknown) => onError(formatError(error)))}
                onDelete={() => void remove(chat)}
              />
            ))}
          </ul>
        )}
        {status?.connected && (
          <DeskGroup
            key={status.connected.ticket_fingerprint}
            query={query}
            currentId={address?.kind === 'far' ? address.id : null}
            machine={pairedName(status)}
            onOpen={(id) => navigate(paths.farChat(id))}
          />
        )}
      </div>
    </section>
  );
}
