import { ReactNode } from 'react';
import { cn } from '../../utils/cn';

// Legacy type for ModelLibraryPanel backwards compatibility
export type SidebarTabId = 'models' | 'add';

export interface SidebarTab<T extends string = SidebarTabId> {
  id: T;
  label: string;
  icon?: ReactNode;
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
    <div className="flex items-center justify-between gap-md pb-md border-b border-border mb-md max-mobile:flex-wrap">
      <div className="flex gap-xs flex-1 min-w-0 max-mobile:w-full max-mobile:order-2" role="tablist">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            role="tab"
            aria-selected={activeTab === tab.id}
            className={cn(
              'flex items-center gap-xs px-md py-sm bg-transparent border-none border-b-2 border-b-transparent text-text-muted cursor-pointer text-sm font-medium transition-all whitespace-nowrap hover:text-text hover:bg-background-hover max-mobile:flex-1 max-mobile:justify-center max-mobile:px-xs max-mobile:py-sm',
              activeTab === tab.id && 'text-primary border-b-primary',
            )}
            onClick={() => onTabChange(tab.id)}
          >
            {tab.icon && <span className="text-base flex items-center justify-center [&>svg]:w-4 [&>svg]:h-4 max-mobile:hidden">{tab.icon}</span>}
            <span className="overflow-hidden text-ellipsis">{tab.label}</span>
          </button>
        ))}
      </div>
      {rightContent && (
        <div className="flex items-center gap-md shrink-0 ml-md max-mobile:order-1 max-mobile:ml-auto">
          {rightContent}
        </div>
      )}
    </div>
  );
}

export default SidebarTabs;
