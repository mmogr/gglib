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
    <div className="flex flex-wrap items-center justify-between gap-md pb-md border-b border-border mb-md mobile:flex-nowrap">
      <div className="flex gap-xs w-full order-2 mobile:flex-1 mobile:w-auto mobile:order-none" role="tablist">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            role="tab"
            aria-selected={activeTab === tab.id}
            className={cn(
              'flex items-center gap-xs px-xs py-sm bg-transparent border-none border-b-2 border-b-transparent text-text-muted cursor-pointer text-sm font-medium transition-all whitespace-nowrap hover:text-text hover:bg-background-hover flex-1 justify-center mobile:flex-initial mobile:justify-start mobile:px-md',
              activeTab === tab.id && 'text-primary border-b-primary',
            )}
            onClick={() => onTabChange(tab.id)}
          >
            {tab.icon && <span className="hidden mobile:flex text-base items-center justify-center [&>svg]:w-4 [&>svg]:h-4">{tab.icon}</span>}
            <span className="overflow-hidden text-ellipsis">{tab.label}</span>
          </button>
        ))}
      </div>
      {rightContent && (
        <div className="flex items-center gap-md order-1 ml-auto mobile:order-none mobile:ml-md mobile:shrink-0">
          {rightContent}
        </div>
      )}
    </div>
  );
}

export default SidebarTabs;
