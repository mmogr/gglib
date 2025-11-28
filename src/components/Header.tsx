import { FC, useState, useRef } from "react";
import ProxyControl from "./ProxyControl";
import { useClickOutside } from "../hooks/useClickOutside";
import styles from './Header.module.css';

interface HeaderProps {
  onOpenChat: () => void;
  onOpenSettings: () => void;
  onToggleWorkPanel: () => void;
  isWorkPanelVisible: boolean;
  isModelRunning: boolean;
}

const Header: FC<HeaderProps> = ({
  onOpenChat,
  onOpenSettings,
  onToggleWorkPanel,
  isWorkPanelVisible,
  isModelRunning,
}) => {
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const workPanelLabel = isWorkPanelVisible ? 'Hide work panel' : 'Show work panel';

  // Close menu when clicking outside
  useClickOutside(menuRef, () => setIsMobileMenuOpen(false), isMobileMenuOpen);

  const handleMobileMenuAction = (action: () => void) => {
    action();
    setIsMobileMenuOpen(false);
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
            <button
              type="button"
              className={styles.headerButton}
              onClick={onOpenChat}
              title="Open chat"
            >
              💬 Chat
            </button>
            <ProxyControl
              buttonClassName={styles.headerButton}
              buttonActiveClassName={styles.headerButtonActive}
              statusDotClassName={styles.statusDot}
              statusDotActiveClassName={styles.statusDotActive}
            />
            <button
              type="button"
              className={styles.headerButton}
              onClick={onOpenSettings}
              aria-label="Open settings"
              title="Open settings"
            >
              ⚙️ Settings
            </button>
            <button
              type="button"
              className={`${styles.headerButton} ${isWorkPanelVisible ? styles.headerButtonActive : ''}`}
              onClick={onToggleWorkPanel}
              aria-pressed={isWorkPanelVisible}
              aria-label={`${workPanelLabel}. ${isModelRunning ? 'A model is running.' : 'No models running.'}`}
              title={workPanelLabel}
            >
              <span>Work Panel</span>
              <span
                className={`${styles.statusDot} ${isModelRunning ? styles.statusDotActive : ''}`}
                aria-hidden="true"
              />
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
              className={styles.mobileMenuItem}
              onClick={() => handleMobileMenuAction(onOpenChat)}
            >
              💬 Chat
            </button>
            <div className={styles.mobileMenuProxyWrapper}>
              <ProxyControl
                buttonClassName={styles.mobileMenuItem}
                buttonActiveClassName={styles.mobileMenuItemActive}
                statusDotClassName={styles.statusDot}
                statusDotActiveClassName={styles.statusDotActive}
              />
            </div>
            <button
              type="button"
              className={styles.mobileMenuItem}
              onClick={() => handleMobileMenuAction(onOpenSettings)}
            >
              ⚙️ Settings
            </button>
            <button
              type="button"
              className={`${styles.mobileMenuItem} ${isWorkPanelVisible ? styles.mobileMenuItemActive : ''}`}
              onClick={() => handleMobileMenuAction(onToggleWorkPanel)}
            >
              📋 {workPanelLabel}
            </button>
          </div>
        </div>
      </div>
    </header>
  );
};

export default Header;