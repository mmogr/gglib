/**
 * Tests for format utility functions.
 * 
 * These are pure functions with no dependencies - straightforward input/output testing.
 */

import { describe, it, expect } from 'vitest';
import {
  formatBytes,
  formatDuration,
  formatRate,
  formatNumber,
  formatParamCount,
  getHuggingFaceUrl,
  getHuggingFaceModelUrl,
} from '../../../src/utils/format';

describe('formatBytes', () => {
  it('returns "0 Bytes" for 0', () => {
    expect(formatBytes(0)).toBe('0 Bytes');
  });

  it('formats bytes correctly', () => {
    expect(formatBytes(500)).toBe('500 Bytes');
    expect(formatBytes(1023)).toBe('1023 Bytes');
  });

  it('formats kilobytes correctly', () => {
    expect(formatBytes(1024)).toBe('1 KiB');
    expect(formatBytes(1536)).toBe('1.5 KiB');
    expect(formatBytes(10240)).toBe('10 KiB');
  });

  it('formats megabytes correctly', () => {
    expect(formatBytes(1048576)).toBe('1 MiB');
    expect(formatBytes(1572864)).toBe('1.5 MiB');
    expect(formatBytes(10485760)).toBe('10 MiB');
  });

  it('formats gigabytes correctly', () => {
    expect(formatBytes(1073741824)).toBe('1 GiB');
    expect(formatBytes(5368709120)).toBe('5 GiB');
    expect(formatBytes(7516192768)).toBe('7 GiB');
  });

  it('formats terabytes correctly', () => {
    expect(formatBytes(1099511627776)).toBe('1 TiB');
    expect(formatBytes(2199023255552)).toBe('2 TiB');
  });

  it('respects decimal places parameter', () => {
    expect(formatBytes(1536, 0)).toBe('2 KiB');
    expect(formatBytes(1536, 1)).toBe('1.5 KiB');
    expect(formatBytes(1536, 3)).toBe('1.5 KiB');
  });

  it('handles negative decimal places as 0', () => {
    expect(formatBytes(1536, -1)).toBe('2 KiB');
  });
});

describe('formatRate', () => {
  it('returns a placeholder when the rate is not known', () => {
    // Absent means "the estimator has not warmed up", not "stalled". Rendering
    // 0 here is what made a healthy download look stuck.
    expect(formatRate(undefined)).toBe('—');
    expect(formatRate(null)).toBe('—');
    expect(formatRate(NaN)).toBe('—');
    expect(formatRate(Infinity)).toBe('—');
    expect(formatRate(-1)).toBe('—');
  });

  it('uses decimal units so the number matches a system network monitor', () => {
    expect(formatRate(1_000_000)).toBe('1.0 MB/s');
    // 1 MiB/s reads as 1.0 MB/s, not 1.0: decimal is the unit users compare to.
    expect(formatRate(1_048_576)).toBe('1.0 MB/s');
  });

  it('scales by magnitude', () => {
    expect(formatRate(0)).toBe('0 B/s');
    expect(formatRate(999)).toBe('999 B/s');
    expect(formatRate(1_000)).toBe('1 kB/s');
    expect(formatRate(1_500_000)).toBe('1.5 MB/s');
    expect(formatRate(118_400_000)).toBe('118.4 MB/s');
    expect(formatRate(2_500_000_000)).toBe('2.50 GB/s');
  });
});

describe('formatDuration', () => {
  it('returns a placeholder when the ETA is not known', () => {
    expect(formatDuration(undefined)).toBe('—');
    expect(formatDuration(null)).toBe('—');
    expect(formatDuration(NaN)).toBe('—');
    expect(formatDuration(-5)).toBe('—');
  });

  it('never shows 0s while work is in flight', () => {
    expect(formatDuration(0)).toBe('1s');
    expect(formatDuration(0.1)).toBe('1s');
  });

  it('scales by magnitude', () => {
    expect(formatDuration(45)).toBe('45s');
    expect(formatDuration(59.4)).toBe('1m 00s');
    expect(formatDuration(200)).toBe('3m 20s');
    expect(formatDuration(3_600)).toBe('1h 00m');
    expect(formatDuration(3_845)).toBe('1h 04m');
  });

  it('saturates rather than overflowing on absurd input', () => {
    expect(formatDuration(1e18)).toBe('99h 59m');
  });
});

describe('formatNumber', () => {
  it('formats small numbers as-is', () => {
    expect(formatNumber(0)).toBe('0');
    expect(formatNumber(1)).toBe('1');
    expect(formatNumber(999)).toBe('999');
  });

  it('formats thousands with K suffix', () => {
    expect(formatNumber(1000)).toBe('1.0K');
    expect(formatNumber(1500)).toBe('1.5K');
    expect(formatNumber(10000)).toBe('10.0K');
    expect(formatNumber(999999)).toBe('1000.0K');
  });

  it('formats millions with M suffix', () => {
    expect(formatNumber(1000000)).toBe('1.0M');
    expect(formatNumber(1500000)).toBe('1.5M');
    expect(formatNumber(10000000)).toBe('10.0M');
    expect(formatNumber(999999999)).toBe('1000.0M');
  });

  it('handles edge cases at boundaries', () => {
    expect(formatNumber(999)).toBe('999');
    expect(formatNumber(1000)).toBe('1.0K');
    expect(formatNumber(999999)).toBe('1000.0K');
    expect(formatNumber(1000000)).toBe('1.0M');
  });
});

describe('formatParamCount', () => {
  it('formats billions with B suffix', () => {
    expect(formatParamCount(1)).toBe('1.0B');
    expect(formatParamCount(7)).toBe('7.0B');
    expect(formatParamCount(7.5)).toBe('7.5B');
    expect(formatParamCount(70)).toBe('70.0B');
    expect(formatParamCount(405)).toBe('405.0B');
  });

  it('formats sub-billion as millions with M suffix', () => {
    expect(formatParamCount(0.5)).toBe('500M');
    expect(formatParamCount(0.125)).toBe('125M');
    expect(formatParamCount(0.1)).toBe('100M');
  });

  it('handles boundary at 1B', () => {
    expect(formatParamCount(0.999)).toBe('999M');
    expect(formatParamCount(1.0)).toBe('1.0B');
  });

  it('handles very small param counts', () => {
    expect(formatParamCount(0.001)).toBe('1M');
    expect(formatParamCount(0.01)).toBe('10M');
  });
});

describe('getHuggingFaceUrl', () => {
  it('returns null for null/undefined repo ID', () => {
    expect(getHuggingFaceUrl(null)).toBeNull();
    expect(getHuggingFaceUrl(undefined)).toBeNull();
    expect(getHuggingFaceUrl('')).toBeNull();
  });

  it('returns repo URL without filename', () => {
    expect(getHuggingFaceUrl('TheBloke/Llama-2-7B-GGUF')).toBe(
      'https://huggingface.co/TheBloke/Llama-2-7B-GGUF'
    );
    expect(getHuggingFaceUrl('meta-llama/Meta-Llama-3-8B')).toBe(
      'https://huggingface.co/meta-llama/Meta-Llama-3-8B'
    );
  });

  it('returns file URL with filename', () => {
    expect(getHuggingFaceUrl('TheBloke/Llama-2-7B-GGUF', 'llama-2-7b.Q4_K_M.gguf')).toBe(
      'https://huggingface.co/TheBloke/Llama-2-7B-GGUF/blob/main/llama-2-7b.Q4_K_M.gguf'
    );
  });

  it('handles null/undefined filename', () => {
    expect(getHuggingFaceUrl('TheBloke/Llama-2-7B-GGUF', null)).toBe(
      'https://huggingface.co/TheBloke/Llama-2-7B-GGUF'
    );
    expect(getHuggingFaceUrl('TheBloke/Llama-2-7B-GGUF', undefined)).toBe(
      'https://huggingface.co/TheBloke/Llama-2-7B-GGUF'
    );
  });
});

describe('getHuggingFaceModelUrl', () => {
  it('returns model URL for model ID', () => {
    expect(getHuggingFaceModelUrl('TheBloke/Llama-2-7B-GGUF')).toBe(
      'https://huggingface.co/TheBloke/Llama-2-7B-GGUF'
    );
    expect(getHuggingFaceModelUrl('mistralai/Mistral-7B-v0.1')).toBe(
      'https://huggingface.co/mistralai/Mistral-7B-v0.1'
    );
  });

  it('handles various model ID formats', () => {
    expect(getHuggingFaceModelUrl('user/model')).toBe('https://huggingface.co/user/model');
    expect(getHuggingFaceModelUrl('org/model-name-v1')).toBe('https://huggingface.co/org/model-name-v1');
  });
});
