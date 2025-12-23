/**
 * Tests for thinkingContentHandler utility.
 *
 * These tests verify the thinking content handler updates a PartsAccumulator:
 * - Accumulates reasoning_content deltas into reasoning parts
 * - Handles inline <think> tags in streaming content
 * - Transitions out of thinking when main content starts / thinking completes
 */

import { describe, it, expect } from 'vitest';
import { createThinkingContentHandler } from '../../../../src/hooks/useGglibRuntime/thinkingContentHandler';
import { PartsAccumulator } from '../../../../src/hooks/useGglibRuntime/partsAccumulator';

// =============================================================================
// Tests
// =============================================================================

describe('createThinkingContentHandler', () => {
  it('starts not thinking with empty accumulator', () => {
    const handler = createThinkingContentHandler();
    const acc = new PartsAccumulator();

    expect(handler.isThinking()).toBe(false);
    expect(acc.snapshot()).toEqual([]);
  });

  it('accumulates reasoning_content deltas into a reasoning part', () => {
    const handler = createThinkingContentHandler();
    const acc = new PartsAccumulator();

    handler.handleReasoningDelta('Let me ', acc);
    handler.handleReasoningDelta('think...', acc);

    expect(handler.isThinking()).toBe(true);
    expect(acc.snapshot()).toEqual([{ type: 'reasoning', text: 'Let me think...' }]);
  });

  it('transitions out of thinking when main content starts', () => {
    const handler = createThinkingContentHandler();
    const acc = new PartsAccumulator();

    handler.handleReasoningDelta('Thinking...', acc);
    expect(handler.isThinking()).toBe(true);

    handler.markMainContentStarted();
    expect(handler.isThinking()).toBe(false);
  });

  describe('inline <think> tag handling', () => {
    it('extracts inline thinking into reasoning + text parts', () => {
      const handler = createThinkingContentHandler();
      const acc = new PartsAccumulator();

      const content = '<think>Reasoning here</think>\nActual response';
      handler.handleContentDelta(content, content, acc);

      expect(handler.isThinking()).toBe(false);
      expect(acc.snapshot()).toEqual([
        { type: 'reasoning', text: 'Reasoning here' },
        { type: 'text', text: 'Actual response' },
      ]);
    });

    it('treats partial inline <think> as still thinking until completion', () => {
      const handler = createThinkingContentHandler();
      const acc = new PartsAccumulator();

      handler.handleContentDelta('<think>Start', '<think>Start', acc);
      expect(handler.isThinking()).toBe(true);
      expect(acc.snapshot()).toEqual([{ type: 'reasoning', text: 'Start' }]);

      const fullContent = '<think>Start thinking</think>\nDone';
      handler.handleContentDelta(' thinking</think>\nDone', fullContent, acc);
      expect(handler.isThinking()).toBe(false);
      expect(acc.snapshot()).toEqual([
        { type: 'reasoning', text: 'Start thinking' },
        { type: 'text', text: 'Done' },
      ]);
    });
  });
});
