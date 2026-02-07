/**
 * Shared formatting utilities for displaying bytes, time, and numbers.
 */

/**
 * Format bytes into human-readable format (B, KB, MB, GB, etc.)
 */
export const formatBytes = (bytes: number, decimals = 2): string => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB', 'ZB', 'YB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

/**
 * Format seconds into human-readable time (e.g., "5m 30s")
 */
export const formatTime = (seconds: number): string => {
  if (!isFinite(seconds) || seconds < 0) return 'Calculating...';
  if (seconds < 60) return `${Math.ceil(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.ceil(seconds % 60);
  return `${minutes}m ${remainingSeconds}s`;
};

/**
 * Format large numbers with K/M suffix (e.g., 1.5K, 2.3M)
 */
export const formatNumber = (num: number): string => {
  if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
  if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
  return num.toString();
};

/**
 * Format model parameter count (e.g., "7.0B", "500M")
 * For MoE models, show total with active count if available.
 * @param paramCount - Parameter count in billions (total for MoE)
 * @param expertUsedCount - Number of experts used (for MoE models)
 * @param expertCount - Total number of experts (for MoE models)
 */
export const formatParamCount = (
  paramCount: number,
  expertUsedCount?: number,
  expertCount?: number
): string => {
  const baseFormat = paramCount >= 1 
    ? `${paramCount.toFixed(1)}B` 
    : `${(paramCount * 1000).toFixed(0)}M`;

  // For MoE models, calculate and show active parameters
  if (expertCount && expertCount > 1 && expertUsedCount && expertUsedCount > 0) {
    const activeParams = (expertUsedCount / expertCount) * paramCount;
    const activeFormat = activeParams >= 1
      ? `${activeParams.toFixed(1)}B`
      : `${(activeParams * 1000).toFixed(0)}M`;
    return `${baseFormat} (Active: ${activeFormat})`;
  }

  return baseFormat;
};

/**
 * Compute the HuggingFace URL for a local model.
 * Returns the URL if hf_repo_id exists, otherwise null.
 * 
 * @param hfRepoId - HuggingFace repository ID (e.g., "TheBloke/Llama-2-7B-GGUF")
 * @param hfFilename - Optional filename for direct file link
 * @returns The HuggingFace URL or null if no repo ID
 */
export const getHuggingFaceUrl = (
  hfRepoId: string | null | undefined,
  hfFilename?: string | null
): string | null => {
  if (!hfRepoId) return null;
  
  if (hfFilename) {
    return `https://huggingface.co/${hfRepoId}/blob/main/${hfFilename}`;
  }
  return `https://huggingface.co/${hfRepoId}`;
};

/**
 * Get the HuggingFace URL for an HfModelSummary (browse mode).
 * @param modelId - The model ID from HuggingFace (e.g., "TheBloke/Llama-2-7B-GGUF")
 */
export const getHuggingFaceModelUrl = (modelId: string): string => {
  return `https://huggingface.co/${modelId}`;
};