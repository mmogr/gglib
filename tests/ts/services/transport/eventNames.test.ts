/**
 * Tests for event name constants that protect frontend-backend contract.
 * 
 * These tests ensure the frontend Tauri event subscription stays in sync with
 * backend event emission. If backend event names change, these tests will fail,
 * preventing silent event subscription mismatches.
 * 
 * Context: Issue where Tauri GUI downloads started but progress UI never appeared
 * because frontend listened to unified "download-event" instead of granular
 * "download:started", "download:progress", etc. that backend actually emits.
 */

import { describe, it, expect } from 'vitest';
import {
  DOWNLOAD_EVENT_NAMES,
  SERVER_EVENT_NAMES,
  LOG_EVENT_NAMES,
  MCP_EVENT_NAMES,
  MODEL_EVENT_NAMES,
} from '../../../../src/services/transport/events/eventNames';

describe('Event Names Contract', () => {
  describe('DOWNLOAD_EVENT_NAMES', () => {
    it('includes all download event variants from backend', () => {
      // Must match AppEvent::event_name() outputs in crates/gglib-core/src/events/mod.rs
      expect(DOWNLOAD_EVENT_NAMES).toEqual([
        'download:started',
        'download:progress',
        'download:completed',
        'download:failed',
        'download:cancelled',
        'download:queue_snapshot',
        'download:queue_run_complete',
      ]);
    });

    it('maintains consistent array length', () => {
      // This protects against accidental additions/removals
      expect(DOWNLOAD_EVENT_NAMES).toHaveLength(7);
    });

    it('contains only string literals', () => {
      DOWNLOAD_EVENT_NAMES.forEach((name) => {
        expect(typeof name).toBe('string');
        expect(name.startsWith('download:')).toBe(true);
      });
    });
  });

  describe('SERVER_EVENT_NAMES', () => {
    it('includes all server event variants from backend', () => {
      expect(SERVER_EVENT_NAMES).toEqual([
        'server:started',
        'server:stopped',
        'server:error',
        'server:snapshot',
      ]);
    });

    it('contains only server-prefixed names', () => {
      SERVER_EVENT_NAMES.forEach((name) => {
        expect(name.startsWith('server:')).toBe(true);
      });
    });
  });

  describe('LOG_EVENT_NAMES', () => {
    it('includes all log event variants from backend', () => {
      expect(LOG_EVENT_NAMES).toEqual([
        'log:entry',
      ]);
    });
  });

  describe('MCP_EVENT_NAMES', () => {
    it('includes all MCP event variants from backend', () => {
      expect(MCP_EVENT_NAMES).toEqual([
        'mcp:added',
        'mcp:removed',
        'mcp:started',
        'mcp:stopped',
        'mcp:error',
      ]);
    });

    it('contains only mcp-prefixed names', () => {
      MCP_EVENT_NAMES.forEach((name) => {
        expect(name.startsWith('mcp:')).toBe(true);
      });
    });
  });

  describe('MODEL_EVENT_NAMES', () => {
    it('includes all model event variants from backend', () => {
      expect(MODEL_EVENT_NAMES).toEqual([
        'model:added',
        'model:removed',
        'model:updated',
      ]);
    });

    it('contains only model-prefixed names', () => {
      MODEL_EVENT_NAMES.forEach((name) => {
        expect(name.startsWith('model:')).toBe(true);
      });
    });
  });

  describe('No Overlap', () => {
    it('ensures event name arrays do not overlap', () => {
      const allNames = [
        ...DOWNLOAD_EVENT_NAMES,
        ...SERVER_EVENT_NAMES,
        ...LOG_EVENT_NAMES,
        ...MCP_EVENT_NAMES,
        ...MODEL_EVENT_NAMES,
      ];

      const uniqueNames = new Set(allNames);
      expect(uniqueNames.size).toBe(allNames.length);
    });
  });
});
