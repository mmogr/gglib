/**
 * Tests for the promptBuilder composition utilities.
 *
 * Coverage:
 * - buildSystemPrompt() — no layers, single prepend, single append, priority
 *   ordering, mixed prepend+append, empty base prompt filtering
 * - TOOL_INSTRUCTIONS_LAYER and FORMAT_REMINDER_LAYER shape invariants
 * - Key regression: injectToolInstructions-style composition works for
 *   *any* base prompt, not just the exact DEFAULT_SYSTEM_PROMPT string
 */

import { describe, it, expect } from 'vitest';
import {
  buildSystemPrompt,
  injectPromptLayers,
  TOOL_INSTRUCTIONS_LAYER,
  FORMAT_REMINDER_LAYER,
  FORMAT_REMINDER,
  type PromptLayer,
} from '../../../../src/hooks/useGglibRuntime/promptBuilder';

// =============================================================================
// helpers
// =============================================================================

function layer(overrides: Partial<PromptLayer> & Pick<PromptLayer, 'id' | 'content'>): PromptLayer {
  return {
    position: 'append',
    priority: 100,
    ...overrides,
  };
}

// =============================================================================
// buildSystemPrompt
// =============================================================================

describe('buildSystemPrompt', () => {
  describe('no layers', () => {
    it('returns the base prompt unchanged', () => {
      expect(buildSystemPrompt('You are a helpful assistant.', [])).toBe(
        'You are a helpful assistant.',
      );
    });

    it('returns an empty string when both base and layers are empty', () => {
      expect(buildSystemPrompt('', [])).toBe('');
    });
  });

  describe('single append layer', () => {
    it('appends the layer content after the base prompt', () => {
      const l = layer({ id: 'a', content: 'Use tools.' });
      expect(buildSystemPrompt('You are a helpful assistant.', [l])).toBe(
        'You are a helpful assistant.\n\nUse tools.',
      );
    });
  });

  describe('single prepend layer', () => {
    it('prepends the layer content before the base prompt', () => {
      const l = layer({ id: 'a', content: 'Context preamble.', position: 'prepend' });
      expect(buildSystemPrompt('You are a helpful assistant.', [l])).toBe(
        'Context preamble.\n\nYou are a helpful assistant.',
      );
    });
  });

  describe('priority ordering', () => {
    it('sorts multiple append layers by ascending priority', () => {
      const high = layer({ id: 'high', content: 'HIGH', priority: 50 });
      const low  = layer({ id: 'low',  content: 'LOW',  priority: 200 });
      const mid  = layer({ id: 'mid',  content: 'MID',  priority: 100 });

      // Intentionally pass in reverse order to prove sorting is applied.
      expect(buildSystemPrompt('BASE', [low, high, mid])).toBe(
        'BASE\n\nHIGH\n\nMID\n\nLOW',
      );
    });

    it('sorts multiple prepend layers by ascending priority', () => {
      const first  = layer({ id: 'first',  content: 'FIRST',  position: 'prepend', priority: 10 });
      const second = layer({ id: 'second', content: 'SECOND', position: 'prepend', priority: 20 });

      expect(buildSystemPrompt('BASE', [second, first])).toBe(
        'FIRST\n\nSECOND\n\nBASE',
      );
    });
  });

  describe('mixed prepend and append layers', () => {
    it('places prepends before base and appends after base', () => {
      const pre  = layer({ id: 'pre',  content: 'PRE',  position: 'prepend', priority: 10 });
      const post = layer({ id: 'post', content: 'POST', position: 'append',  priority: 10 });

      expect(buildSystemPrompt('BASE', [post, pre])).toBe('PRE\n\nBASE\n\nPOST');
    });
  });

  describe('empty segment filtering', () => {
    it('does not produce a leading newline when the base prompt is empty', () => {
      const l = layer({ id: 'a', content: 'Use tools.' });
      const result = buildSystemPrompt('', [l]);
      expect(result).toBe('Use tools.');
      expect(result.startsWith('\n')).toBe(false);
    });

    it('does not produce a trailing newline when an append layer is empty', () => {
      const empty = layer({ id: 'empty', content: '   ' });
      const result = buildSystemPrompt('BASE', [empty]);
      expect(result).toBe('BASE');
      expect(result.endsWith('\n')).toBe(false);
    });

    it('skips whitespace-only layer content', () => {
      const blank = layer({ id: 'blank', content: '\n  \n' });
      const real  = layer({ id: 'real',  content: 'Real.',  priority: 200 });
      expect(buildSystemPrompt('BASE', [blank, real])).toBe('BASE\n\nReal.');
    });
  });

  describe('does not mutate the input layers array', () => {
    it('leaves the original layers array unsorted', () => {
      const layers: PromptLayer[] = [
        layer({ id: 'z', content: 'Z', priority: 300 }),
        layer({ id: 'a', content: 'A', priority: 100 }),
      ];
      buildSystemPrompt('BASE', layers);
      expect(layers[0].id).toBe('z');
      expect(layers[1].id).toBe('a');
    });
  });
});

// =============================================================================
// Standard layer shape invariants
// =============================================================================

describe('TOOL_INSTRUCTIONS_LAYER', () => {
  it('has id "tool-instructions"', () => {
    expect(TOOL_INSTRUCTIONS_LAYER.id).toBe('tool-instructions');
  });

  it('appends at priority 100', () => {
    expect(TOOL_INSTRUCTIONS_LAYER.position).toBe('append');
    expect(TOOL_INSTRUCTIONS_LAYER.priority).toBe(100);
  });

  it('contains non-empty tool guidance text', () => {
    expect(TOOL_INSTRUCTIONS_LAYER.content.trim().length).toBeGreaterThan(0);
  });
});

describe('FORMAT_REMINDER_LAYER', () => {
  it('has id "format-reminder"', () => {
    expect(FORMAT_REMINDER_LAYER.id).toBe('format-reminder');
  });

  it('appends after tool instructions (priority 200 > 100)', () => {
    expect(FORMAT_REMINDER_LAYER.position).toBe('append');
    expect(FORMAT_REMINDER_LAYER.priority).toBe(200);
    expect(FORMAT_REMINDER_LAYER.priority).toBeGreaterThan(TOOL_INSTRUCTIONS_LAYER.priority);
  });

  it('content matches the exported FORMAT_REMINDER constant', () => {
    expect(FORMAT_REMINDER_LAYER.content).toBe(FORMAT_REMINDER);
  });
});

// =============================================================================
// Regression: tool injection works for any base prompt
// =============================================================================

describe('tool injection regression (any base prompt)', () => {
  const customPrompts = [
    'You are a helpful assistant.',               // exact DEFAULT_SYSTEM_PROMPT
    'You are a pirate assistant.',                // customised
    'You are a helpful assistant. Be concise.',   // minor edit that old exact-match would miss
    'Custom system prompt with extra context.\nMultiple lines.',
  ];

  it.each(customPrompts)(
    'injects tool instructions regardless of base prompt content: "%s"',
    (base) => {
      const result = buildSystemPrompt(base, [TOOL_INSTRUCTIONS_LAYER]);
      expect(result).toContain(base);
      expect(result).toContain(TOOL_INSTRUCTIONS_LAYER.content);
      // Tool instructions come AFTER the base prompt.
      expect(result.indexOf(base)).toBeLessThan(result.indexOf(TOOL_INSTRUCTIONS_LAYER.content));
    },
  );
});

// =============================================================================
// injectPromptLayers
// =============================================================================

describe('injectPromptLayers', () => {
  const toolLayer = layer({ id: 'tool', content: 'Use tools.' });

  describe('finds and modifies the first system message', () => {
    it('composes layers into the existing system message content', () => {
      const messages = [
        { role: 'system', content: 'You are helpful.' },
        { role: 'user',   content: 'Hello!' },
      ];
      const result = injectPromptLayers(messages, [toolLayer]);
      expect(result[0].content).toBe('You are helpful.\n\nUse tools.');
    });

    it('leaves all non-system messages completely unchanged', () => {
      const messages = [
        { role: 'system',    content: 'Base.' },
        { role: 'user',      content: 'Hello!' },
        { role: 'assistant', content: 'Hi there.' },
      ];
      const result = injectPromptLayers(messages, [toolLayer]);
      // Structural identity for non-system messages.
      expect(result[1]).toEqual({ role: 'user',      content: 'Hello!' });
      expect(result[2]).toEqual({ role: 'assistant', content: 'Hi there.' });
      expect(result).toHaveLength(3);
    });
  });

  describe('creates a system message when none exists', () => {
    it('prepends a new system message built from the layers alone', () => {
      const messages = [
        { role: 'user',      content: 'Hello!' },
        { role: 'assistant', content: 'Hi.' },
      ];
      const result = injectPromptLayers(messages, [toolLayer]);
      expect(result).toHaveLength(3);
      expect(result[0]).toEqual({ role: 'system', content: 'Use tools.' });
      expect(result[1]).toEqual({ role: 'user',   content: 'Hello!' });
      expect(result[2]).toEqual({ role: 'assistant', content: 'Hi.' });
    });
  });

  describe('immutability', () => {
    it('always returns a new array', () => {
      const messages = [{ role: 'system', content: 'Base.' }];
      const result = injectPromptLayers(messages, [toolLayer]);
      expect(result).not.toBe(messages);
    });

    it('does not mutate the original system message object', () => {
      const original = { role: 'system', content: 'Original.' };
      const messages = [original, { role: 'user', content: 'Hi.' }];
      injectPromptLayers(messages, [toolLayer]);
      // The original object reference must be untouched.
      expect(original.content).toBe('Original.');
    });

    it('returns a defensive clone when layers array is empty', () => {
      const messages = [{ role: 'system', content: 'Base.' }];
      const result = injectPromptLayers(messages, []);
      expect(result).not.toBe(messages);
      // Content unchanged with no layers.
      expect(result[0].content).toBe('Base.');
    });
  });

  describe('multiple system messages (edge case)', () => {
    it('targets only the first system message', () => {
      const sys1 = { role: 'system', content: 'First system.' };
      const sys2 = { role: 'system', content: 'Second system.' };
      const user = { role: 'user',   content: 'Hello.' };
      const messages = [sys1, sys2, user];

      const result = injectPromptLayers(messages, [toolLayer]);

      // First system message gets the injected layer.
      expect(result[0].content).toBe('First system.\n\nUse tools.');
      // Second system message is completely untouched — same content, same shape.
      expect(result[1].content).toBe('Second system.');
      expect(result[1]).toEqual({ role: 'system', content: 'Second system.' });
      // Non-system message is untouched too.
      expect(result[2]).toEqual({ role: 'user', content: 'Hello.' });
    });
  });
});
