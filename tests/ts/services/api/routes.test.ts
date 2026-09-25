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
  REMOTE_JOIN_PATH,
  VERSION_PATH,
} from '../../../../src/services/api/routes';

describe('services/api/routes', () => {
  describe('Daemon routes', () => {
    // Must stay in step with `VERSION_PATH` in
    // `gglib-core::contracts::http::daemon`; the dashboard's only source of
    // build provenance is this path resolving.
    it('VERSION_PATH is canonical', () => {
      expect(VERSION_PATH).toBe('/api/version');
    });

    // `REMOTE_JOIN_PATH` in `gglib-core::contracts::http::daemon`. The daemon
    // routes no other name for it, so a stale path here is a 405.
    it('REMOTE_JOIN_PATH is canonical', () => {
      expect(REMOTE_JOIN_PATH).toBe('/api/remote/join');
    });
  });

  describe('HuggingFace routes', () => {
    it('HF_SEARCH_PATH is canonical', () => {
      expect(HF_SEARCH_PATH).toBe('/api/models/hf/search');
    });

    it('HF_QUANTIZATIONS_PATH is canonical', () => {
      expect(HF_QUANTIZATIONS_PATH).toBe('/api/models/hf/quantizations');
    });

    it('HF_TOOL_SUPPORT_PATH is canonical', () => {
      expect(HF_TOOL_SUPPORT_PATH).toBe('/api/models/hf/tool-support');
    });
  });
});
