/**
 * Tests for thinkingContentHandler utility.
 *
 * These tests verify the thinking content handler:
 * - Accumulates reasoning_content deltas
 * - Tracks timing for thinking phase
 * - Handles inline <think> tags
 * - Builds display content with duration
 * - Resets state correctly
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createThinkingContentHandler } from '../../../../src/hooks/useGglibRuntime/thinkingContentHandler';

// =============================================================================
// Tests
// =============================================================================

describe('createThinkingContentHandler', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('initialization', () => {
    it('starts with empty state', () => {
      const handler = createThinkingContentHandler();
      const state = handler.getState();

      expect(state.isThinking).toBe(false);
      expect(state.thinkingContent).toBe('');
      expect(state.startedAt).toBeNull();
      expect(state.endedAt).toBeNull();
    });
  });

  describe('reasoning_content handling', () => {
    it('accumulates reasoning content', () => {
      const handler = createThinkingContentHandler();

      handler.handleReasoningDelta('Let me ');
      handler.handleReasoningDelta('think...');

      const state = handler.getState();

      expect(state.thinkingContent).toBe('Let me think...');
      expect(state.isThinking).toBe(true);
    });

    it('tracks start time on first delta', () => {
      const handler = createThinkingContentHandler();

      vi.setSystemTime(new Date('2024-01-01T00:00:05.000Z'));
      handler.handleReasoningDelta('Thinking...');

      const state = handler.getState();

      expect(state.startedAt).toBe(new Date('2024-01-01T00:00:05.000Z').getTime());
    });

    it('does not update start time on subsequent deltas', () => {
      const handler = createThinkingContentHandler();

      vi.setSystemTime(new Date('2024-01-01T00:00:05.000Z'));
      handler.handleReasoningDelta('First');

      vi.setSystemTime(new Date('2024-01-01T00:00:10.000Z'));
      handler.handleReasoningDelta(' Second');

      const state = handler.getState();

      expect(state.startedAt).toBe(new Date('2024-01-01T00:00:05.000Z').getTime());
    });

    it('marks end time when main content starts', () => {
      const handler = createThinkingContentHandler();

      vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
      handler.handleReasoningDelta('Thinking...');

      vi.setSystemTime(new Date('2024-01-01T00:00:03.000Z'));
      handler.markMainContentStarted();

      const state = handler.getState();

      expect(state.endedAt).toBe(new Date('2024-01-01T00:00:03.000Z').getTime());
      expect(state.isThinking).toBe(false);
    });
  });

  describe('buildDisplayContent', () => {
    it('returns main content when no thinking', () => {
      const handler = createThinkingContentHandler();

      const display = handler.buildDisplayContent('Hello world');

      expect(display).toBe('Hello world');
    });

    it('embeds thinking content with duration', () => {
      const handler = createThinkingContentHandler();

      vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
      handler.handleReasoningDelta('Let me think...');

      vi.setSystemTime(new Date('2024-01-01T00:00:05.000Z'));
      handler.markMainContentStarted();

      const display = handler.buildDisplayContent('The answer');

      expect(display).toContain('<think');
      expect(display).toContain('duration="5.0"');
      expect(display).toContain('Let me think...');
      expect(display).toContain('The answer');
    });

    it('uses current time if thinking not ended', () => {
      const handler = createThinkingContentHandler();

      vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
      handler.handleReasoningDelta('Still thinking...');

      vi.setSystemTime(new Date('2024-01-01T00:00:02.500Z'));
      const display = handler.buildDisplayContent('');

      expect(display).toContain('duration="2.5"');
    });
  });

  describe('inline <think> tag handling', () => {
    it('detects inline thinking tags and uses start time for duration', () => {
      const handler = createThinkingContentHandler();

      // When content arrives with complete <think> tags, the handler
      // records start time on first delta and end time when complete
      vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
      const content = '<think>Reasoning here</think>\nActual response';
      handler.handleContentDelta(content, content);

      const display = handler.buildDisplayContent(content);

      // Duration is 0 because start and end happen in same tick for complete content
      expect(display).toContain('duration="0.0"');
      expect(display).toContain('Reasoning here');
      expect(display).toContain('Actual response');
    });

    it('tracks inline thinking timing', () => {
      const handler = createThinkingContentHandler();

      // Partial thinking tag
      vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
      handler.handleContentDelta('<think>Start', '<think>Start');

      // Complete thinking tag
      vi.setSystemTime(new Date('2024-01-01T00:00:02.000Z'));
      const fullContent = '<think>Start thinking</think>\nDone';
      handler.handleContentDelta(' thinking</think>\nDone', fullContent);

      const display = handler.buildDisplayContent(fullContent);

      expect(display).toContain('duration="2.0"');
    });
  });

  describe('buildFinalContent', () => {
    it('returns main content when no thinking', () => {
      const handler = createThinkingContentHandler();

      const final = handler.buildFinalContent('Final answer');

      expect(final).toBe('Final answer');
    });

    it('embeds thinking with final duration', () => {
      const handler = createThinkingContentHandler();

      vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
      handler.handleReasoningDelta('Deep thought');

      vi.setSystemTime(new Date('2024-01-01T00:00:10.000Z'));
      handler.markMainContentStarted();

      const final = handler.buildFinalContent('42');

      expect(final).toContain('duration="10.0"');
      expect(final).toContain('Deep thought');
      expect(final).toContain('42');
    });
  });

  describe('reset', () => {
    it('clears all state', () => {
      const handler = createThinkingContentHandler();

      handler.handleReasoningDelta('Some thinking');
      handler.markMainContentStarted();

      handler.reset();

      const state = handler.getState();

      expect(state.isThinking).toBe(false);
      expect(state.thinkingContent).toBe('');
      expect(state.startedAt).toBeNull();
      expect(state.endedAt).toBeNull();
    });

    it('allows reuse after reset', () => {
      const handler = createThinkingContentHandler();

      handler.handleReasoningDelta('Old thought');
      handler.reset();

      vi.setSystemTime(new Date('2024-01-01T00:00:00.000Z'));
      handler.handleReasoningDelta('New thought');

      const state = handler.getState();

      expect(state.thinkingContent).toBe('New thought');
      expect(state.startedAt).toBe(new Date('2024-01-01T00:00:00.000Z').getTime());
    });
  });
});
