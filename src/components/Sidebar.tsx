import { FC } from "react";
import styles from './Sidebar.module.css';

type ViewType = 'models' | 'add-model' | 'download';

interface SidebarProps {
  currentView: ViewType;
  onViewChange: (view: ViewType) => void;
}

const Sidebar: FC<SidebarProps> = ({ currentView, onViewChange }) => {
  const modelMenuItems = [
    { id: "models", label: "📋 Models", icon: "📋" },
    { id: "add-model", label: "➕ Add Model", icon: "➕" },
    { id: "download", label: "⬇️ Download", icon: "⬇️" },
  ];

  return (
    <nav className="sidebar">
      <div className={styles.navSection}>
        <div className={styles.sectionTitle}>Models</div>
        <ul className="nav-menu">
          {modelMenuItems.map((item) => (
            <li key={item.id}>
              <button
                className={`nav-button ${currentView === item.id ? "active" : ""}`}
                onClick={() => onViewChange(item.id as ViewType)}
              >
                <span className="nav-icon">{item.icon}</span>
                <span className="nav-label">{item.label.replace(/^.+\s/, "")}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </nav>
  );
};

export default Sidebar;
