/**
 * A state is a shape plus a word, never a colour alone: each kind has its
 * own shape, the word is read beside it, and a shape standing alone still
 * has a name for a reader.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { StatusMark, type MarkKind } from '../../../src/components/status/StatusMark';

const KINDS: MarkKind[] = ['on', 'off', 'changing', 'away', 'attention', 'failed'];

describe('StatusMark', () => {
  it('draws a different shape for each kind, and names the kind on the mark', () => {
    const { container } = render(
      <>
        {KINDS.map((kind) => (
          <StatusMark key={kind} kind={kind} word={kind} />
        ))}
      </>,
    );
    const shapes = KINDS.map((kind) => {
      const mark = container.querySelector(`[data-kind="${kind}"]`);
      expect(mark).not.toBeNull();
      return mark!.firstElementChild!.outerHTML;
    });
    expect(new Set(shapes).size).toBe(KINDS.length);
  });

  it('reads as the word when one is drawn', () => {
    render(<StatusMark kind="on" word="Loaded" />);
    expect(screen.getByText('Loaded')).toBeInTheDocument();
  });

  it('standing alone, is an image named by its label', () => {
    render(<StatusMark kind="off" label="Off" />);
    expect(screen.getByRole('img', { name: 'Off' })).toBeInTheDocument();
    expect(screen.queryByText('Off')).not.toBeInTheDocument();
  });

  it('only the changing kind moves', () => {
    const { container } = render(
      <>
        {KINDS.map((kind) => (
          <StatusMark key={kind} kind={kind} word={kind} />
        ))}
      </>,
    );
    for (const kind of KINDS) {
      const shape = container.querySelector(`[data-kind="${kind}"]`)!.firstElementChild!;
      expect(shape.className.includes('animate-')).toBe(kind === 'changing');
    }
  });
});
