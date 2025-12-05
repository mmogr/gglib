import { FC } from 'react';
import { formatBytes } from '../../utils/format';
import styles from './ShardProgressIndicator.module.css';

interface ShardProgressIndicatorProps {
  shardLabel: string;
  filename?: string;
  downloaded?: number;
  total?: number;
}

const ShardProgressIndicator: FC<ShardProgressIndicatorProps> = ({ shardLabel, filename, downloaded, total }) => {
  return (
    <div className={styles.container}>
      <div className={styles.header}>
        <span className={styles.label}>{shardLabel}</span>
        {filename && <span className={styles.filename} title={filename}>{filename}</span>}
      </div>
      {(downloaded !== undefined && total !== undefined) && (
        <div className={styles.metrics}>
          <span>{formatBytes(downloaded)} / {formatBytes(total)}</span>
        </div>
      )}
    </div>
  );
};

export default ShardProgressIndicator;
