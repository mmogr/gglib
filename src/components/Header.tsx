import { FC, useState, useRef } from "react";
import { Library, Menu, Monitor, Settings, X } from "lucide-react";
import { ServerInfo } from "../types";
import { RunsPopover } from "./RunsPopover";
import { useClickOutside } from "../hooks/useClickOutside";
import { cn } from "../utils/cn";

interface HeaderProps {
  onOpenSettings: () => void;
  servers: ServerInfo[];
  onStopServer: (modelId: number) => Promise<void>;
  onSelectModel: (modelId: number, view?: 'chat' | 'console') => void;
  onRefreshServers?: () => void;
}

const Header: FC<HeaderProps> = ({
  onOpenSettings,
  servers,
  onStopServer,
  onSelectModel,
  onRefreshServers,
}) => {
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);
  const [isRunsPopoverOpen, setIsRunsPopoverOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const runsButtonRef = useRef<HTMLButtonElement>(null);

  const serverCount = servers.length;
  const hasRunningServers = serverCount > 0;

  // Close menu when clicking outside
  useClickOutside(menuRef, () => setIsMobileMenuOpen(false), isMobileMenuOpen);

  const handleMobileMenuAction = (action: () => void) => {
    action();
    setIsMobileMenuOpen(false);
  };

  const handleRunsClick = () => {
    if (hasRunningServers) {
      setIsRunsPopoverOpen(!isRunsPopoverOpen);
    }
  };

  return (
    <header className="bg-[linear-gradient(135deg,var(--color-background-elevated)_0%,var(--color-surface-elevated)_100%)] text-text py-sm px-xl border-b border-border shadow-md shrink-0">
      <div className="flex justify-between items-center w-full">
        <div className="flex flex-row items-center gap-sm">
          <h1 className="flex items-center gap-sm text-xl font-bold m-0">
            <Library className="w-5 h-5" aria-hidden />
            <span>GGLib</span>
          </h1>
        </div>
        <div className="flex items-center gap-base tablet:relative" ref={menuRef}>
          {/* Desktop navigation */}
          <div className="flex items-center gap-base tablet:hidden">
            {/* Server status button with popover */}
            <div className="relative">
              <button
                ref={runsButtonRef}
                type="button"
                className={cn(
                  'flex items-center justify-center gap-sm px-[calc(var(--spacing-lg)+var(--spacing-xs))] h-[var(--button-height-base)] rounded-full border border-border bg-background-elevated text-inherit font-medium text-sm leading-none cursor-pointer transition-all',
                  'hover:not-disabled:border-border-hover hover:not-disabled:bg-background-hover',
                  'disabled:opacity-50 disabled:cursor-not-allowed',
                  'w-[var(--button-height-base)] p-0 relative',
                  hasRunningServers && 'border-primary text-primary-light shadow-[0_0_12px_rgba(59,130,246,0.25)]',
                )}
                onClick={handleRunsClick}
                disabled={!hasRunningServers}
                aria-label={hasRunningServers ? `${serverCount} server${serverCount !== 1 ? 's' : ''} running` : 'No servers running'}
                title={hasRunningServers ? `${serverCount} server${serverCount !== 1 ? 's' : ''} running` : 'No servers running'}
              >
                <Monitor className="w-[18px] h-[18px]" aria-hidden />
                {hasRunningServers && (
                  <span className="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 rounded-[9px] bg-[#10b981] text-white text-[11px] font-semibold flex items-center justify-center shadow-[0_0_6px_rgba(16,185,129,0.6)]">{serverCount}</span>
                )}
              </button>
              <RunsPopover
                isOpen={isRunsPopoverOpen}
                onClose={() => setIsRunsPopoverOpen(false)}
                servers={servers}
                onStopServer={onStopServer}
                onSelectModel={onSelectModel}
                onRefresh={onRefreshServers}
              />
            </div>
            <button
              type="button"
              className={cn(
                'flex items-center justify-center gap-sm px-[calc(var(--spacing-lg)+var(--spacing-xs))] h-[var(--button-height-base)] rounded-full border border-border bg-background-elevated text-inherit font-medium text-sm leading-none cursor-pointer transition-all',
                'hover:not-disabled:border-border-hover hover:not-disabled:bg-background-hover',
                'w-[var(--button-height-base)] p-0 relative',
              )}
              onClick={onOpenSettings}
              aria-label="Open settings"
              title="Open settings"
            >
                <Settings className="w-[18px] h-[18px]" aria-hidden />
            </button>
          </div>

          {/* Mobile menu toggle */}
          <button
            type="button"
            className="hidden tablet:flex items-center justify-center w-[var(--button-height-base)] h-[var(--button-height-base)] p-0 rounded-full border border-border bg-background-elevated text-inherit text-lg cursor-pointer transition-all hover:border-border-hover hover:bg-background-hover"
            onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
            aria-label={isMobileMenuOpen ? 'Close menu' : 'Open menu'}
            aria-expanded={isMobileMenuOpen}
          >
            {isMobileMenuOpen ? (
              <X className="w-[18px] h-[18px]" aria-hidden />
            ) : (
              <Menu className="w-[18px] h-[18px]" aria-hidden />
            )}
          </button>

          {/* Mobile dropdown menu */}
          <div className={cn(
            'hidden absolute top-full right-base min-w-[180px] p-sm bg-surface border border-border rounded-base shadow-lg z-[100]',
            isMobileMenuOpen && 'flex flex-col gap-xs',
          )}>
            <button
              type="button"
              className={cn(
                'flex items-center gap-sm w-full px-base py-sm border-none rounded-sm bg-transparent text-inherit text-sm font-medium text-left cursor-pointer transition-all hover:bg-background-hover',
                hasRunningServers && 'bg-primary text-white',
              )}
              onClick={() => hasRunningServers && handleMobileMenuAction(() => setIsRunsPopoverOpen(true))}
              disabled={!hasRunningServers}
            >
              <Monitor className="w-[18px] h-[18px]" aria-hidden />
              {hasRunningServers ? `${serverCount} Running` : 'No Servers'}
            </button>
            <button
              type="button"
              className="flex items-center gap-sm w-full px-base py-sm border-none rounded-sm bg-transparent text-inherit text-sm font-medium text-left cursor-pointer transition-all hover:bg-background-hover"
              onClick={() => handleMobileMenuAction(onOpenSettings)}
            >
              <Settings className="w-[18px] h-[18px]" aria-hidden />
              Settings
            </button>
          </div>
        </div>
      </div>
    </header>
  );
};

export default Header;