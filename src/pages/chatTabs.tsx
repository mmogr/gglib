import { MessageSquare, Terminal, X } from 'lucide-react';
import { Tabs, TabItem } from '../components/ui/Tabs';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';

export type ChatPageTabId = 'chat' | 'console';

/** Shared tab definitions for Chat/Console view switching */
export const CHAT_PAGE_TABS: TabItem<ChatPageTabId>[] = [
  { id: 'chat', label: 'Chat', icon: <MessageSquare size={16} /> },
  { id: 'console', label: 'Console', icon: <Terminal size={16} /> },
];

/**
 * The same switcher with nothing to switch to.
 *
 * A chat with the machine across the tunnel has no console here: the log,
 * the port and the uptime all belong to a process on the other side, which
 * this window cannot read. Offering the tab would open an empty panel
 * claiming a server that is not this one's to report.
 */
export const REMOTE_CHAT_PAGE_TABS: TabItem<ChatPageTabId>[] = CHAT_PAGE_TABS.filter(
  (tab) => tab.id !== 'console',
);

/**
 * The page's own controls, in the notebook head's margin: the view
 * switcher and Close, which stops the server and leaves the chat.
 */
export function ChatPageControls({
  activeTab,
  onTabChange,
  remote,
  onClose,
}: {
  activeTab: ChatPageTabId;
  onTabChange: (tab: ChatPageTabId) => void;
  remote: boolean;
  onClose: () => void;
}) {
  return (
    <>
      <Tabs<ChatPageTabId>
        tabs={remote ? REMOTE_CHAT_PAGE_TABS : CHAT_PAGE_TABS}
        activeId={activeTab}
        onChange={onTabChange}
        aria-label="Chat views"
        size="sm"
      />
      <Button
        variant="dangerGhost"
        size="sm"
        onClick={onClose}
        title="Stop server and close chat"
        leftIcon={<Icon icon={X} size={14} />}
      >
        Close
      </Button>
    </>
  );
}
