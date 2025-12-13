/**
 * Tests for API route constants.
 *
 * Ensures route constants match their canonical values to prevent drift.
 */

import { describe, it, expect } from 'vitest';
import {
  HF_SEARCH_PATH,
  HF_QUANTIZATIONS_PATH,
  HF_TOOL_SUPPORT_PATH,
} from '../../../../src/services/api/routes';

describe('services/api/routes', () => {
  describe('HuggingFace routes', () => {
    it('HF_SEARCH_PATH is canonical', () => {
      expect(HF_SEARCH_PATH).toBe('/api/hf/search');
    });

    it('HF_QUANTIZATIONS_PATH is canonical', () => {
      expect(HF_QUANTIZATIONS_PATH).toBe('/api/hf/quantizations');
    });

    it('HF_TOOL_SUPPORT_PATH is canonical', () => {
      expect(HF_TOOL_SUPPORT_PATH).toBe('/api/hf/tool-support');
    });
  });
});
