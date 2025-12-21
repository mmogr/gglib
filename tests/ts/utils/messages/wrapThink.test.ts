/**
 * Tests for wrapThink utility functions.
 * 
 * Tests wrapping and detection of thinking/reasoning content in <think> tags.
 */

import { describe, it, expect } from 'vitest';
import { wrapThink, isWrappedThink } from '../../../../src/utils/messages/wrapThink';

describe('wrapThink', () => {
  it('wraps text in <think> tags', () => {
    expect(wrapThink('Some thinking')).toBe('<think>\nSome thinking\n</think>');
  });

  it('trims input whitespace', () => {
    expect(wrapThink('  \n  Some thinking  \n  ')).toBe('<think>\nSome thinking\n</think>');
  });

  it('returns empty string for empty input', () => {
    expect(wrapThink('')).toBe('');
  });

  it('returns empty string for whitespace-only input', () => {
    expect(wrapThink('   \n  \t  ')).toBe('');
  });

  it('preserves internal whitespace and newlines', () => {
    const input = 'Line 1\n\nLine 2\n  Indented';
    expect(wrapThink(input)).toBe('<think>\nLine 1\n\nLine 2\n  Indented\n</think>');
  });

  it('handles multiline thinking content', () => {
    const input = `First line
Second line
Third line`;
    expect(wrapThink(input)).toBe(`<think>\nFirst line
Second line
Third line\n</think>`);
  });

  describe('with duration parameter', () => {
    it('adds duration attribute when provided', () => {
      expect(wrapThink('Thinking', 5.2)).toBe('<think duration="5.2">\nThinking\n</think>');
    });

    it('formats duration to 1 decimal place', () => {
      expect(wrapThink('Test', 1.234)).toBe('<think duration="1.2">\nTest\n</think>');
      expect(wrapThink('Test', 9.999)).toBe('<think duration="10.0">\nTest\n</think>');
      expect(wrapThink('Test', 3)).toBe('<think duration="3.0">\nTest\n</think>');
    });

    it('omits duration attribute when undefined', () => {
      expect(wrapThink('Test', undefined)).toBe('<think>\nTest\n</think>');
    });

    it('omits duration attribute for NaN', () => {
      expect(wrapThink('Test', NaN)).toBe('<think>\nTest\n</think>');
    });

    it('omits duration attribute for Infinity', () => {
      expect(wrapThink('Test', Infinity)).toBe('<think>\nTest\n</think>');
      expect(wrapThink('Test', -Infinity)).toBe('<think>\nTest\n</think>');
    });

    it('handles zero duration', () => {
      expect(wrapThink('Test', 0)).toBe('<think duration="0.0">\nTest\n</think>');
    });

    it('handles negative duration', () => {
      expect(wrapThink('Test', -1.5)).toBe('<think duration="-1.5">\nTest\n</think>');
    });

    it('still returns empty string for empty input even with duration', () => {
      expect(wrapThink('', 5.2)).toBe('');
      expect(wrapThink('   ', 3.0)).toBe('');
    });
  });
});

describe('isWrappedThink', () => {
  it('returns true for text starting with <think>', () => {
    expect(isWrappedThink('<think>content</think>')).toBe(true);
  });

  it('returns true for text starting with <think and attributes', () => {
    expect(isWrappedThink('<think duration="5.2">content</think>')).toBe(true);
  });

  it('ignores leading whitespace', () => {
    expect(isWrappedThink('  \n\t<think>content</think>')).toBe(true);
    expect(isWrappedThink('\n\n  <think>content</think>')).toBe(true);
  });

  it('returns false for text not starting with <think', () => {
    expect(isWrappedThink('Some regular text')).toBe(false);
    expect(isWrappedThink('Not a think tag')).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(isWrappedThink('')).toBe(false);
  });

  it('returns false for text containing <think but not at start', () => {
    expect(isWrappedThink('Some text <think>content</think>')).toBe(false);
  });

  it('handles case-sensitive matching', () => {
    expect(isWrappedThink('<think>content</think>')).toBe(true);
    expect(isWrappedThink('<THINK>content</THINK>')).toBe(false);
  });
});
