import { FC, useState, useRef } from "react";
import { ServerInfo } from "../types";
import { RunsPopover } from "./RunsPopover";
import { useClickOutside } from "../hooks/useClickOutside";
import styles from './Header.module.css';

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
    <header className="header">
      <div className={styles.headerContent}>
        <div className={styles.headerLeft}>
          <h1 className="app-title">
            <span className="logo">🦀</span>
            GGLib
          </h1>
        </div>
        <div className={styles.headerRight} ref={menuRef}>
          {/* Desktop navigation */}
          <div className={styles.desktopNav}>
            {/* Server status button with popover */}
            <div className={styles.serverButtonWrapper}>
              <button
                ref={runsButtonRef}
                type="button"
                className={`${styles.headerButton} ${styles.headerButtonIconOnly} ${hasRunningServers ? styles.headerButtonActive : ''}`}
                onClick={handleRunsClick}
                disabled={!hasRunningServers}
                aria-label={hasRunningServers ? `${serverCount} server${serverCount !== 1 ? 's' : ''} running` : 'No servers running'}
                title={hasRunningServers ? `${serverCount} server${serverCount !== 1 ? 's' : ''} running` : 'No servers running'}
              >
                🖥️
                {hasRunningServers && (
                  <span className={styles.serverBadge}>{serverCount}</span>
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
              className={`${styles.headerButton} ${styles.headerButtonIconOnly}`}
              onClick={onOpenSettings}
              aria-label="Open settings"
              title="Open settings"
            >
              ⚙️
            </button>
          </div>

          {/* Mobile menu toggle */}
          <button
            type="button"
            className={styles.menuToggle}
            onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
            aria-label={isMobileMenuOpen ? 'Close menu' : 'Open menu'}
            aria-expanded={isMobileMenuOpen}
          >
            {isMobileMenuOpen ? '✕' : '☰'}
          </button>

          {/* Mobile dropdown menu */}
          <div className={`${styles.mobileMenu} ${isMobileMenuOpen ? styles.open : ''}`}>
            <button
              type="button"
              className={`${styles.mobileMenuItem} ${hasRunningServers ? styles.mobileMenuItemActive : ''}`}
              onClick={() => hasRunningServers && handleMobileMenuAction(() => setIsRunsPopoverOpen(true))}
              disabled={!hasRunningServers}
            >
              🖥️ {hasRunningServers ? `${serverCount} Running` : 'No Servers'}
            </button>
            <button
              type="button"
              className={styles.mobileMenuItem}
              onClick={() => handleMobileMenuAction(onOpenSettings)}
            >
              ⚙️ Settings
            </button>
          </div>
        </div>
      </div>
    </header>
  );
};

export default Header;