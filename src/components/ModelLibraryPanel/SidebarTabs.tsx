import { FC, ReactNode } from 'react';
import './SidebarTabs.css';

export type SidebarTabId = 'models' | 'add';

export interface SidebarTab {
  id: SidebarTabId;
  label: string;
  icon?: string;
}

interface SidebarTabsProps {
  tabs: SidebarTab[];
  activeTab: SidebarTabId;
  onTabChange: (tabId: SidebarTabId) => void;
  /** Optional content to render on the right side of the tabs (e.g., action buttons) */
  rightContent?: ReactNode;
}

const SidebarTabs: FC<SidebarTabsProps> = ({
  tabs,
  activeTab,
  onTabChange,
  rightContent,
}) => {
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
};

export default SidebarTabs;
