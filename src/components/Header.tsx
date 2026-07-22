import { FC, useState, useRef } from "react";
import { Library, Menu, Monitor, Settings, X } from "lucide-react";
import { ServerInfo } from "../types";
import { RunsPopover } from "./RunsPopover";
import { useClickOutside } from "../hooks/useClickOutside";
import { Button } from "./ui/Button";
import { Icon } from "./ui/Icon";
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
  const runsLabel = hasRunningServers
    ? `${serverCount} server${serverCount !== 1 ? 's' : ''} running`
    : 'No servers running';

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
    <header className="bg-background-elevated text-text py-sm px-xl border-b border-border shrink-0">
      <div className="flex justify-between items-center w-full">
        <div className="flex flex-row items-center gap-sm">
          <h1 className="flex items-center gap-sm text-xl font-bold m-0">
            <Library className="w-5 h-5" aria-hidden />
            <span>GGLib</span>
          </h1>
        </div>
        <div className="relative flex items-center gap-base" ref={menuRef}>
          {/* Desktop navigation */}
          <div className="hidden md:flex items-center gap-base">
            {/* Server status button with popover */}
            <div className="relative">
              <Button
                ref={runsButtonRef}
                variant="secondary"
                iconOnly
                className={cn(
                  'rounded-full relative',
                  hasRunningServers && 'border-primary text-primary-light',
                )}
                onClick={handleRunsClick}
                disabled={!hasRunningServers}
                aria-label={runsLabel}
                title={runsLabel}
              >
                <Icon icon={Monitor} size={18} />
                {hasRunningServers && (
                  <span className="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 rounded-full bg-success text-text-inverse text-[11px] font-semibold flex items-center justify-center">
                    {serverCount}
                  </span>
                )}
              </Button>
              <RunsPopover
                isOpen={isRunsPopoverOpen}
                onClose={() => setIsRunsPopoverOpen(false)}
                servers={servers}
                onStopServer={onStopServer}
                onSelectModel={onSelectModel}
                onRefresh={onRefreshServers}
              />
            </div>
            <Button
              variant="secondary"
              iconOnly
              className="rounded-full"
              onClick={onOpenSettings}
              aria-label="Open settings"
              title="Open settings"
            >
              <Icon icon={Settings} size={18} />
            </Button>
          </div>

          {/* Mobile menu toggle */}
          <Button
            variant="secondary"
            iconOnly
            className="flex md:hidden rounded-full"
            onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
            aria-label={isMobileMenuOpen ? 'Close menu' : 'Open menu'}
            aria-expanded={isMobileMenuOpen}
          >
            <Icon icon={isMobileMenuOpen ? X : Menu} size={18} />
          </Button>

          {/* Mobile dropdown menu */}
          <div className={cn(
            'hidden absolute top-full right-base min-w-[180px] p-sm bg-surface border border-border rounded-base shadow-lg z-[100]',
            isMobileMenuOpen && 'flex flex-col gap-xs',
          )}>
            <Button
              variant="ghost"
              fullWidth
              className="justify-start"
              onClick={() => hasRunningServers && handleMobileMenuAction(() => setIsRunsPopoverOpen(true))}
              disabled={!hasRunningServers}
              leftIcon={<Icon icon={Monitor} size={18} />}
            >
              {hasRunningServers ? `${serverCount} Running` : 'No Servers'}
            </Button>
            <Button
              variant="ghost"
              fullWidth
              className="justify-start"
              onClick={() => handleMobileMenuAction(onOpenSettings)}
              leftIcon={<Icon icon={Settings} size={18} />}
            >
              Settings
            </Button>
          </div>
        </div>
      </div>
    </header>
  );
};

export default Header;