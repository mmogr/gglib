import { ReactNode } from 'react';
import './SidebarTabs.css';

// Legacy type for ModelLibraryPanel backwards compatibility
export type SidebarTabId = 'models' | 'add';

export interface SidebarTab<T extends string = SidebarTabId> {
  id: T;
  label: string;
  icon?: string;
}

interface SidebarTabsProps<T extends string = SidebarTabId> {
  tabs: SidebarTab<T>[];
  activeTab: T;
  onTabChange: (tabId: T) => void;
  /** Optional content to render on the right side of the tabs (e.g., action buttons) */
  rightContent?: ReactNode;
}

// Generic version using function overload for better type inference
function SidebarTabs<T extends string = SidebarTabId>({
  tabs,
  activeTab,
  onTabChange,
  rightContent,
}: SidebarTabsProps<T>) {
  return (
    <div className="sidebar-tabs-container">
      <div className="sidebar-tabs" role="tablist">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            role="tab"
            aria-selected={activeTab === tab.id}
            className={`sidebar-tab ${activeTab === tab.id ? 'active' : ''}`}
            onClick={() => onTabChange(tab.id)}
          >
            {tab.icon && <span className="sidebar-tab-icon">{tab.icon}</span>}
            <span className="sidebar-tab-label">{tab.label}</span>
          </button>
        ))}
      </div>
      {rightContent && (
        <div className="sidebar-tabs-actions">
          {rightContent}
        </div>
      )}
    </div>
  );
}

export default SidebarTabs;
