import { MessageSquare, Terminal } from 'lucide-react';
import { TabItem } from '../components/ui/Tabs';

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
