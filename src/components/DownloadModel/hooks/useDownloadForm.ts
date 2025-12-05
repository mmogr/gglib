import { useState, useCallback, FormEvent } from "react";
import type { DownloadQueueStatus, QueueDownloadResponse } from "../../../download";

/**
 * Dependencies for useDownloadForm hook.
 * Uses dependency injection to keep the hook UI-agnostic.
 */
export interface UseDownloadFormDeps {
  queueDownload: (modelId: string, quantization?: string) => Promise<QueueDownloadResponse>;
  queueStatus: DownloadQueueStatus | null;
  refreshQueue: () => Promise<void>;
  setError: (msg: string | null) => void;
}

/**
 * Form state and handlers for download form.
 */
export interface DownloadFormState {
  repoId: string;
  setRepoId: (value: string) => void;
  quantization: string;
  setQuantization: (value: string) => void;
  submitting: boolean;
  canSubmit: boolean;
  handleSubmit: (e: FormEvent) => Promise<void>;
}

/**
 * Hook that manages download form state and submission logic.
 * 
 * Responsibilities:
 * - Owns form field state (repoId, quantization, submitting)
 * - Validates input before submission
 * - Checks queue capacity before adding
 * - Clears form on successful queue
 */
export function useDownloadForm({
  queueDownload,
  queueStatus,
  refreshQueue,
  setError,
}: UseDownloadFormDeps): DownloadFormState {
  const [repoId, setRepoId] = useState("");
  const [quantization, setQuantization] = useState("");
  const [submitting, setSubmitting] = useState(false);

  // Calculate if form can be submitted
  const queueCount = queueStatus
    ? (queueStatus.current ? 1 : 0) + queueStatus.pending.length
    : 0;
  const isQueueFull = queueStatus !== null && queueCount >= queueStatus.max_size;
  const canSubmit = !submitting && repoId.trim().length > 0 && !isQueueFull;

  const handleSubmit = useCallback(async (e: FormEvent) => {
    e.preventDefault();

    const trimmedRepoId = repoId.trim();
    if (!trimmedRepoId) {
      setError("Please provide a repository ID");
      return;
    }

    // Check if queue is full
    if (queueStatus) {
      const currentCount = (queueStatus.current ? 1 : 0) + queueStatus.pending.length;
      if (currentCount >= queueStatus.max_size) {
        setError(`Queue is full (max ${queueStatus.max_size}). Please wait for a download to complete.`);
        return;
      }
    }

    try {
      setSubmitting(true);
      setError(null);

      await queueDownload(trimmedRepoId, quantization || undefined);

      // Clear form after successful queue
      setRepoId("");
      setQuantization("");

      // Refresh queue status
      await refreshQueue();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to queue download");
    } finally {
      setSubmitting(false);
    }
  }, [repoId, quantization, queueStatus, queueDownload, refreshQueue, setError]);

  return {
    repoId,
    setRepoId,
    quantization,
    setQuantization,
    submitting,
    canSubmit,
    handleSubmit,
  };
}

/** Type for the return value of useDownloadForm */
export type DownloadFormController = ReturnType<typeof useDownloadForm>;
