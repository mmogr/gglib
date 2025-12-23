import { MessageSquare, Terminal } from 'lucide-react';
import { SidebarTab } from '../components/ModelLibraryPanel/SidebarTabs';

export type ChatPageTabId = 'chat' | 'console';

/** Shared tab definitions for Chat/Console view switching */
export const CHAT_PAGE_TABS: SidebarTab<ChatPageTabId>[] = [
  { id: 'chat', label: 'Chat', icon: <MessageSquare size={16} /> },
  { id: 'console', label: 'Console', icon: <Terminal size={16} /> },
];
