import { describe, it, expect } from 'vitest';
import {
  looksLikeMarkdown,
  isArrayOfHomogeneousObjects,
  mcpGenericRenderer,
} from '../../../../src/services/tools/renderers/McpGenericRenderer';
import { createMcpSchemaRenderer } from '../../../../src/services/tools/renderers/McpSchemaRenderer';

// =============================================================================
// looksLikeMarkdown
// =============================================================================

describe('looksLikeMarkdown', () => {
  it('returns false for an empty string', () => {
    expect(looksLikeMarkdown('')).toBe(false);
  });

  it('returns false for a string ≤ 20 characters even with markdown syntax', () => {
    // "# Short" has a heading but is only 7 chars
    expect(looksLikeMarkdown('# Short')).toBe(false);
    // "**bold** text here!!" is exactly 20 chars
    expect(looksLikeMarkdown('**bold** text here!!')).toBe(false);
  });

  it('returns false for a long plain-text string with no markdown patterns', () => {
    const plain = 'This is a completely plain sentence with no formatting at all, just words.';
    expect(looksLikeMarkdown(plain)).toBe(false);
  });

  it('returns false for a long string matching only one markdown pattern', () => {
    // Only bold — not enough on its own
    const onlyBold = 'This sentence is quite long and contains only **one** bold marker without anything else.';
    expect(looksLikeMarkdown(onlyBold)).toBe(false);
  });

  it('returns true for a long string with a heading and bold text (2 patterns)', () => {
    const md = '# Chapter 1\n\nThis is a **bold** introductory paragraph about the topic.';
    expect(looksLikeMarkdown(md)).toBe(true);
  });

  it('returns true for a long string with a list and inline code (2 patterns)', () => {
    const md = 'Install the dependencies:\n\n- Run `npm install`\n- Then start the server\n';
    expect(looksLikeMarkdown(md)).toBe(true);
  });

  it('returns true for a long string with bold text and a markdown link (2 patterns)', () => {
    const md = 'Check out **this project** and read [the docs](https://example.com) for more details.';
    expect(looksLikeMarkdown(md)).toBe(true);
  });

  it('returns true for a realistic multi-section markdown document', () => {
    const md = [
      '## Results',
      '',
      '**Status**: Complete',
      '',
      '- Item one',
      '- Item two',
      '',
      'See `README.md` for more.',
    ].join('\n');
    expect(looksLikeMarkdown(md)).toBe(true);
  });
});

// =============================================================================
// isArrayOfHomogeneousObjects
// =============================================================================

describe('isArrayOfHomogeneousObjects', () => {
  it('returns false for an empty array', () => {
    expect(isArrayOfHomogeneousObjects([])).toBe(false);
  });

  it('returns false for an array of primitive numbers', () => {
    expect(isArrayOfHomogeneousObjects([1, 2, 3])).toBe(false);
  });

  it('returns false for an array of strings', () => {
    expect(isArrayOfHomogeneousObjects(['a', 'b', 'c'])).toBe(false);
  });

  it('returns false for a non-array value', () => {
    expect(isArrayOfHomogeneousObjects(null)).toBe(false);
    expect(isArrayOfHomogeneousObjects(undefined)).toBe(false);
    expect(isArrayOfHomogeneousObjects({ a: 1 })).toBe(false);
    expect(isArrayOfHomogeneousObjects('text')).toBe(false);
  });

  it('returns false for an array of objects with no shared keys', () => {
    expect(isArrayOfHomogeneousObjects([{ a: 1 }, { b: 2 }])).toBe(false);
  });

  it('returns false for a mixed array containing null alongside an object', () => {
    expect(isArrayOfHomogeneousObjects([{ a: 1 }, null])).toBe(false);
  });

  it('returns false for an array of empty objects (no keys to share)', () => {
    expect(isArrayOfHomogeneousObjects([{}, {}])).toBe(false);
  });

  it('returns true for an array of objects sharing at least one key', () => {
    // id is shared; age is absent in first object — still true
    expect(
      isArrayOfHomogeneousObjects([
        { id: 1, name: 'Alice' },
        { id: 2, age: 30 },
      ]),
    ).toBe(true);
  });

  it('returns true when all objects share the same complete key set', () => {
    expect(
      isArrayOfHomogeneousObjects([
        { id: 1, value: 'x' },
        { id: 2, value: 'y' },
        { id: 3, value: 'z' },
      ]),
    ).toBe(true);
  });

  it('returns true for a single-element array containing an object with keys', () => {
    expect(isArrayOfHomogeneousObjects([{ id: 1 }])).toBe(true);
  });
});

// =============================================================================
// mcpGenericRenderer.renderSummary
// =============================================================================

describe('mcpGenericRenderer.renderSummary', () => {
  it('delegates to the fallback renderer and returns a string for an object', () => {
    const result = mcpGenericRenderer.renderSummary!({ key: 'value' }, 'my_tool');
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('returns a string for null data without throwing', () => {
    expect(() => {
      const result = mcpGenericRenderer.renderSummary!(null, 'my_tool');
      expect(typeof result).toBe('string');
    }).not.toThrow();
  });

  it('returns a string for a circular reference without throwing', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(() => {
      const result = mcpGenericRenderer.renderSummary!(circular, 'my_tool');
      expect(typeof result).toBe('string');
    }).not.toThrow();
  });

  it('returns a string for a primitive number', () => {
    const result = mcpGenericRenderer.renderSummary!(42, 'my_tool');
    expect(typeof result).toBe('string');
    expect(result).toBe('42');
  });

  it('returns a string for an array result', () => {
    const result = mcpGenericRenderer.renderSummary!([1, 2, 3], 'my_tool');
    expect(typeof result).toBe('string');
  });
});

// =============================================================================
// createMcpSchemaRenderer
// =============================================================================

describe('createMcpSchemaRenderer', () => {
  it('returns an object with renderResult and renderSummary functions', () => {
    const renderer = createMcpSchemaRenderer({ type: 'object' });
    expect(typeof renderer.renderResult).toBe('function');
    expect(typeof renderer.renderSummary).toBe('function');
  });

  it('returns a new renderer instance each call (factory pattern)', () => {
    const schema = { type: 'object', properties: {} };
    const r1 = createMcpSchemaRenderer(schema);
    const r2 = createMcpSchemaRenderer(schema);
    expect(r1).not.toBe(r2);
  });

  it('renderSummary delegates to fallbackRenderer and returns a string', () => {
    const renderer = createMcpSchemaRenderer({ type: 'object' });
    const result = renderer.renderSummary!({ status: 'ok', count: 5 }, 'schema_tool');
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('renderSummary returns a string for null data without throwing', () => {
    const renderer = createMcpSchemaRenderer({ type: 'object' });
    expect(() => {
      const result = renderer.renderSummary!(null, 'schema_tool');
      expect(typeof result).toBe('string');
    }).not.toThrow();
  });

  it('renderResult returns a defined React element', () => {
    const renderer = createMcpSchemaRenderer({ type: 'object', properties: {} });
    const result = renderer.renderResult({ key: 'value' }, 'schema_tool');
    expect(result).not.toBeNull();
    expect(result).toBeDefined();
  });
});
