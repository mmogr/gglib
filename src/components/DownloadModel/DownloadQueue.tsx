import { FC } from "react";
import type { DownloadQueueStatus, DownloadQueueItem } from "../../download";
import type { QueueActionsController } from "./hooks";
import styles from "./DownloadModel.module.css";

interface DownloadQueueProps {
  queueStatus: DownloadQueueStatus;
  queueActions: QueueActionsController;
}

/**
 * Displays the download queue with pending and failed sections.
 */
const DownloadQueue: FC<DownloadQueueProps> = ({
  queueStatus,
  queueActions,
}) => {
  const { 
    handleRemoveFromQueue, 
    handleCancelShardGroup, 
    handleClearFailed, 
    handleRetry 
  } = queueActions;

  const hasFailedDownloads = queueStatus.failed.length > 0;
  const hasPending = queueStatus.pending.length > 0;

  if (!hasPending && !hasFailedDownloads) {
    return null;
  }

  return (
    <div className={styles.queueSection}>
      {/* Pending Queue */}
      {hasPending && (
        <PendingQueue
          items={queueStatus.pending}
          onRemove={handleRemoveFromQueue}
          onCancelShardGroup={handleCancelShardGroup}
        />
      )}

      {/* Failed Downloads */}
      {hasFailedDownloads && (
        <FailedDownloads
          items={queueStatus.failed}
          onRetry={handleRetry}
          onRemove={handleRemoveFromQueue}
          onClearAll={handleClearFailed}
        />
      )}
    </div>
  );
};

// ─────────────────────────────────────────────────────────────────────────────
// Sub-components
// ─────────────────────────────────────────────────────────────────────────────

interface PendingQueueProps {
  items: DownloadQueueItem[];
  onRemove: (modelId: string) => Promise<void>;
  onCancelShardGroup: (groupId: string) => Promise<void>;
}

const PendingQueue: FC<PendingQueueProps> = ({ items, onRemove, onCancelShardGroup }) => (
  <>
    <h3 className={styles.queueTitle}>
      Queued Downloads ({items.length})
    </h3>
    <ul className={styles.queueList}>
      {items.map((item, index) => (
        <PendingQueueItem
          key={`${item.id}-${index}`}
          item={item}
          onRemove={onRemove}
          onCancelShardGroup={onCancelShardGroup}
        />
      ))}
    </ul>
  </>
);

interface PendingQueueItemProps {
  item: DownloadQueueItem;
  onRemove: (modelId: string) => Promise<void>;
  onCancelShardGroup: (groupId: string) => Promise<void>;
}

const PendingQueueItem: FC<PendingQueueItemProps> = ({ item, onRemove, onCancelShardGroup }) => {
  const isShard = item.shard_info !== null && item.shard_info !== undefined;
  const shardLabel = isShard
    ? `shard ${item.shard_info!.shard_index + 1}/${item.shard_info!.total_shards}`
    : null;

  return (
    <li className={styles.queueItem}>
      <div className={styles.queueItemInfo}>
        <span className={styles.queuePosition}>#{item.position}</span>
        <span className={styles.queueModelId}>
          {item.display_name}
          {shardLabel && (
            <span className={styles.shardBadge}>{shardLabel}</span>
          )}
        </span>
      </div>
      <button
        type="button"
        className={`btn btn-sm ${styles.removeBtn}`}
        onClick={() => item.group_id 
          ? onCancelShardGroup(item.group_id)
          : onRemove(item.id)
        }
        aria-label={item.group_id 
          ? `Cancel all shards for ${item.display_name}`
          : `Remove ${item.display_name} from queue`
        }
        title={item.group_id ? "Cancel all shards" : "Remove from queue"}
      >
        {item.group_id ? "Cancel All" : "✕"}
      </button>
    </li>
  );
};

interface FailedDownloadsProps {
  items: DownloadQueueItem[];
  onRetry: (item: DownloadQueueItem) => Promise<void>;
  onRemove: (modelId: string) => Promise<void>;
  onClearAll: () => Promise<void>;
}

const FailedDownloads: FC<FailedDownloadsProps> = ({ items, onRetry, onRemove, onClearAll }) => (
  <>
    <div className={styles.failedHeader}>
      <h3 className={styles.queueTitle}>
        Failed Downloads ({items.length})
      </h3>
      <button
        type="button"
        className={`btn btn-sm ${styles.clearFailedBtn}`}
        onClick={onClearAll}
      >
        Clear All
      </button>
    </div>
    <ul className={styles.queueList}>
      {items.map((item, index) => (
        <FailedQueueItem
          key={`failed-${item.id}-${index}`}
          item={item}
          onRetry={onRetry}
          onRemove={onRemove}
        />
      ))}
    </ul>
  </>
);

interface FailedQueueItemProps {
  item: DownloadQueueItem;
  onRetry: (item: DownloadQueueItem) => Promise<void>;
  onRemove: (modelId: string) => Promise<void>;
}

const FailedQueueItem: FC<FailedQueueItemProps> = ({ item, onRetry, onRemove }) => {
  const isShard = item.shard_info !== null && item.shard_info !== undefined;
  const shardLabel = isShard
    ? `shard ${item.shard_info!.shard_index + 1}/${item.shard_info!.total_shards}`
    : null;

  return (
    <li className={`${styles.queueItem} ${styles.queueItemFailed}`}>
      <div className={styles.queueItemInfo}>
        <span className={styles.failedIcon}>❌</span>
        <span className={styles.queueModelId}>
          {item.display_name}
          {shardLabel && (
            <span className={styles.shardBadge}>{shardLabel}</span>
          )}
        </span>
        {item.error && (
          <span className={styles.errorText} title={item.error}>
            {item.error.length > 40 ? item.error.substring(0, 40) + '...' : item.error}
          </span>
        )}
      </div>
      <div className={styles.failedActions}>
        <button
          type="button"
          className={`btn btn-sm ${styles.retryBtn}`}
          onClick={() => onRetry(item)}
          aria-label={`Retry ${item.display_name}`}
        >
          Retry
        </button>
        <button
          type="button"
          className={`btn btn-sm ${styles.removeBtn}`}
          onClick={() => onRemove(item.id)}
          aria-label={`Remove ${item.display_name} from failed list`}
        >
          ✕
        </button>
      </div>
    </li>
  );
};

export default DownloadQueue;
