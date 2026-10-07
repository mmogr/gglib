/**
 * The one collapsible JSON block, as its two callers use it.
 *
 * A tool call's card shows its arguments through it, labelled and collapsed;
 * the fallback result renderer shows a result through it, unlabelled and open.
 */

import { describe, it, expect } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { JsonViewer, fallbackRenderer } from '../../../../src/services/tools/renderers';

describe('JsonViewer', () => {
  it('labelled and collapsed shows the label and a one-line preview, and opens to the whole value', () => {
    const args = { city: 'London', days: 3 };
    render(<JsonViewer data={args} label="Arguments" defaultExpanded={false} />);

    const toggle = screen.getByRole('button', { name: /Arguments/ });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    // Collapsed, the preview is the start of the pretty-printed value.
    expect(toggle).toHaveTextContent('"city": "London"');
    expect(document.querySelector('pre')).toBeNull();

    fireEvent.click(toggle);

    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(document.querySelector('pre')).toHaveTextContent(JSON.stringify(args, null, 2), {
      normalizeWhitespace: false,
    });
  });

  it('cuts a long collapsed preview at fifty characters', () => {
    render(<JsonViewer data={{ text: 'x'.repeat(200) }} label="Arguments" />);

    const preview = JSON.stringify({ text: 'x'.repeat(200) }, null, 2).substring(0, 50);
    expect(screen.getByRole('button', { name: /Arguments/ })).toHaveTextContent(`${preview}...`, {
      normalizeWhitespace: false,
    });
  });

  it('shows a value that is not an object inline, after its label', () => {
    const { container } = render(<JsonViewer data={42} label="Count" />);

    expect(container).toHaveTextContent('Count:42');
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('as the fallback renderer draws a result, is open and carries no label', () => {
    const result = { ok: true };
    render(<>{fallbackRenderer.renderResult(result, 'any_tool')}</>);

    const toggle = screen.getByRole('button');
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    // Open and unlabelled, the toggle is its chevron and nothing else.
    expect(toggle.textContent).toBe('');
    expect(document.querySelector('pre')).toHaveTextContent(JSON.stringify(result, null, 2), {
      normalizeWhitespace: false,
    });
  });
});
