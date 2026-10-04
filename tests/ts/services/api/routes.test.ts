/**
 * Tests for API route constants.
 *
 * Ensures route constants match their canonical values to prevent drift.
 */

import { describe, it, expect } from 'vitest';
import {
  ATTACHMENTS_PATH,
  HF_SEARCH_PATH,
  HF_QUANTIZATIONS_PATH,
  HF_TOOL_SUPPORT_PATH,
  REMOTE_ATTACHMENTS_PATH,
  REMOTE_CHATS_PATH,
  REMOTE_JOIN_PATH,
  REMOTE_RUNS_PATH,
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

    // `REMOTE_CHATS_PATH` and `REMOTE_RUNS_PATH` in
    // `gglib-core::contracts::http::daemon`: the chat page reads the far
    // machine's chats and runs under these, and a stale one is a 404.
    it('REMOTE_CHATS_PATH and REMOTE_RUNS_PATH are canonical', () => {
      expect(REMOTE_CHATS_PATH).toBe('/api/remote/chats');
      expect(REMOTE_RUNS_PATH).toBe('/api/remote/runs');
    });

    // `ATTACHMENTS_PATH` and `REMOTE_ATTACHMENTS_PATH` in
    // `gglib-core::contracts::http::attachments`: the chat page uploads and
    // reads a chat's images under these, its own or the far machine's.
    it('ATTACHMENTS_PATH and REMOTE_ATTACHMENTS_PATH are canonical', () => {
      expect(ATTACHMENTS_PATH).toBe('/api/attachments');
      expect(REMOTE_ATTACHMENTS_PATH).toBe('/api/remote/attachments');
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
