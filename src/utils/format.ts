/**
 * Shared formatting utilities for displaying bytes, time, and numbers.
 */

/**
 * Placeholder for a value that is not known yet.
 *
 * Deliberately not '0': zero is a real reading meaning "stalled", and
 * conflating the two is what rendered "ETA: 0s" on a healthy download.
 * Mirrors `UNKNOWN` in `crates/gglib-core/src/download/format.rs`.
 */
export const UNKNOWN = '—';

/**
 * Format a byte *size* in binary units (KiB, MiB, GiB).
 *
 * Sizes stay binary because that is the convention for model files on disk.
 * For transfer *rates* use {@link formatRate}, which is decimal.
 */
export const formatBytes = (bytes: number, decimals = 2): string => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB', 'EiB', 'ZiB', 'YiB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

/**
 * Format a transfer rate in decimal units, e.g. "118.4 MB/s".
 *
 * Decimal (1 MB/s = 1,000,000 B/s) so the number agrees with Activity Monitor,
 * nettop and every ISP — which is what a download speed gets compared against.
 * Must stay byte-for-byte identical to `format_rate` in
 * `crates/gglib-core/src/download/format.rs` so the CLI and the GUI never
 * disagree about the same transfer.
 */
export const formatRate = (bps: number | undefined | null): string => {
  if (bps == null || !isFinite(bps) || bps < 0) return UNKNOWN;

  const KB = 1_000;
  const MB = 1_000_000;
  const GB = 1_000_000_000;

  if (bps >= GB) return `${(bps / GB).toFixed(2)} GB/s`;
  if (bps >= MB) return `${(bps / MB).toFixed(1)} MB/s`;
  if (bps >= KB) return `${(bps / KB).toFixed(0)} kB/s`;
  return `${bps.toFixed(0)} B/s`;
};

/**
 * Format a duration in seconds as "45s", "3m 20s" or "1h 04m".
 *
 * Mirrors `format_duration` in `crates/gglib-core/src/download/format.rs`.
 * Returns {@link UNKNOWN} when the value is absent — an unknown ETA is not a
 * zero ETA.
 */
export const formatDuration = (seconds: number | undefined | null): string => {
  if (seconds == null || !isFinite(seconds) || seconds < 0) return UNKNOWN;

  // Saturate rather than overflow the display on absurd inputs (a near-zero
  // rate can produce an ETA of centuries before the average settles).
  const total = Math.min(Math.ceil(seconds), 359_999);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;

  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, '0')}m`;
  if (minutes > 0) return `${minutes}m ${String(secs).padStart(2, '0')}s`;
  return `${Math.max(total, 1)}s`;
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