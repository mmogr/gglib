/**
 * Tests for thinkingParser utility functions.
 * 
 * Tests parsing of thinking/reasoning content from various AI model formats.
 */

import { describe, it, expect } from 'vitest';
import {
  normalizeThinkingTags,
  parseThinkingContent,
  embedThinkingContent,
  hasThinkingContent,
  parseStreamingThinkingContent,
  formatThinkingDuration,
} from '../../../src/utils/thinkingParser';

describe('normalizeThinkingTags', () => {
  it('returns empty/null input unchanged', () => {
    expect(normalizeThinkingTags('')).toBe('');
    // @ts-expect-error Testing null input
    expect(normalizeThinkingTags(null)).toBe(null);
    // @ts-expect-error Testing undefined input
    expect(normalizeThinkingTags(undefined)).toBe(undefined);
  });

  it('leaves standard <think> tags unchanged', () => {
    const input = '<think>Some thinking</think>Response';
    expect(normalizeThinkingTags(input)).toBe(input);
  });

  it('normalizes <seed:think> tags to <think>', () => {
    const input = '<seed:think>Seed thinking</seed:think>Response';
    expect(normalizeThinkingTags(input)).toBe('<think>Seed thinking</think>Response');
  });

  it('normalizes <|START_THINKING|> tags to <think>', () => {
    const input = '<|START_THINKING|>Command R thinking<|END_THINKING|>Response';
    expect(normalizeThinkingTags(input)).toBe('<think>Command R thinking</think>Response');
  });

  it('normalizes <reasoning> tags to <think>', () => {
    const input = '<reasoning>Deep reasoning</reasoning>Response';
    expect(normalizeThinkingTags(input)).toBe('<think>Deep reasoning</think>Response');
  });

  it('handles case-insensitive tags', () => {
    expect(normalizeThinkingTags('<THINK>Upper</THINK>')).toBe('<THINK>Upper</THINK>');
    expect(normalizeThinkingTags('<Reasoning>Mixed</Reasoning>')).toBe('<think>Mixed</think>');
    expect(normalizeThinkingTags('<SEED:THINK>Upper seed</SEED:THINK>')).toBe('<think>Upper seed</think>');
  });

  it('handles multiple tag normalizations in one text', () => {
    const input = '<reasoning>First</reasoning> and <seed:think>Second</seed:think>';
    expect(normalizeThinkingTags(input)).toBe('<think>First</think> and <think>Second</think>');
  });
});

describe('parseThinkingContent', () => {
  it('returns empty result for empty/null input', () => {
    expect(parseThinkingContent('')).toEqual({
      thinking: null,
      content: '',
      durationSeconds: null,
    });
    // @ts-expect-error Testing null input
    expect(parseThinkingContent(null)).toEqual({
      thinking: null,
      content: '',
      durationSeconds: null,
    });
  });

  it('returns content unchanged when no thinking tags', () => {
    const input = 'Just a normal response without thinking';
    expect(parseThinkingContent(input)).toEqual({
      thinking: null,
      content: input,
      durationSeconds: null,
    });
  });

  it('extracts thinking from standard <think> tags', () => {
    const input = '<think>I need to analyze this carefully</think>\nHere is my response.';
    const result = parseThinkingContent(input);
    expect(result.thinking).toBe('I need to analyze this carefully');
    expect(result.content).toBe('Here is my response.');
    expect(result.durationSeconds).toBeNull();
  });

  it('extracts thinking with duration attribute', () => {
    const input = '<think duration="5.2">Quick thinking</think>\nResponse here.';
    const result = parseThinkingContent(input);
    expect(result.thinking).toBe('Quick thinking');
    expect(result.content).toBe('Response here.');
    expect(result.durationSeconds).toBe(5.2);
  });

  it('handles multiline thinking content', () => {
    const input = `<think>
First line of thinking
Second line of thinking
Third line
</think>
The actual response.`;
    const result = parseThinkingContent(input);
    expect(result.thinking).toContain('First line of thinking');
    expect(result.thinking).toContain('Third line');
    expect(result.content).toBe('The actual response.');
  });

  it('normalizes and parses <reasoning> tags', () => {
    const input = '<reasoning>Deep analysis here</reasoning>\nFinal answer.';
    const result = parseThinkingContent(input);
    expect(result.thinking).toBe('Deep analysis here');
    expect(result.content).toBe('Final answer.');
  });

  it('normalizes and parses <seed:think> tags', () => {
    const input = '<seed:think>Seed model thinking</seed:think>\nSeed response.';
    const result = parseThinkingContent(input);
    expect(result.thinking).toBe('Seed model thinking');
    expect(result.content).toBe('Seed response.');
  });

  it('normalizes and parses Command-R style tags', () => {
    const input = '<|START_THINKING|>Command R analysis<|END_THINKING|>\nCommand R response.';
    const result = parseThinkingContent(input);
    expect(result.thinking).toBe('Command R analysis');
    expect(result.content).toBe('Command R response.');
  });

  it('only matches thinking tags at the start', () => {
    const input = 'Some prefix <think>thinking</think> response';
    const result = parseThinkingContent(input);
    expect(result.thinking).toBeNull();
    expect(result.content).toBe(input);
  });

  it('handles empty thinking content', () => {
    const input = '<think></think>Just the response';
    const result = parseThinkingContent(input);
    expect(result.thinking).toBeNull();
    expect(result.content).toBe('Just the response');
  });
});

describe('embedThinkingContent', () => {
  it('returns content unchanged when no thinking', () => {
    expect(embedThinkingContent(null, 'Response')).toBe('Response');
    expect(embedThinkingContent('', 'Response')).toBe('Response');
  });

  it('embeds thinking with <think> tags', () => {
    const result = embedThinkingContent('My thinking', 'My response');
    expect(result).toBe('<think>My thinking</think>\nMy response');
  });

  it('includes duration attribute when provided', () => {
    const result = embedThinkingContent('My thinking', 'My response', 5.2);
    expect(result).toBe('<think duration="5.2">My thinking</think>\nMy response');
  });

  it('handles zero duration', () => {
    const result = embedThinkingContent('Quick', 'Response', 0);
    expect(result).toBe('<think duration="0.0">Quick</think>\nResponse');
  });

  it('ignores null duration', () => {
    const result = embedThinkingContent('Thinking', 'Response', null);
    expect(result).toBe('<think>Thinking</think>\nResponse');
  });

  it('roundtrips with parseThinkingContent', () => {
    const thinking = 'Original thinking';
    const content = 'Original response';
    const duration = 3.5;

    const embedded = embedThinkingContent(thinking, content, duration);
    const parsed = parseThinkingContent(embedded);

    expect(parsed.thinking).toBe(thinking);
    expect(parsed.content).toBe(content);
    expect(parsed.durationSeconds).toBe(duration);
  });
});

describe('hasThinkingContent', () => {
  it('returns false for empty input', () => {
    expect(hasThinkingContent('')).toBe(false);
  });

  it('returns true for <think> tags', () => {
    expect(hasThinkingContent('<think>content</think>')).toBe(true);
    expect(hasThinkingContent('  <think>with whitespace')).toBe(true);
  });

  it('returns true for <reasoning> tags', () => {
    expect(hasThinkingContent('<reasoning>content</reasoning>')).toBe(true);
    expect(hasThinkingContent('\n<reasoning>newline prefix')).toBe(true);
  });

  it('returns true for <seed:think> tags', () => {
    expect(hasThinkingContent('<seed:think>content</seed:think>')).toBe(true);
  });

  it('returns true for Command-R style tags', () => {
    expect(hasThinkingContent('<|START_THINKING|>content')).toBe(true);
    expect(hasThinkingContent('<|start_thinking|>lowercase')).toBe(true);
  });

  it('returns false when tags are not at start', () => {
    expect(hasThinkingContent('prefix <think>content</think>')).toBe(false);
    expect(hasThinkingContent('Hello <reasoning>world</reasoning>')).toBe(false);
  });

  it('handles case-insensitive matching', () => {
    expect(hasThinkingContent('<THINK>upper')).toBe(true);
    expect(hasThinkingContent('<Reasoning>mixed')).toBe(true);
  });
});

describe('parseStreamingThinkingContent', () => {
  it('returns empty thinking for no tags', () => {
    const result = parseStreamingThinkingContent('Normal content');
    expect(result).toEqual({
      thinking: '',
      content: 'Normal content',
      isThinkingComplete: true,
    });
  });

  it('parses complete thinking block', () => {
    const result = parseStreamingThinkingContent('<think>Complete thought</think>Response');
    expect(result).toEqual({
      thinking: 'Complete thought',
      content: 'Response',
      isThinkingComplete: true,
    });
  });

  it('detects incomplete thinking (streaming)', () => {
    const result = parseStreamingThinkingContent('<think>Still thinking...');
    expect(result).toEqual({
      thinking: 'Still thinking...',
      content: '',
      isThinkingComplete: false,
    });
  });

  it('handles empty thinking in progress', () => {
    const result = parseStreamingThinkingContent('<think>');
    expect(result).toEqual({
      thinking: '',
      content: '',
      isThinkingComplete: false,
    });
  });

  it('normalizes and parses <reasoning> tags', () => {
    const result = parseStreamingThinkingContent('<reasoning>Analyzing</reasoning>Done');
    expect(result).toEqual({
      thinking: 'Analyzing',
      content: 'Done',
      isThinkingComplete: true,
    });
  });

  it('handles streaming <seed:think> tags', () => {
    const result = parseStreamingThinkingContent('<seed:think>Processing...');
    expect(result.isThinkingComplete).toBe(false);
    expect(result.thinking).toBe('Processing...');
  });

  it('handles Command-R style tags', () => {
    const result = parseStreamingThinkingContent('<|START_THINKING|>Thinking<|END_THINKING|>Response');
    expect(result.isThinkingComplete).toBe(true);
    expect(result.thinking).toBe('Thinking');
    expect(result.content).toBe('Response');
  });

  it('handles multiline streaming content', () => {
    const input = `<think>Line 1
Line 2
Line 3`;
    const result = parseStreamingThinkingContent(input);
    expect(result.isThinkingComplete).toBe(false);
    expect(result.thinking).toContain('Line 1');
    expect(result.thinking).toContain('Line 3');
  });

  it('trims whitespace after closing tag', () => {
    const result = parseStreamingThinkingContent('<think>Thought</think>   Content with spaces');
    // The parser trims leading whitespace from content
    expect(result.content).toBe('Content with spaces');
    expect(result.thinking).toBe('Thought');
  });
});

describe('formatThinkingDuration', () => {
  it('formats seconds under 60', () => {
    expect(formatThinkingDuration(0)).toBe('0.0s');
    expect(formatThinkingDuration(1)).toBe('1.0s');
    expect(formatThinkingDuration(5.5)).toBe('5.5s');
    expect(formatThinkingDuration(59.9)).toBe('59.9s');
  });

  it('formats minutes and seconds', () => {
    expect(formatThinkingDuration(60)).toBe('1m 0s');
    expect(formatThinkingDuration(61)).toBe('1m 1s');
    expect(formatThinkingDuration(90)).toBe('1m 30s');
    expect(formatThinkingDuration(125)).toBe('2m 5s');
  });

  it('handles fractional seconds in minutes format', () => {
    // The function rounds seconds, so 90.5 -> 91s -> 1m 31s
    expect(formatThinkingDuration(90.5)).toBe('1m 31s');
    expect(formatThinkingDuration(61.9)).toBe('1m 2s');
  });

  it('handles large values', () => {
    expect(formatThinkingDuration(3600)).toBe('60m 0s');
    expect(formatThinkingDuration(3665)).toBe('61m 5s');
  });
});
