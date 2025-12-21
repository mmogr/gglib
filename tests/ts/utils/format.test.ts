/**
 * Tests for format utility functions.
 * 
 * These are pure functions with no dependencies - straightforward input/output testing.
 */

import { describe, it, expect } from 'vitest';
import {
  formatBytes,
  formatTime,
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
    expect(formatBytes(1024)).toBe('1 KB');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(10240)).toBe('10 KB');
  });

  it('formats megabytes correctly', () => {
    expect(formatBytes(1048576)).toBe('1 MB');
    expect(formatBytes(1572864)).toBe('1.5 MB');
    expect(formatBytes(10485760)).toBe('10 MB');
  });

  it('formats gigabytes correctly', () => {
    expect(formatBytes(1073741824)).toBe('1 GB');
    expect(formatBytes(5368709120)).toBe('5 GB');
    expect(formatBytes(7516192768)).toBe('7 GB');
  });

  it('formats terabytes correctly', () => {
    expect(formatBytes(1099511627776)).toBe('1 TB');
    expect(formatBytes(2199023255552)).toBe('2 TB');
  });

  it('respects decimal places parameter', () => {
    expect(formatBytes(1536, 0)).toBe('2 KB');
    expect(formatBytes(1536, 1)).toBe('1.5 KB');
    expect(formatBytes(1536, 3)).toBe('1.5 KB');
  });

  it('handles negative decimal places as 0', () => {
    expect(formatBytes(1536, -1)).toBe('2 KB');
  });
});

describe('formatTime', () => {
  it('returns "Calculating..." for non-finite values', () => {
    expect(formatTime(Infinity)).toBe('Calculating...');
    expect(formatTime(-Infinity)).toBe('Calculating...');
    expect(formatTime(NaN)).toBe('Calculating...');
  });

  it('returns "Calculating..." for negative values', () => {
    expect(formatTime(-1)).toBe('Calculating...');
    expect(formatTime(-100)).toBe('Calculating...');
  });

  it('formats seconds correctly', () => {
    expect(formatTime(0)).toBe('0s');
    expect(formatTime(1)).toBe('1s');
    expect(formatTime(30)).toBe('30s');
    expect(formatTime(59)).toBe('59s');
  });

  it('rounds up fractional seconds', () => {
    expect(formatTime(0.1)).toBe('1s');
    expect(formatTime(0.9)).toBe('1s');
    expect(formatTime(29.1)).toBe('30s');
  });

  it('formats minutes and seconds correctly', () => {
    expect(formatTime(60)).toBe('1m 0s');
    expect(formatTime(61)).toBe('1m 1s');
    expect(formatTime(90)).toBe('1m 30s');
    expect(formatTime(120)).toBe('2m 0s');
    expect(formatTime(150)).toBe('2m 30s');
  });

  it('handles large values', () => {
    expect(formatTime(3600)).toBe('60m 0s');
    expect(formatTime(3661)).toBe('61m 1s');
  });

  it('rounds up remaining seconds in minutes format', () => {
    expect(formatTime(90.5)).toBe('1m 31s');
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
