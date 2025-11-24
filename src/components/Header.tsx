import { FC } from "react";
import ProxyControl from "./ProxyControl";
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
  const workPanelLabel = isWorkPanelVisible ? 'Hide work panel' : 'Show work panel';

  return (
    <header className="header">
      <div className={styles.headerContent}>
        <div className={styles.headerLeft}>
          <h1 className="app-title">
            <span className="logo">🦀</span>
            GGLib
          </h1>
        </div>
        <div className={styles.headerRight}>
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
      </div>
    </header>
  );
};

export default Header;