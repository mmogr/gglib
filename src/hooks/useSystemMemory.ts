import { useCallback, useEffect, useState } from "react";
import { SystemMemoryInfo, FitStatus } from "../types";
import { getSystemMemory } from "../services/clients/system";
import { getSettings } from "../services/clients/settings";

/**
 * Estimate the memory required to run a model.
 * 
 * Formula: required = file_size × 1.2 + (context_length / 1000) × 0.5GB
 * - File size × 1.2 accounts for runtime overhead (activation memory, etc.)
 * - KV cache overhead scales with context length (~0.5GB per 1K tokens)
 * 
 * @param fileSizeBytes - The model file size in bytes
 * @param contextLength - The context length to use (default: 4096)
 * @returns The estimated memory requirement in bytes
 */
export function estimateRequiredMemory(fileSizeBytes: number, contextLength: number = 4096): number {
  const modelMemory = fileSizeBytes * 1.2;
  const kvCacheOverhead = (contextLength / 1000) * 0.5 * 1024 * 1024 * 1024; // 0.5GB per 1K context
  return modelMemory + kvCacheOverhead;
}

/**
 * Determine fit status based on required vs available memory.
 * 
 * Thresholds:
 * - fits: Required memory < 85% of available
 * - tight: Required memory 85-100% of available
 * - wont_fit: Required memory > 100% of available
 * 
 * @param requiredBytes - Required memory in bytes
 * @param availableBytes - Available memory in bytes
 * @returns The fit status
 */
export function determineFitStatus(requiredBytes: number, availableBytes: number): FitStatus {
  const ratio = requiredBytes / availableBytes;
  
  if (ratio <= 0.85) {
    return 'fits';
  } else if (ratio <= 1.0) {
    return 'tight';
  } else {
    return 'wont_fit';
  }
}

/**
 * Format bytes to a human-readable string.
 */
export function formatMemorySize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  } else if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  } else {
    return `${(bytes / 1024).toFixed(0)} KB`;
  }
}

interface UseSystemMemoryReturn {
  /** System memory info (null while loading) */
  memoryInfo: SystemMemoryInfo | null;
  /** Whether memory info is still loading */
  loading: boolean;
  /** Error message if loading failed */
  error: string | null;
  /** Check if a model file will fit in available memory */
  checkFit: (fileSizeBytes: number) => FitStatus;
  /** Get estimated memory requirement for a file size */
  getEstimate: (fileSizeBytes: number) => number;
  /** Get available memory (GPU if available, else RAM) */
  availableMemory: number | null;
  /** Get a tooltip message for a file size */
  getTooltip: (fileSizeBytes: number) => string;
  /** Refresh memory info */
  refresh: () => Promise<void>;
}

/**
 * Hook for system memory detection and "Will it fit?" calculations.
 * 
 * Uses the default_context_size from settings to calculate KV cache overhead.
 * Caches memory info to avoid repeated API calls.
 */
export function useSystemMemory(): UseSystemMemoryReturn {
  const [memoryInfo, setMemoryInfo] = useState<SystemMemoryInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [contextLength, setContextLength] = useState<number>(4096);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const settings = await getSettings();
        if (!cancelled) {
          setContextLength(settings.default_context_size ?? 4096);
        }
      } catch {
        // Ignore settings load failures; default context length remains.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const loadMemoryInfo = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const info = await getSystemMemory();
      setMemoryInfo(info);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadMemoryInfo();
  }, [loadMemoryInfo]);

  // Calculate available memory (prefer GPU memory if available)
  const availableMemory = memoryInfo
    ? (memoryInfo.gpuMemoryBytes ?? memoryInfo.totalRamBytes)
    : null;

  const getEstimate = useCallback(
    (fileSizeBytes: number): number => {
      return estimateRequiredMemory(fileSizeBytes, contextLength);
    },
    [contextLength]
  );

  const checkFit = useCallback(
    (fileSizeBytes: number): FitStatus => {
      if (!availableMemory) {
        // Return 'unknown' if we can't determine memory availability
        return 'unknown';
      }
      const required = getEstimate(fileSizeBytes);
      return determineFitStatus(required, availableMemory);
    },
    [availableMemory, getEstimate]
  );

  const getTooltip = useCallback(
    (fileSizeBytes: number): string => {
      if (!memoryInfo || !availableMemory) {
        return "❓ Memory info unavailable\n\nSystem memory could not be determined. The fit indicator is unavailable.";
      }

      const required = getEstimate(fileSizeBytes);
      const status = determineFitStatus(required, availableMemory);
      
      const requiredStr = formatMemorySize(required);
      const availableStr = formatMemorySize(availableMemory);
      const memoryType = memoryInfo.gpuMemoryBytes 
        ? (memoryInfo.isAppleSilicon ? "Unified Memory" : "GPU VRAM")
        : "System RAM";
      
      let statusText: string;
      switch (status) {
        case 'fits':
          statusText = "✅ Should fit comfortably";
          break;
        case 'tight':
          statusText = "⚠️ Tight fit - may work";
          break;
        case 'wont_fit':
          statusText = "❌ Likely won't fit";
          break;
        case 'unknown':
          statusText = "❓ Unknown";
          break;
      }

      return `${statusText}\n\nEstimated: ${requiredStr}\nAvailable: ${availableStr} (${memoryType})\nContext: ${contextLength.toLocaleString()} tokens`;
    },
    [memoryInfo, availableMemory, getEstimate, contextLength]
  );

  return {
    memoryInfo,
    loading,
    error,
    checkFit,
    getEstimate,
    availableMemory,
    getTooltip,
    refresh: loadMemoryInfo,
  };
}
