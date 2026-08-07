import { MessageSquare, Terminal } from 'lucide-react';
import { TabItem } from '../components/ui/Tabs';

export type ChatPageTabId = 'chat' | 'console';

/** Shared tab definitions for Chat/Console view switching */
export const CHAT_PAGE_TABS: TabItem<ChatPageTabId>[] = [
  { id: 'chat', label: 'Chat', icon: <MessageSquare size={16} /> },
  { id: 'console', label: 'Console', icon: <Terminal size={16} /> },
];
