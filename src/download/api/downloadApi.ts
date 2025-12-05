import { apiFetch, isTauriApp, tauriInvoke } from '../../services/tauri';
import type { DownloadEvent, DownloadQueueStatus, DownloadSummary } from './types';

export interface QueueDownloadResponse {
  position: number;
  shard_count: number;
}

export interface ReorderResponse {
  actual_position: number;
}

export async function queueDownload(modelId: string, quantization?: string): Promise<QueueDownloadResponse> {
  if (isTauriApp) {
    return tauriInvoke<QueueDownloadResponse>('queue_download', { modelId, quantization });
  }

  const response = await apiFetch(`/models/download/queue`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model_id: modelId, quantization }),
  });

  if (!response.ok) {
    throw new Error('Failed to queue download');
  }

  const data = await response.json();
  return (data?.data as QueueDownloadResponse) || { position: 1, shard_count: 1 };
}

export async function cancelDownload(id: string): Promise<void> {
  if (isTauriApp) {
    await tauriInvoke('cancel_download', { modelId: id });
    return;
  }

  const response = await apiFetch(`/models/download/cancel`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model_id: id }),
  });

  if (!response.ok) {
    throw new Error('Failed to cancel download');
  }
}

export async function removeFromDownloadQueue(modelId: string): Promise<void> {
  if (isTauriApp) {
    await tauriInvoke('remove_from_download_queue', { modelId });
    return;
  }

  const response = await apiFetch(`/models/download/queue/remove`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model_id: modelId }),
  });

  if (!response.ok) {
    throw new Error('Failed to remove from queue');
  }
}

export async function getQueueSnapshot(): Promise<DownloadQueueStatus> {
  if (isTauriApp) {
    return tauriInvoke<DownloadQueueStatus>('get_download_queue');
  }

  const response = await apiFetch(`/models/download/queue`);
  if (!response.ok) {
    throw new Error(`Failed to fetch download queue: ${response.statusText}`);
  }
  const data = await response.json();
  return (data?.data as DownloadQueueStatus) || { pending: [], failed: [], max_size: 10 };
}

export async function clearFailedDownloads(): Promise<void> {
  if (isTauriApp) {
    await tauriInvoke('clear_failed_downloads');
    return;
  }

  const response = await apiFetch(`/models/download/queue/clear-failed`, { method: 'POST' });
  if (!response.ok) {
    throw new Error('Failed to clear failed downloads');
  }
}

export async function cancelShardGroup(groupId: string): Promise<void> {
  if (isTauriApp) {
    await tauriInvoke('cancel_shard_group', { groupId });
    return;
  }

  const response = await apiFetch(`/models/download/queue/cancel-group`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ group_id: groupId }),
  });

  if (!response.ok) {
    throw new Error('Failed to cancel shard group');
  }
}

export async function reorderDownloadQueue(modelId: string, newPosition: number): Promise<number> {
  if (isTauriApp) {
    return tauriInvoke<number>('reorder_download_queue', { modelId, newPosition });
  }

  const response = await apiFetch(`/models/download/queue/reorder`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model_id: modelId, new_position: newPosition }),
  });

  if (!response.ok) {
    throw new Error('Failed to reorder queue');
  }

  const data = await response.json();
  return (data?.data as ReorderResponse)?.actual_position ?? newPosition;
}

export type DownloadEventListener = (event: DownloadEvent) => void;

export async function subscribeToDownloadEvents(onEvent: DownloadEventListener): Promise<() => void> {
  if (isTauriApp) {
    const { listen } = await import('@tauri-apps/api/event');

    const unlistenProgress = await listen<DownloadEvent>('download-progress', (event) => {
      if (event.payload) onEvent(event.payload);
    });

    const unlistenQueue = await listen<{ items: DownloadSummary[]; max_size: number }>('download-queue-snapshot', (event) => {
      onEvent({ type: 'queue_snapshot', items: event.payload.items, max_size: event.payload.max_size });
    });

    return () => {
      unlistenProgress();
      unlistenQueue();
    };
  }

  const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
  const eventSource = new EventSource(`${baseUrl}/api/models/download/progress`);

  eventSource.onmessage = (evt) => {
    if (!evt.data || evt.data.trim() === '') return;
    try {
      const parsed = JSON.parse(evt.data) as DownloadEvent;
      onEvent(parsed);
    } catch (e) {
      console.error('[downloadApi] Failed to parse download event', e);
    }
  };

  eventSource.onerror = (err) => {
    console.error('[downloadApi] SSE error', err);
  };

  return () => {
    eventSource.close();
  };
}
