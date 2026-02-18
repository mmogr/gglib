import { FC, useState } from 'react';
import { BookOpenText, PlusCircle, RefreshCcw } from 'lucide-react';
import { GgufModel, ServerInfo, HfModelSummary, ModelFilterOptions } from '../../types';
import SidebarTabs, { SidebarTabId, SidebarTab } from './SidebarTabs';
import ModelsListContent from './ModelsListContent';
import AddDownloadContent, { AddDownloadSubTab } from './AddDownloadContent';
import ProxyControl from '../ProxyControl';
import { FilterPopover, FilterState } from '../FilterPopover';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { cn } from '../../utils/cn';

interface ModelLibraryPanelProps {
  // Models list props
  models: GgufModel[];
  selectedModelId: number | null;
  onSelectModel: (id: number | null) => void;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  tags: string[];
  servers: ServerInfo[];
  
  // Filter props
  filterOptions: ModelFilterOptions | null;
  filters: FilterState;
  onFiltersChange: (filters: FilterState) => void;
  onClearFilters: () => void;
  
  // Add/Download props
  onModelAdded: (filePath: string) => Promise<void>;
  activeSubTab?: AddDownloadSubTab;
  onSubTabChange?: (subtab: AddDownloadSubTab) => void;
  /** Optional error message if the backend download system failed to initialize */
  downloadSystemError?: string | null;
  
  // HuggingFace model selection (for preview in inspector)
  onSelectHfModel?: (model: HfModelSummary | null) => void;
  selectedHfModelId?: string | null;
  
  // Tab control (optional - can be controlled externally or internally)
  activeTab?: SidebarTabId;
  onTabChange?: (tab: SidebarTabId) => void;
}

const SIDEBAR_TABS: SidebarTab[] = [
  { id: 'models', label: 'Your Models', icon: <BookOpenText size={18} /> },
  { id: 'add', label: 'Add Models', icon: <PlusCircle size={18} /> },
];

const ModelLibraryPanel: FC<ModelLibraryPanelProps> = ({
  models,
  selectedModelId,
  onSelectModel,
  loading,
  error,
  onRefresh,
  searchQuery,
  onSearchChange,
  tags,
  servers,
  filterOptions,
  filters,
  onFiltersChange,
  onClearFilters,
  onModelAdded,
  activeSubTab,
  onSubTabChange,
  downloadSystemError,
  onSelectHfModel,
  selectedHfModelId,
  activeTab: externalActiveTab,
  onTabChange: externalOnTabChange,
}) => {
  // Internal tab state (used if not controlled externally)
  const [internalActiveTab, setInternalActiveTab] = useState<SidebarTabId>('models');
  const [filterPopoverOpen, setFilterPopoverOpen] = useState(false);
  const activeTab = externalActiveTab ?? internalActiveTab;
  
  const handleTabChange = (tab: SidebarTabId) => {
    if (externalOnTabChange) {
      externalOnTabChange(tab);
    } else {
      setInternalActiveTab(tab);
    }
  };

  // Check if any filters are active (for badge indicator)
  const hasActiveFilters = 
    filters.paramRange !== null ||
    filters.contextRange !== null ||
    filters.selectedQuantizations.length > 0 ||
    filters.selectedTags.length > 0;

  const handleSwitchToAddTab = () => {
    handleTabChange('add');
  };

  // Error state
  if (error) {
    return (
      <div className="flex flex-col h-full min-h-0 overflow-y-auto overflow-x-hidden border-r border-border relative flex-1 max-md:h-auto max-md:max-h-none max-md:border-r-0 max-md:border-b max-md:border-border bg-surface">
        <div className="p-base border-b border-border bg-background shrink-0">
          <SidebarTabs
            tabs={SIDEBAR_TABS}
            activeTab={activeTab}
            onTabChange={handleTabChange}
          />
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col">
          <div className="error-container">
            <p className="error-message">Error: {error}</p>
            <button onClick={onRefresh} className="retry-button">
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Header actions (refresh + proxy)
  const headerActions = (
    <>
      {activeTab === 'models' && (
        <Button 
          onClick={onRefresh}
          variant="ghost"
          size="sm"
          disabled={loading}
          title="Refresh models"
          iconOnly
        >
          <RefreshCcw size={16} />
        </Button>
      )}
      <ProxyControl
        buttonClassName="relative text-base w-auto h-auto py-xs px-sm gap-xs inline-flex"
        buttonActiveClassName="text-success"
        statusDotClassName="absolute top-[2px] right-[2px] w-[6px] h-[6px] rounded-full bg-transparent"
        statusDotActiveClassName="bg-success animate-pulse"
      />
    </>
  );

  return (
    <div className="flex flex-col h-full min-h-0 overflow-y-auto overflow-x-hidden border-r border-border relative flex-1 max-md:h-auto max-md:max-h-none max-md:border-r-0 max-md:border-b max-md:border-border bg-surface">
      <div className="p-base border-b border-border bg-background shrink-0">
        <SidebarTabs
          tabs={SIDEBAR_TABS}
          activeTab={activeTab}
          onTabChange={handleTabChange}
          rightContent={headerActions}
        />

        {/* Search and filters - only show on models tab */}
        {activeTab === 'models' && (
          <div className="flex items-center gap-sm mt-md">
            <div className="flex-1">
              <Input
                type="text"
                placeholder="Search models..."
                value={searchQuery}
                onChange={(e) => onSearchChange(e.target.value)}
                className="w-full"
                size="sm"
              />
            </div>

            <div className="relative">
              <Button
                variant="ghost"
                size="sm"
                className={cn("relative", hasActiveFilters && "text-primary")}
                onClick={() => setFilterPopoverOpen(!filterPopoverOpen)}
                title="Filter models"
                iconOnly
              >
                🔽
                {hasActiveFilters && <span className="absolute top-[2px] right-[2px] w-[8px] h-[8px] bg-primary rounded-full border-2 border-surface" />}
              </Button>
              
              <FilterPopover
                isOpen={filterPopoverOpen}
                onClose={() => setFilterPopoverOpen(false)}
                filterOptions={filterOptions}
                tags={tags}
                filters={filters}
                onFiltersChange={onFiltersChange}
                onClearFilters={onClearFilters}
              />
            </div>
          </div>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col">
        {activeTab === 'models' ? (
          <ModelsListContent
            models={models}
            selectedModelId={selectedModelId}
            onSelectModel={onSelectModel}
            loading={loading}
            servers={servers}
            onSwitchToAddTab={handleSwitchToAddTab}
          />
        ) : (
          <AddDownloadContent
            onModelAdded={onModelAdded}
            activeSubTab={activeSubTab}
            onSubTabChange={onSubTabChange}
            downloadSystemError={downloadSystemError}
            onSelectHfModel={onSelectHfModel}
            selectedHfModelId={selectedHfModelId}
          />
        )}
      </div>
    </div>
  );
};

export default ModelLibraryPanel;
