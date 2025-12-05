import { FC } from 'react';
import styles from './DownloadProgressBar.module.css';

interface DownloadProgressBarProps {
  percentage?: number;
  indeterminate?: boolean;
}

const DownloadProgressBar: FC<DownloadProgressBarProps> = ({ percentage, indeterminate }) => {
  return (
    <div className={styles.progressBar}>
      <div
        className={`${styles.fill} ${indeterminate ? styles.indeterminate : ''}`}
        style={!indeterminate && percentage !== undefined ? { width: `${percentage}%` } : {}}
      />
    </div>
  );
};

export default DownloadProgressBar;
